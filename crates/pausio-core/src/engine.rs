use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};
use pausio_protocol::{BreakKind, ContextReason, PauseReason, TimerPhase};

use crate::settings::{DisplayTarget, Settings, Strictness};
use crate::types::{EngineError, EngineEvent, SESSION_SCHEMA_VERSION, SessionCheckpoint, Snapshot};

/// How long a break may sit `Due` under Gentle/notification-only delivery,
/// with no active context, before PausIO starts it anyway. Long enough that
/// a person who is mid-thought is not interrupted the instant a notification
/// appears; bounded enough that the break is guaranteed to actually happen.
pub(crate) const GENTLE_DUE_GRACE_SECONDS: u32 = 3 * 60;
/// Balanced delivery advertises "prompt then overlay": the prompt gets the
/// first word, and this is how long it keeps it before the overlay takes
/// over. Deliberately short — the prompt is a courtesy, not a veto.
pub(crate) const BALANCED_DUE_GRACE_SECONDS: u32 = 30;

/// How long a due break waits for a person to act before starting itself.
///
/// Notification-only delivery has no other surface to fall back to, so it keeps
/// the long grace regardless of reminder style. Otherwise the style's own
/// promise sets the length: Gentle is "notifications only" and stays unhurried;
/// Balanced is "prompt then overlay" and hands over quickly; Firm and Strict are
/// "fullscreen reminder", where waiting to be acknowledged contradicts the
/// point — they raise the overlay on the next tick.
///
/// Public because the shell needs it too: it is exactly how long a break-due
/// notification's buttons stay meaningful, and a banner left actionable past
/// this point is a control that no longer matches the timer's state.
pub fn due_grace_seconds(settings: &Settings) -> u32 {
    if settings.display_target == DisplayTarget::NotificationOnly {
        return GENTLE_DUE_GRACE_SECONDS;
    }
    match settings.strictness {
        Strictness::Gentle => GENTLE_DUE_GRACE_SECONDS,
        Strictness::Balanced => BALANCED_DUE_GRACE_SECONDS,
        Strictness::Firm | Strictness::Strict => 0,
    }
}
#[derive(Debug, Clone)]
pub struct TimerEngine {
    pub(crate) settings: Settings,
    pub(crate) phase: TimerPhase,
    pub(crate) remaining: u32,
    pub(crate) completed_breaks: u32,
    pub(crate) completed_short_breaks: u32,
    pub(crate) postpones_today: u8,
    pub(crate) manual_session: bool,
    pub(crate) lock_context: Option<LockContext>,
    pub(crate) context: Option<ContextReason>,
    pub(crate) context_expires_at: Option<DateTime<Utc>>,
    pub(crate) local_day: NaiveDate,
    pub(crate) paused_until: Option<DateTime<Utc>>,
    pub(crate) automatic_deferrals_today: u8,
    pub(crate) fixed_breaks_seen_today: Vec<u16>,
    pub(crate) work_seconds_today: u32,
    pub(crate) blink_remaining: Option<u32>,
    pub(crate) posture_remaining: Option<u32>,
    pub(crate) hydration_remaining: Option<u32>,
    /// Counts down while a break sits `Due` with no active context, then the
    /// break starts on its own. This is the engine's guarantee that a break
    /// never depends on an acknowledgement that may never arrive; see
    /// [`due_grace_seconds`] for how long each delivery style
    /// waits. `None` outside `Due`, and deliberately not persisted across a
    /// restart — a fresh grace on relaunch is the safe direction to err.
    pub(crate) due_grace_remaining: Option<u32>,
}

/// The visible state switches to `Paused(ScreenLock)` while the display is
/// locked, but the engine must retain the state that existed immediately before
/// the lock. The native shell measures elapsed time with a monotonic clock and
/// provides it when the session becomes active again, allowing work time to be
/// consumed exactly once without surfacing a stale break prompt on unlock.
#[derive(Debug, Clone)]
pub(crate) struct LockContext {
    pub(crate) phase: TimerPhase,
    pub(crate) remaining: u32,
    pub(crate) paused_until: Option<DateTime<Utc>>,
}

/// Supplies the two clocks the product uses. Monotonic seconds drive elapsed
/// intervals; the local wall clock is consulted only for calendar boundaries.
pub trait TimerClock {
    fn monotonic_seconds(&self) -> u64;
    fn wall_now(&self) -> DateTime<Local>;
}

