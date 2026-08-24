use std::sync::Mutex;
#[cfg(desktop)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(desktop)]
use std::time::Instant;

use pausio_core::{EngineError, EngineEvent, SessionCheckpoint, Settings, Snapshot, TimerEngine};
use serde::Serialize;
use tauri::AppHandle;
pub(crate) struct EngineState(pub Mutex<TimerEngine>);

/// Tracks only PausIO-generated break instance IDs while the process is
/// running. It intentionally carries no user activity or application data.
#[derive(Default)]
pub(crate) struct HistoryTracker(pub Mutex<Option<String>>);

/// Native session events are edge-triggered and do not carry a duration. Keep
/// the start time outside the portable timing core so unlock calculations use
/// a monotonic clock rather than wall time (and therefore survive DST changes).
#[cfg(desktop)]
#[derive(Default)]
pub(crate) struct SessionLockState {
    started: Mutex<Option<Instant>>,
    transition_generation: AtomicU64,
}

#[cfg(desktop)]
impl SessionLockState {
    pub(crate) fn begin_lock(&self) -> bool {
        let mut started = self
            .started
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if started.is_some() {
            return false;
        }
        *started = Some(Instant::now());
        self.transition_generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    pub(crate) fn finish_unlock(&self) -> Option<u32> {
        let started = self
            .started
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()?;
        self.transition_generation.fetch_add(1, Ordering::AcqRel);
        Some(started.elapsed().as_secs().min(u64::from(u32::MAX)) as u32)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn is_locked(&self) -> bool {
        self.started
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub(crate) fn transition_generation(&self) -> u64 {
        self.transition_generation.load(Ordering::Acquire)
    }
}

/// A panic anywhere while the engine mutex is held would otherwise poison it permanently:
/// the next `.expect()` panics too, the tick loop task dies, and — with a non-dismissible
/// break overlay — the user is locked out with no way to end the break. Recovering the
/// poisoned guard keeps the engine usable; the panic itself is still reported by the
/// unwinding thread.
pub(crate) fn lock_engine(state: &Mutex<TimerEngine>) -> std::sync::MutexGuard<'_, TimerEngine> {
    // Debug builds deliberately report a contended engine lock. This is the
    // critical diagnostic for a UI that appears frozen: it distinguishes a
    // stuck native-window/menu dispatch from an engine mutex deadlock without
    // changing release behavior or persisting any user data.
    #[cfg(debug_assertions)]
    let started = std::time::Instant::now();
    let guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    #[cfg(debug_assertions)]
    if started.elapsed() >= std::time::Duration::from_millis(250) {
        eprintln!(
            "[pausio] engine mutex wait: {} ms",
            started.elapsed().as_millis()
        );
    }
    guard
}

/// An immutable capture of everything `emit`/persistence/tray updates need,
/// taken while the engine mutex is held so the lock can be dropped before any
/// of that work runs.
///
/// INVARIANT: never hold the `EngineState` mutex across `emit`, tray menu
/// mutation, or window create/close/destroy. Those dispatch to (and, via
/// `run_item_main_thread!`/window APIs, block on) the main event loop, while
/// several `#[tauri::command]` handlers are plain `fn`s that Tauri runs
/// *on* that same main thread and that call `lock_engine` themselves. Holding
/// the mutex across a main-thread dispatch from any other thread — the 1s
/// tick loop, a session-monitor callback, a tray-triggered `spawn_blocking`
/// transition — is an AB-BA deadlock: the other thread waits for the main
/// loop to service the queued menu/window call, and the main thread (stuck
/// inside `lock_engine`) never gets back to its loop to service it. Every
/// engine mutation must capture an `EngineView`, hand it to `publish`, drop the
/// guard, and never touch the tray, windows, or the store itself — the publisher
/// thread owns all of that (see `PUBLISHER`).
#[derive(Clone)]
pub(crate) struct EngineView {
    pub snapshot: Snapshot,
    pub settings: Settings,
    pub checkpoint: SessionCheckpoint,
}
impl EngineView {
    pub(crate) fn capture(engine: &TimerEngine) -> Self {
        Self {
            snapshot: engine.snapshot(),
            settings: engine.settings().clone(),
            checkpoint: engine.checkpoint(),
        }
    }
}

/// One publication of one engine transition, as handed to the publisher thread.
pub(crate) struct PublishBatch {
    events: Vec<EngineEvent>,
    view: EngineView,
    /// Mobile-only: whether this transition is worth a new watch context.
    /// Ticks deliberately are not.
    #[cfg(mobile)]
    sync_watch: bool,
}

/// The single sanctioned consumer of engine transitions.
///
/// Everything that can dispatch to — and therefore block on — the native event
/// loop (tray menu mutation, break window create/destroy, notifications) runs
/// on exactly one dedicated thread, fed by an unbounded queue. That gives three
/// properties no lock-based scheme provided:
///
/// 1. **No lock cycle.** Producers only ever `send` on this channel, which never
///    touches the main thread. No thread that holds the engine mutex — or any
///    lock the main thread can want — ever waits on native UI dispatch, so the
///    AB-BA deadlock described on `EngineView` cannot form.
/// 2. **Exact ordering.** Producers enqueue while still holding the engine
///    guard, so queue order *is* mutation order. A break's `Started` overlay can
///    never be created after the `Ended` teardown that superseded it.
/// 3. **No pile-up.** One publisher means one outstanding main-thread trip; a
///    slow window destroy queues work instead of fanning out threads that all
///    block on the same loop.
static PUBLISHER: std::sync::OnceLock<std::sync::mpsc::Sender<PublishBatch>> =
    std::sync::OnceLock::new();

/// Starts the publisher thread. Called once from `setup`, before any transition
/// can be published; until it runs, `publish` falls back to emitting inline.
pub(crate) fn install_publisher(app: &AppHandle) {
    let (sender, receiver) = std::sync::mpsc::channel::<PublishBatch>();
    if PUBLISHER.set(sender).is_err() {
        return;
    }
    let handle = app.clone();
    let _ = std::thread::Builder::new()
        .name("pausio-publisher".into())
        .spawn(move || publisher_loop(&handle, receiver));
}

fn publisher_loop(app: &AppHandle, receiver: std::sync::mpsc::Receiver<PublishBatch>) {
    // The session checkpoint heartbeat lives here rather than in the tick loop
    // so that the newest write always wins: the publisher is the only writer and
    // it processes batches in order, which a second concurrent writer could not
    // guarantee.
    let mut last_checkpoint_write = std::time::Instant::now();
    let mut last_saved_checkpoint: Option<SessionCheckpoint> = None;
    while let Ok(batch) = receiver.recv() {
        let batch = coalesce_stale_ticks(&receiver, batch);
        crate::events::emit(app, batch.events, &batch.view);
        if last_checkpoint_write.elapsed() >= std::time::Duration::from_secs(30) {
            last_checkpoint_write = std::time::Instant::now();
            // Ignore `saved_at` when comparing: it always differs, and is not
            // itself something worth waking the disk for.
            let mut comparable = batch.view.checkpoint.clone();
            if let Some(previous) = &last_saved_checkpoint {
                comparable.saved_at = previous.saved_at;
            }
            if last_saved_checkpoint.as_ref() != Some(&comparable) {
                let _ = crate::store::persist_session(app, &batch.view.checkpoint);
                last_saved_checkpoint = Some(batch.view.checkpoint.clone());
            }
        }
        #[cfg(mobile)]
        if batch.sync_watch {
            crate::events::sync_watch_state(app, &batch.view);
        }
    }
}

/// Drops countdown-only batches that a newer batch has already superseded. If
/// the publisher ever falls behind — a slow WebView2 window destroy on Windows
/// is the realistic case — replaying every intermediate second would make the
/// visible countdown lag further and further behind the engine. A `Tick` carries
/// no state change, no history, and no persistence, so the newest one is the
/// only one worth delivering. Batches containing anything else are never
/// dropped.
fn coalesce_stale_ticks(
    receiver: &std::sync::mpsc::Receiver<PublishBatch>,
    mut batch: PublishBatch,
) -> PublishBatch {
    while batch
        .events
        .iter()
        .all(|event| matches!(event, EngineEvent::Tick(_)))
    {
        match receiver.try_recv() {
            Ok(next) => batch = next,
            Err(_) => break,
        }
    }
    batch
}

/// Hands one transition to the publisher thread. Must be called while the engine
/// guard is still held, so that queue order matches mutation order; the send
/// itself is non-blocking and touches no other lock.
#[cfg(mobile)]
pub(crate) fn publish(
    app: &AppHandle,
    events: Vec<EngineEvent>,
    view: EngineView,
    sync_watch: bool,
) {
    let batch = PublishBatch {
        events,
        view,
        sync_watch,
    };
    let Some(sender) = PUBLISHER.get() else {
        // Pre-`setup` (or after the publisher thread has gone): emit inline. The
        // callers that can reach this are already off the main thread.
        crate::events::emit(app, batch.events, &batch.view);
        return;
    };
    if let Err(rejected) = sender.send(batch) {
        crate::events::emit(app, rejected.0.events, &rejected.0.view);
    }
}

#[cfg(not(mobile))]
pub(crate) fn publish(app: &AppHandle, events: Vec<EngineEvent>, view: EngineView) {
    let batch = PublishBatch { events, view };
    let Some(sender) = PUBLISHER.get() else {
        // Pre-`setup` (or after the publisher thread has gone): emit inline. The
        // callers that can reach this are already off the main thread.
        crate::events::emit(app, batch.events, &batch.view);
        return;
    };
    if let Err(rejected) = sender.send(batch) {
        crate::events::emit(app, rejected.0.events, &rejected.0.view);
    }
}

/// The only sanctioned way to publish engine transitions: takes ownership of the
/// still-held engine guard, captures a lock-free view, queues publication, and
/// only then drops the guard. A command response must never wait for window or
/// tray dispatch — those operations can synchronously require the native event
/// loop that is delivering the command.
pub(crate) fn drain_and_emit(
    app: &AppHandle,
    guard: std::sync::MutexGuard<'_, TimerEngine>,
    events: Vec<EngineEvent>,
) -> EngineView {
    let view = EngineView::capture(&guard);
    #[cfg(mobile)]
    publish(app, events, view.clone(), true);
    #[cfg(not(mobile))]
    publish(app, events, view.clone());
    drop(guard);
    view
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    pub code: &'static str,
    pub message: String,
    /// Which settings field failed validation, as a stable locale-independent slug (e.g.
    /// "fixed_breaks"). Present only for `invalid_settings`; UI clients use this to resolve a
    /// translated message instead of showing `message` (which is only ever English) to the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
}
impl From<EngineError> for ApiError {
    fn from(error: EngineError) -> Self {
        let (code, field) = match &error {
            EngineError::Settings(settings_error) => {
                ("invalid_settings", Some(settings_error.field()))
            }
            EngineError::InvalidTransition => ("invalid_transition", None),
        };
        Self {
            code,
            message: error.to_string(),
            field,
        }
    }
}
pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub(crate) fn internal_error(error: impl std::fmt::Display) -> ApiError {
    ApiError {
        code: "internal",
        message: error.to_string(),
        field: None,
    }
}

#[cfg(mobile)]
pub(crate) fn platform_unavailable(error: impl std::fmt::Display) -> ApiError {
    ApiError {
        code: "platform_unavailable",
        message: error.to_string(),
        field: None,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(desktop)]
    use super::SessionLockState;
    use super::{EngineView, PublishBatch, coalesce_stale_ticks};
    use chrono::Local;
    use pausio_core::{EngineEvent, Settings, TimerEngine};
    use pausio_protocol::TimerPhase;

    fn batch(events: Vec<EngineEvent>) -> PublishBatch {
        let engine = TimerEngine::new(Settings::default(), Local::now()).unwrap();
        PublishBatch {
            events,
            view: EngineView::capture(&engine),
            #[cfg(mobile)]
            sync_watch: false,
        }
    }

    #[test]
    fn a_backlog_of_countdown_batches_publishes_only_the_newest() {
        let (sender, receiver) = std::sync::mpsc::channel();
        for remaining in [19, 18, 17] {
            sender
                .send(batch(vec![EngineEvent::Tick(remaining)]))
                .unwrap();
        }
        let first = receiver.recv().unwrap();

        let coalesced = coalesce_stale_ticks(&receiver, first);

        assert_eq!(coalesced.events, vec![EngineEvent::Tick(17)]);
        assert!(receiver.try_recv().is_err());
    }

    /// The whole point of the publisher queue: a break's `Started` (overlay
    /// creation) and `Ended` (teardown) must both reach `emit`, in order. A
    /// coalescing rule that dropped them would leave the shield on screen.
    #[test]
    fn state_carrying_batches_are_never_coalesced_away() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(batch(vec![
                EngineEvent::Tick(20),
                EngineEvent::StateChanged(TimerPhase::Working),
            ]))
            .unwrap();
        sender.send(batch(vec![EngineEvent::Tick(19)])).unwrap();
        let first = receiver.recv().unwrap();

        let coalesced = coalesce_stale_ticks(&receiver, first);

        assert_eq!(coalesced.events.len(), 2);
        assert_eq!(receiver.recv().unwrap().events, vec![EngineEvent::Tick(19)]);
    }

    #[cfg(desktop)]
    #[test]
    fn session_lock_generation_changes_only_for_real_edges() {
        let state = SessionLockState::default();
        assert_eq!(state.transition_generation(), 0);

        assert!(state.begin_lock());
        assert_eq!(state.transition_generation(), 1);

        assert!(!state.begin_lock());
        assert_eq!(state.transition_generation(), 1);

        assert!(state.finish_unlock().is_some());
        assert_eq!(state.transition_generation(), 2);

        assert!(state.finish_unlock().is_none());
        assert_eq!(state.transition_generation(), 2);
    }
}