/// A testable polling adapter for native schedulers. It never asks JavaScript
/// to calculate elapsed time and the engine emits at most one visible tick for
/// each poll, even when a native run loop wakes up late.
#[derive(Debug)]
pub struct TimerDriver<C> {
    clock: C,
    last_monotonic_seconds: u64,
}

impl<C: TimerClock> TimerDriver<C> {
    pub fn new(clock: C) -> Self {
        let last_monotonic_seconds = clock.monotonic_seconds();
        Self {
            clock,
            last_monotonic_seconds,
        }
    }

    pub fn poll(&mut self, engine: &mut TimerEngine) -> Vec<EngineEvent> {
        let now = self.clock.monotonic_seconds();
        let elapsed = now.saturating_sub(self.last_monotonic_seconds);
        self.last_monotonic_seconds = now;
        if elapsed == 0 {
            return vec![];
        }
        engine.advance(elapsed.min(u32::MAX as u64) as u32, self.clock.wall_now())
    }

    pub fn clock_mut(&mut self) -> &mut C {
        &mut self.clock
    }
}

impl TimerEngine {
    pub fn new(settings: Settings, now: DateTime<Local>) -> Result<Self, EngineError> {
        settings.validate()?;
        let active = settings.active_at(now);
        let blink_remaining = settings
            .blink_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        let posture_remaining = settings
            .posture_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        let hydration_remaining = settings
            .hydration_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        Ok(Self {
            remaining: settings.work_seconds,
            phase: if active {
                TimerPhase::Working
            } else {
                TimerPhase::Dormant
            },
            settings,
            completed_breaks: 0,
            completed_short_breaks: 0,
            postpones_today: 0,
            manual_session: false,
            lock_context: None,
            context: None,
            context_expires_at: None,
            local_day: now.date_naive(),
            paused_until: None,
            automatic_deferrals_today: 0,
            fixed_breaks_seen_today: vec![],
            work_seconds_today: 0,
            blink_remaining,
            posture_remaining,
            hydration_remaining,
            due_grace_remaining: None,
        })
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase.clone(),
            remaining_seconds: self.remaining,
            completed_short_breaks: self.completed_short_breaks,
            postpones_today: self.postpones_today,
            context: self.context.clone(),
            context_expires_at: self.context_expires_at,
            paused_until: self.paused_until,
        }
    }
    pub fn checkpoint(&self) -> SessionCheckpoint {
        SessionCheckpoint {
            schema_version: SESSION_SCHEMA_VERSION,
            saved_at: Utc::now(),
            local_day: self.local_day,
            phase: self.phase.clone(),
            remaining_seconds: self.remaining,
            completed_breaks: self.completed_breaks,
            completed_short_breaks: self.completed_short_breaks,
            postpones_today: self.postpones_today,
            manual_session: self.manual_session,
            // Context is intentionally transient. A meeting or screen-share
            // signal must be freshly observed after restart instead of being
            // retained as a stale reason to suppress reminders forever.
            context: None,
            paused_until: self.paused_until,
            automatic_deferrals_today: self.automatic_deferrals_today,
            fixed_breaks_seen_today: self.fixed_breaks_seen_today.clone(),
            work_seconds_today: self.work_seconds_today,
        }
    }
    /// Restores only valid, schema-compatible state. A relaunch always starts
    /// a fresh, full work interval rather than replaying elapsed wall time —
    /// otherwise quitting the app for longer than the work interval would
    /// drop the user straight into a break-due prompt the moment they reopen
    /// it. Paused and context-deferred sessions retain their intent until the
    /// user or context monitor resumes them, since those are explicit holds
    /// rather than an interval quietly ticking in the background.
    pub fn restore(
        settings: Settings,
        checkpoint: SessionCheckpoint,
        now: DateTime<Local>,
    ) -> Result<(Self, Vec<EngineEvent>), EngineError> {
        settings.validate()?;
        if checkpoint.schema_version != SESSION_SCHEMA_VERSION {
            return Ok((Self::new(settings, now)?, vec![]));
        }
        let blink_remaining = settings
            .blink_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        let posture_remaining = settings
            .posture_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        let hydration_remaining = settings
            .hydration_nudge_minutes
            .map(|minutes| u32::from(minutes) * 60);
        let mut engine = Self {
            settings,
            phase: checkpoint.phase,
            remaining: checkpoint.remaining_seconds,
            completed_breaks: checkpoint.completed_breaks,
            completed_short_breaks: checkpoint.completed_short_breaks,
            postpones_today: checkpoint.postpones_today,
            manual_session: checkpoint.manual_session,
            lock_context: None,
            context: None,
            context_expires_at: None,
            local_day: checkpoint.local_day,
            paused_until: checkpoint.paused_until,
            automatic_deferrals_today: checkpoint.automatic_deferrals_today,
            fixed_breaks_seen_today: checkpoint.fixed_breaks_seen_today,
            work_seconds_today: checkpoint.work_seconds_today,
            blink_remaining,
            posture_remaining,
            hydration_remaining,
            due_grace_remaining: None,
        };
        let rolled_over = engine.rollover_day(now);
        // BreakDue is included: quitting while a break prompt was showing must
        // not resurrect that stale prompt on the next launch — it is the same
        // "drop the user straight into a break-due state" symptom this reset
        // exists to prevent.
        let resumable = matches!(
            engine.phase,
            TimerPhase::Working
                | TimerPhase::PreBreak
                | TimerPhase::BreakDue { .. }
                | TimerPhase::Breaking { .. }
        );
        // A checkpoint written while the daily focus limit was exhausted must not
        // carry that pause into a new day, which has a fresh allowance — otherwise
        // quitting at the limit reinstates it on every later launch, and the
        // rollover handling in `advance` cannot help because `rollover_day` above
        // has already moved `local_day` on. Inside the same day the pause is still
        // correct, so this is gated on the rollover having actually happened.
        let stale_daily_limit = rolled_over
            && matches!(
                engine.phase,
                TimerPhase::Paused {
                    reason: PauseReason::DailyLimit
                }
            );
        if resumable || stale_daily_limit {
            engine.phase = if engine.settings.active_at(now) {
                TimerPhase::Working
            } else {
                TimerPhase::Dormant
            };
            engine.remaining = engine.settings.work_seconds;
        }
        let events = engine.state_and_tick();
        Ok((engine, events))
    }
    pub fn replace_settings(
        &mut self,
        settings: Settings,
        now: DateTime<Local>,
    ) -> Result<Vec<EngineEvent>, EngineError> {
        settings.validate()?;
        // A nudge countdown has to survive a save that did not touch it. These were
        // recomputed from scratch on every write, so changing an accent colour — or
        // simply sitting in Settings with autosave running — restarted all three.
        // With the blink interval at ten minutes, ten minutes of adjusting settings
        // meant the nudge never arrived at all.
        let previous_nudges = (
            self.settings.blink_nudge_minutes,
            self.settings.posture_nudge_minutes,
            self.settings.hydration_nudge_minutes,
        );
        // Shortening "time between breaks" mid-interval used to have no effect
        // until the break that was already running finished -- someone who just
        // asked for a shorter workday kept waiting on the interval they had before
        // they asked. Lengthening it is left alone: the current countdown keeps
        // the deadline it already promised, and the longer interval simply starts
        // applying from the next one, which is the least surprising direction to
        // err in and needs no special handling here.
        let previous_work_seconds = self.settings.work_seconds;
        let mid_interval = matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak);
        self.settings = settings;
        let mut clamped_tick = None;
        if mid_interval && self.settings.work_seconds < previous_work_seconds {
            let elapsed = previous_work_seconds.saturating_sub(self.remaining);
            self.remaining = self.settings.work_seconds.saturating_sub(elapsed);
            // The rest of this function returns `Ok(vec![])` on this path: without
            // an explicit Tick, the displayed countdown would sit on its old value
            // until the next ordinary poll caught up, which could be a full second
            // or more of showing a number that no longer matches the engine.
            clamped_tick = Some(EngineEvent::Tick(self.remaining));
        }
        if matches!(self.phase, TimerPhase::Dormant) && self.settings.active_at(now) {
            self.phase = TimerPhase::Working;
            self.remaining = self.settings.work_seconds;
            return Ok(vec![
                EngineEvent::StateChanged(self.phase.clone()),
                EngineEvent::Tick(self.remaining),
            ]);
        }
        if !self.manual_session
            && !self.settings.active_at(now)
            && matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak)
        {
            self.phase = TimerPhase::Dormant;
            self.remaining = self.settings.work_seconds;
            return Ok(vec![
                EngineEvent::StateChanged(self.phase.clone()),
                EngineEvent::Tick(self.remaining),
            ]);
        }
        if matches!(self.phase, TimerPhase::Dormant) {
            self.remaining = self.settings.work_seconds;
        }
        // Only re-arm the countdowns whose interval actually changed; an interval a
        // person did not edit keeps the progress it had already made.
        let countdown = |minutes: Option<u8>| minutes.map(|m| u32::from(m) * 60);
        if previous_nudges.0 != self.settings.blink_nudge_minutes {
            self.blink_remaining = countdown(self.settings.blink_nudge_minutes);
        }
        if previous_nudges.1 != self.settings.posture_nudge_minutes {
            self.posture_remaining = countdown(self.settings.posture_nudge_minutes);
        }
        if previous_nudges.2 != self.settings.hydration_nudge_minutes {
            self.hydration_remaining = countdown(self.settings.hydration_nudge_minutes);
        }
        Ok(clamped_tick.into_iter().collect())
    }
    pub fn advance(&mut self, seconds: u32, now: DateTime<Local>) -> Vec<EngineEvent> {
        let new_day = self.rollover_day(now);
        // Reaching the daily focus limit parks the phase in `Paused { DailyLimit }`,
        // and nothing used to bring it back: the guard further down early-returns
        // for every phase outside Working/PreBreak/Breaking, `activity_resumed`
        // un-pauses only Idle and Sleep, `paused_until` is never set for this
        // reason, and both `start_session` and `take_break_now` require phases that
        // are no longer reachable. Resetting `work_seconds_today` on rollover was
        // therefore not enough — one day at the limit silently stopped every day
        // after it, until somebody noticed and pressed Resume by hand. A new day
        // has a fresh allowance, so the pause has to end with it.
        if new_day
            && matches!(
                self.phase,
                TimerPhase::Paused {
                    reason: PauseReason::DailyLimit
                }
            )
        {
            self.phase = if self.settings.active_at(now) {
                TimerPhase::Working
            } else {
                TimerPhase::Dormant
            };
            self.remaining = self.settings.work_seconds;
            return self.state_and_tick();
        }
        if self
            .context_expires_at
            .is_some_and(|deadline| now.with_timezone(&Utc) >= deadline)
        {
            // A timed quiet period must never turn into a forgotten, permanent
            // suppression. Clearing through the normal path also immediately
            // surfaces an already-due break.
            return self.set_context(None);
        }
        if self
            .paused_until
            .is_some_and(|deadline| now.with_timezone(&Utc) >= deadline)
        {
            self.paused_until = None;
            if matches!(
                self.phase,
                TimerPhase::Paused {
                    reason: PauseReason::Manual
                }
            ) {
                self.phase = TimerPhase::Working;
                return self.state_and_tick();
            }
        }
        if !self.manual_session
            && !self.settings.active_at(now)
            && matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak)
        {
            self.phase = TimerPhase::Dormant;
            self.remaining = self.settings.work_seconds;
            return vec![
                EngineEvent::StateChanged(self.phase.clone()),
                EngineEvent::Tick(self.remaining),
            ];
        }
        if matches!(self.phase, TimerPhase::Dormant) && self.settings.active_at(now) {
            self.phase = TimerPhase::Working;
            self.remaining = self.settings.work_seconds;
            return vec![
                EngineEvent::StateChanged(self.phase.clone()),
                EngineEvent::Tick(self.remaining),
            ];
        }
        if matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak) {
            let minute = (now.hour() * 60 + now.minute()) as u16;
            if self.settings.fixed_break_minutes.contains(&minute)
                && !self.fixed_breaks_seen_today.contains(&minute)
            {
                self.fixed_breaks_seen_today.push(minute);
                let kind = BreakKind::Short;
                self.phase = TimerPhase::BreakDue { kind: kind.clone() };
                self.remaining = 0;
                let mut events = if let Some(reason) = self.context.clone() {
                    vec![EngineEvent::ContextDeferred { kind, reason }]
                } else {
                    vec![EngineEvent::Due(kind)]
                };
                events.push(EngineEvent::StateChanged(self.phase.clone()));
                return events;
            }
        }
        if let TimerPhase::BreakDue { kind } = self.phase.clone() {
            // Every delivery style gets a bounded grace period and then the
            // break starts on its own. Nothing outside the engine is allowed
            // to be the *only* way out of `Due`: the shell's prompt and the
            // OS notification can both fail to reach a person — a denied or
            // unregistered notification permission, a dismissed banner, a
            // Focus filter, an unclicked prompt — and a break that waits
            // forever for an acknowledgement nobody gives is a timer that
            // silently stops working. The grace length is what differs by
            // style, not whether the guarantee exists.
            if self.context.is_none() {
                let grace = due_grace_seconds(&self.settings);
                let remaining_grace = self.due_grace_remaining.get_or_insert(grace);
                if seconds >= *remaining_grace {
                    let mut events = vec![];
                    self.start_break(kind, &mut events);
                    return events;
                }
                *remaining_grace -= seconds;
            }
            return vec![];
        }
        if !matches!(
            self.phase,
            TimerPhase::Working | TimerPhase::PreBreak | TimerPhase::Breaking { .. }
        ) {
            return vec![];
        }
        let blink_during_work = matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak);
        if blink_during_work {
            if let Some(limit_minutes) = self.settings.daily_focus_limit_minutes {
                let limit_seconds = u32::from(limit_minutes) * 60;
                let available = limit_seconds.saturating_sub(self.work_seconds_today);
                if available == 0 {
                    self.phase = TimerPhase::Paused {
                        reason: PauseReason::DailyLimit,
                    };
                    return vec![EngineEvent::StateChanged(self.phase.clone())];
                }
                if seconds >= available {
                    self.work_seconds_today = limit_seconds;
                    self.remaining = self.remaining.saturating_sub(available);
                    self.phase = TimerPhase::Paused {
                        reason: PauseReason::DailyLimit,
                    };
                    return vec![
                        EngineEvent::Tick(self.remaining),
                        EngineEvent::StateChanged(self.phase.clone()),
                    ];
                }
            }
            self.work_seconds_today = self.work_seconds_today.saturating_add(seconds);
        }
        self.remaining = self.remaining.saturating_sub(seconds);
        let mut events = vec![EngineEvent::Tick(self.remaining)];
        if blink_during_work && let Some(remaining) = self.blink_remaining {
            if seconds >= remaining {
                events.push(EngineEvent::BlinkNudge);
                self.blink_remaining = self
                    .settings
                    .blink_nudge_minutes
                    .map(|minutes| u32::from(minutes) * 60);
            } else {
                self.blink_remaining = Some(remaining - seconds);
            }
        }
        if blink_during_work && let Some(remaining) = self.posture_remaining {
            if seconds >= remaining {
                events.push(EngineEvent::PostureNudge);
                self.posture_remaining = self
                    .settings
                    .posture_nudge_minutes
                    .map(|minutes| u32::from(minutes) * 60);
            } else {
                self.posture_remaining = Some(remaining - seconds);
            }
        }
        if blink_during_work && let Some(remaining) = self.hydration_remaining {
            if seconds >= remaining {
                events.push(EngineEvent::HydrationNudge);
                self.hydration_remaining = self
                    .settings
                    .hydration_nudge_minutes
                    .map(|minutes| u32::from(minutes) * 60);
            } else {
                self.hydration_remaining = Some(remaining - seconds);
            }
        }
        match self.phase.clone() {
            TimerPhase::Working | TimerPhase::PreBreak if self.remaining == 0 => {
                let kind = self.next_kind();
                self.phase = TimerPhase::BreakDue { kind: kind.clone() };
                if let Some(reason) = self.context.clone() {
                    events.push(EngineEvent::ContextDeferred { kind, reason });
                } else {
                    events.push(EngineEvent::Due(kind));
                }
                events.push(EngineEvent::StateChanged(self.phase.clone()));
            }
            TimerPhase::Working
                if self.remaining <= self.settings.pre_break_seconds
                    && self.settings.pre_break_seconds > 0 =>
            {
                self.phase = TimerPhase::PreBreak;
                let kind = self.next_kind();
                if self.context.is_none() {
                    events.push(EngineEvent::Incoming(kind));
                }
                events.push(EngineEvent::StateChanged(self.phase.clone()));
            }
            TimerPhase::Breaking { kind } if self.remaining == 0 => {
                self.finish_break(kind, false, &mut events);
            }
            _ => {}
        }
        events
    }
    pub fn start_session(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        if !matches!(self.phase, TimerPhase::Dormant) {
            return Err(EngineError::InvalidTransition);
        }
        self.manual_session = true;
        self.phase = TimerPhase::Working;
        self.remaining = self.settings.work_seconds;
        Ok(vec![
            EngineEvent::StateChanged(self.phase.clone()),
            EngineEvent::Tick(self.remaining),
        ])
    }
    pub fn take_break_now(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        if !matches!(
            self.phase,
            TimerPhase::Dormant | TimerPhase::Working | TimerPhase::PreBreak
        ) {
            return Err(EngineError::InvalidTransition);
        }
        if matches!(self.phase, TimerPhase::Dormant) {
            self.manual_session = true;
        }
        let mut events = vec![];
        self.start_break(self.next_kind(), &mut events);
        Ok(events)
    }
    pub fn start_due_break(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        let TimerPhase::BreakDue { kind } = self.phase.clone() else {
            return Err(EngineError::InvalidTransition);
        };
        let mut events = vec![];
        self.start_break(kind, &mut events);
        Ok(events)
    }
    /// Ends the active break early by explicit person action (the overlay's
    /// "I'm back" / emergency-exit control). Use [`Self::complete_break_from_absence`]
    /// instead when a break's duration elapsed while the person was away —
    /// that is a natural completion, not a skip.
    pub fn skip_break(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        let TimerPhase::Breaking { kind } = self.phase.clone() else {
            return Err(EngineError::InvalidTransition);
        };
        let mut events = vec![];
        self.finish_break(kind, true, &mut events);
        Ok(events)
    }
    /// Ends the active break because its duration elapsed while the person
    /// was away (idle or asleep) rather than because they dismissed it. This
    /// is a natural completion, so it is reported the same way as a break
    /// that ran its course under `advance`.
    fn complete_break_from_absence(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        let TimerPhase::Breaking { kind } = self.phase.clone() else {
            return Err(EngineError::InvalidTransition);
        };
        let mut events = vec![];
        self.finish_break(kind, false, &mut events);
        Ok(events)
    }
    pub fn postpone(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        let TimerPhase::BreakDue { kind } = self.phase.clone() else {
            return Err(EngineError::InvalidTransition);
        };
        if self
            .settings
            .postpone_limit
            .is_some_and(|max| self.postpones_today >= max)
        {
            return Err(EngineError::InvalidTransition);
        }
        self.postpones_today += 1;
        self.phase = TimerPhase::Working;
        self.remaining = 2 * 60;
        self.due_grace_remaining = None;
        Ok(vec![
            EngineEvent::Postponed(kind),
            EngineEvent::StateChanged(self.phase.clone()),
            EngineEvent::Tick(self.remaining),
        ])
    }
    /// Defers a due break for a brief natural pause, such as an active typing
    /// burst. Unlike a user-selected postpone this does not consume the
    /// postpone allowance, but is capped at four occurrences per local day.
    pub fn defer_due_for_active_input(
        &mut self,
        seconds: u32,
    ) -> Result<Vec<EngineEvent>, EngineError> {
        let TimerPhase::BreakDue { kind } = self.phase.clone() else {
            return Err(EngineError::InvalidTransition);
        };
        if self.automatic_deferrals_today >= 4 || seconds == 0 {
            return Err(EngineError::InvalidTransition);
        }
        self.automatic_deferrals_today += 1;
        self.phase = TimerPhase::Working;
        self.remaining = seconds;
        Ok(vec![
            EngineEvent::ContextDeferred {
                kind,
                reason: ContextReason::ActiveInput,
            },
            EngineEvent::StateChanged(self.phase.clone()),
            EngineEvent::Tick(self.remaining),
        ])
    }
    /// Updates a transient context signal. If a break became due while PausIO
    /// was respectfully quiet, surface it immediately after the context ends.
    pub fn set_context(&mut self, context: Option<ContextReason>) -> Vec<EngineEvent> {
        if self.context == context {
            return vec![];
        }
        self.context = context;
        self.context_expires_at = None;
        let mut events = vec![EngineEvent::StateChanged(self.phase.clone())];
        if self.context.is_none()
            && let TimerPhase::BreakDue { kind } = self.phase.clone()
        {
            events.insert(0, EngineEvent::Due(kind));
        }
        events
    }
    /// Applies a local-only interruption context that automatically clears.
    /// The caller supplies a bounded duration, never calendar or application
    /// metadata, so the core only carries a reason and expiry timestamp.
    pub fn set_context_for(
        &mut self,
        context: ContextReason,
        minutes: u16,
    ) -> Result<Vec<EngineEvent>, EngineError> {
        if !(1..=24 * 60).contains(&minutes) {
            return Err(EngineError::InvalidTransition);
        }
        self.context = Some(context);
        self.context_expires_at = Some(Utc::now() + chrono::Duration::minutes(i64::from(minutes)));
        Ok(vec![EngineEvent::StateChanged(self.phase.clone())])
    }
    /// Returning to the keyboard resumes only system-managed pauses. A manual
    /// pause always remains a deliberate user choice.
    pub fn activity_resumed(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        if matches!(
            self.phase,
            TimerPhase::Paused {
                reason: PauseReason::Idle | PauseReason::Sleep
            }
        ) {
            return self.resume();
        }
        Ok(vec![])
    }
    pub fn pause(&mut self, reason: PauseReason) -> Result<Vec<EngineEvent>, EngineError> {
        if !matches!(self.phase, TimerPhase::Working | TimerPhase::PreBreak) {
            return Err(EngineError::InvalidTransition);
        }
        self.phase = TimerPhase::Paused { reason };
        self.paused_until = None;
        Ok(vec![EngineEvent::StateChanged(self.phase.clone())])
    }
    pub fn pause_for(&mut self, minutes: u16) -> Result<Vec<EngineEvent>, EngineError> {
        if !(1..=24 * 60).contains(&minutes)
            || !matches!(
                self.phase,
                TimerPhase::Working | TimerPhase::PreBreak | TimerPhase::BreakDue { .. }
            )
        {
            return Err(EngineError::InvalidTransition);
        }
        // A timed pause chosen while a break is due restarts the gentle-due
        // grace countdown when the pause expires and the break resurfaces.
        // Note the two exit paths differ deliberately: letting the pause run
        // out re-surfaces the due break (`remaining` stays 0, so the next
        // tick re-enters `BreakDue` and re-emits `Due`), while an early
        // manual `resume()` is treated as "break taken" and restarts a full
        // work interval — without consuming the postpone allowance or
        // writing a history entry.
        self.due_grace_remaining = None;
        self.phase = TimerPhase::Paused {
            reason: PauseReason::Manual,
        };
        self.paused_until = Some(Utc::now() + chrono::Duration::minutes(i64::from(minutes)));
        Ok(vec![EngineEvent::StateChanged(self.phase.clone())])
    }
    pub fn resume(&mut self) -> Result<Vec<EngineEvent>, EngineError> {
        if !matches!(self.phase, TimerPhase::Paused { .. }) {
            return Err(EngineError::InvalidTransition);
        }
        self.phase = TimerPhase::Working;
        self.paused_until = None;
        if self.remaining == 0 {
            self.remaining = self.settings.work_seconds;
        }
        Ok(vec![
            EngineEvent::StateChanged(self.phase.clone()),
            EngineEvent::Tick(self.remaining),
        ])
    }
    pub fn screen_locked(&mut self) -> Vec<EngineEvent> {
        // macOS can report duplicate session-resign notifications. Retaining
        // the first context ensures a duplicate cannot overwrite the actual
        // work/break state with the transient ScreenLock pause state.
        if self.lock_context.is_some() {
            return vec![];
        }
        self.lock_context = Some(LockContext {
            phase: self.phase.clone(),
            remaining: self.remaining,
            paused_until: self.paused_until,
        });
        self.phase = TimerPhase::Paused {
            reason: PauseReason::ScreenLock,
        };
        self.paused_until = None;
        vec![EngineEvent::StateChanged(self.phase.clone())]
    }
    pub fn screen_unlocked(
        &mut self,
        locked_seconds: u32,
        now: DateTime<Local>,
    ) -> Vec<EngineEvent> {
        let Some(context) = self.lock_context.take() else {
            // An unmatched unlock must never turn a user-selected manual pause
            // into a running session.
            return vec![];
        };

        self.paused_until = context.paused_until;

        match context.phase {
            TimerPhase::Working | TimerPhase::PreBreak => {
                if !self.manual_session && !self.settings.active_at(now) {
                    self.phase = TimerPhase::Dormant;
                    self.remaining = self.settings.work_seconds;
                    self.state_and_tick()
                } else if locked_seconds >= context.remaining {
                    // Reaching the end of a work interval while the screen is
                    // locked is itself enough time away from the display. Start
                    // a new interval on unlock and never emit Due/Started, which
                    // would present a stale break prompt to a returning person.
                    self.start_fresh_work_or_dormant(now)
                } else {
                    self.remaining = context.remaining - locked_seconds;
                    self.phase = if self.settings.pre_break_seconds > 0
                        && self.remaining <= self.settings.pre_break_seconds
                    {
                        TimerPhase::PreBreak
                    } else {
                        TimerPhase::Working
                    };
                    self.state_and_tick()
                }
            }
            // The work countdown was already exhausted before the lock. The
            // locked display clears that stale due state; showing the prompt on
            // unlock would ask for more eyes-away time immediately after an
            // absence. This does not increment completed-break statistics.
            TimerPhase::BreakDue { .. } => self.start_fresh_work_or_dormant(now),
            TimerPhase::Breaking { kind } => {
                self.remaining = context.remaining.saturating_sub(locked_seconds);
                if self.remaining == 0 {
                    self.phase = TimerPhase::Breaking { kind: kind.clone() };
                    let mut events = vec![];
                    self.finish_break(kind, false, &mut events);
                    events
                } else {
                    self.phase = TimerPhase::Breaking { kind: kind.clone() };
                    vec![
                        // `Started` is also the presentation signal used to
                        // restore a partially completed overlay after unlock;
                        // it never changes completion counters.
                        EngineEvent::Started(kind),
                        EngineEvent::StateChanged(self.phase.clone()),
                        EngineEvent::Tick(self.remaining),
                    ]
                }
            }
            phase => {
                // Dormant and every non-lock pause retain their exact intent.
                // In particular, unlocking cannot silently resume a manually
                // paused timer.
                self.phase = phase;
                self.remaining = context.remaining;
                self.state_and_tick()
            }
        }
    }
    pub fn report_idle(&mut self, seconds: u32) -> Result<Vec<EngineEvent>, EngineError> {
        if seconds >= 15 * 60 {
            self.phase = TimerPhase::Paused {
                reason: PauseReason::Idle,
            };
            self.remaining = self.settings.work_seconds;
            return Ok(vec![EngineEvent::StateChanged(self.phase.clone())]);
        }
        if seconds >= 5 * 60 {
            return self.pause(PauseReason::Idle);
        }
        if seconds >= self.settings.short_break_seconds
            && matches!(self.phase, TimerPhase::Breaking { .. })
        {
            return self.complete_break_from_absence();
        }
        Ok(vec![])
    }
    pub fn woke_after(&mut self, seconds: u32) -> Result<Vec<EngineEvent>, EngineError> {
        if seconds >= self.settings.short_break_seconds {
            if matches!(self.phase, TimerPhase::Breaking { .. }) {
                return self.complete_break_from_absence();
            }
            return self.pause(PauseReason::Sleep);
        }
        Ok(vec![])
    }
    pub(crate) fn next_kind(&self) -> BreakKind {
        if self
            .settings
            .long_break_every
            .is_some_and(|cadence| (self.completed_breaks + 1).is_multiple_of(cadence as u32))
        {
            BreakKind::Long
        } else {
            BreakKind::Short
        }
    }
    fn start_fresh_work_or_dormant(&mut self, now: DateTime<Local>) -> Vec<EngineEvent> {
        self.remaining = self.settings.work_seconds;
        self.due_grace_remaining = None;
        self.phase = if self.manual_session || self.settings.active_at(now) {
            TimerPhase::Working
        } else {
            TimerPhase::Dormant
        };
        self.state_and_tick()
    }
    /// Returns whether the local day actually changed, so callers can react to a
    /// rollover rather than only to the counters it resets.
    fn rollover_day(&mut self, now: DateTime<Local>) -> bool {
        let day = now.date_naive();
        if self.local_day == day {
            return false;
        }
        self.local_day = day;
        self.postpones_today = 0;
        self.completed_short_breaks = 0;
        self.automatic_deferrals_today = 0;
        self.fixed_breaks_seen_today.clear();
        self.work_seconds_today = 0;
        true
    }
    fn state_and_tick(&self) -> Vec<EngineEvent> {
        vec![
            EngineEvent::StateChanged(self.phase.clone()),
            EngineEvent::Tick(self.remaining),
        ]
    }
    fn start_break(&mut self, kind: BreakKind, events: &mut Vec<EngineEvent>) {
        self.phase = TimerPhase::Breaking { kind: kind.clone() };
        self.remaining = match kind {
            BreakKind::Short => self.settings.short_break_seconds,
            BreakKind::Long => self.settings.long_break_seconds,
        };
        self.due_grace_remaining = None;
        events.push(EngineEvent::Started(kind));
        events.push(EngineEvent::StateChanged(self.phase.clone()));
    }
    /// `skipped` distinguishes an explicit early exit from a break that ran
    /// its course (including one whose duration elapsed while the person
    /// was away). Cadence counters advance identically either way — a break
    /// slot was still consumed — only the emitted event differs, so history
    /// and compliance stats can tell the two apart.
    fn finish_break(&mut self, kind: BreakKind, skipped: bool, events: &mut Vec<EngineEvent>) {
        self.completed_breaks += 1;
        if kind == BreakKind::Short {
            self.completed_short_breaks += 1;
        }
        self.phase = TimerPhase::Working;
        self.remaining = self.settings.work_seconds;
        events.push(if skipped {
            EngineEvent::Skipped(kind)
        } else {
            EngineEvent::Ended(kind)
        });
        events.push(EngineEvent::StateChanged(self.phase.clone()));
    }
}
