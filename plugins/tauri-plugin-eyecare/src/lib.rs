//! Native Tauri bridge for PausIO's mobile surfaces.
//!
//! Two separate concerns share this plugin because both are phone-native:
//!
//! - **Local reminders.** A phone cannot keep a timer running — iOS suspends
//!   the app and Android dozes it — so break instants computed by
//!   `pausio-core` are registered with the OS in advance. This is what makes
//!   the phone work standalone, with no wearable and no network.
//! - **The watch bridge.** Strictly optional, and only used once a person has
//!   connected a watch in Settings.
//!
//! Timer decisions stay in `pausio-core`; this plugin only transports.

#[cfg(not(mobile))]
use std::marker::PhantomData;

#[cfg(mobile)]
use pausio_protocol::WatchRuntimeActionV1;
use pausio_protocol::{
    NudgeResult, ReminderScheduleReport, ReminderSlot, WatchPermissionState,
    WatchSettingsEnvelopeV1, WatchStatus,
};
use tauri::{
    Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(mobile)]
    #[error(transparent)]
    Invoke(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error("this capability is only available in a mobile PausIO shell")]
    Unavailable,
}

/// The reminder instants to register, replacing anything already pending.
///
/// Sent as a whole plan rather than incrementally: both platform schedulers
/// are easiest to reason about when the previous plan is cleared and rewritten,
/// and an empty `slots` is therefore the way a caller cancels everything.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReminderPlanRequest<'a> {
    pub slots: &'a [ReminderSlot],
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(mobile)]
pub struct Eyecare<R: Runtime>(PluginHandle<R>);

#[cfg(not(mobile))]
pub struct Eyecare<R: Runtime>(PhantomData<fn() -> R>);

pub trait EyecareExt<R: Runtime> {
    fn eyecare(&self) -> &Eyecare<R>;
}

impl<R: Runtime, T: Manager<R>> EyecareExt<R> for T {
    fn eyecare(&self) -> &Eyecare<R> {
        self.state::<Eyecare<R>>().inner()
    }
}

impl<R: Runtime> Eyecare<R> {
    /// Sends the latest revision through WatchConnectivity or the Wear Data Layer.
    #[cfg(mobile)]
    pub fn sync_settings(&self, envelope: &WatchSettingsEnvelopeV1) -> Result<NudgeResult> {
        Ok(self.0.run_mobile_plugin("syncSettings", envelope)?)
    }

    #[cfg(not(mobile))]
    pub fn sync_settings(&self, _: &WatchSettingsEnvelopeV1) -> Result<NudgeResult> {
        Err(Error::Unavailable)
    }

    /// A diagnostic message: it proves bridge handling only, never a physical haptic.
    #[cfg(mobile)]
    pub fn send_test_nudge(&self) -> Result<NudgeResult> {
        Ok(self.0.run_mobile_plugin("sendTestNudge", ())?)
    }

    #[cfg(not(mobile))]
    pub fn send_test_nudge(&self) -> Result<NudgeResult> {
        Err(Error::Unavailable)
    }

    #[cfg(mobile)]
    pub fn status(&self) -> Result<WatchStatus> {
        Ok(self.0.run_mobile_plugin("getStatus", ())?)
    }

    #[cfg(mobile)]
    pub fn take_pending_action(&self) -> Result<Option<WatchRuntimeActionV1>> {
        let action: String = self.0.run_mobile_plugin("takePendingAction", ())?;
        Ok((!action.is_empty())
            .then(|| serde_json::from_str::<WatchRuntimeActionV1>(&action).ok())
            .flatten()
            .filter(WatchRuntimeActionV1::is_valid))
    }

    #[cfg(not(mobile))]
    pub fn status(&self) -> Result<WatchStatus> {
        Err(Error::Unavailable)
    }

    /// Replaces the pending local reminder plan with `slots`.
    ///
    /// This is the phone's standalone delivery mechanism: the instants are
    /// registered with the OS, so they fire whether or not PausIO is running
    /// and whether or not a watch exists. Passing an empty slice cancels
    /// everything, which is how a pause clears pending reminders.
    ///
    /// The returned report says what was *actually* registered — iOS caps
    /// pending notifications at 64 and Android may downgrade to inexact
    /// alarms — so callers can tell a person their reminders are degraded
    /// rather than discovering it when one silently fails to arrive.
    #[cfg(mobile)]
    pub fn schedule_local_reminders(
        &self,
        slots: &[ReminderSlot],
    ) -> Result<ReminderScheduleReport> {
        Ok(self
            .0
            .run_mobile_plugin("scheduleLocalReminders", ReminderPlanRequest { slots })?)
    }

    #[cfg(not(mobile))]
    pub fn schedule_local_reminders(&self, _: &[ReminderSlot]) -> Result<ReminderScheduleReport> {
        Err(Error::Unavailable)
    }

    /// Clears every pending local reminder.
    #[cfg(mobile)]
    pub fn cancel_local_reminders(&self) -> Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("cancelLocalReminders", ())?;
        Ok(())
    }

    #[cfg(not(mobile))]
    pub fn cancel_local_reminders(&self) -> Result<()> {
        Err(Error::Unavailable)
    }

    /// The current OS notification permission.
    ///
    /// With reminders as the only standalone delivery path, a denial is a
    /// hard functional failure rather than cosmetic, so this is surfaced
    /// prominently instead of being retried silently.
    #[cfg(mobile)]
    pub fn local_notification_permission(&self) -> Result<WatchPermissionState> {
        Ok(self
            .0
            .run_mobile_plugin("localNotificationPermission", ())?)
    }

    #[cfg(not(mobile))]
    pub fn local_notification_permission(&self) -> Result<WatchPermissionState> {
        Err(Error::Unavailable)
    }

    /// Prompts for notification permission, returning the resulting state.
    #[cfg(mobile)]
    pub fn request_local_notification_permission(&self) -> Result<WatchPermissionState> {
        Ok(self
            .0
            .run_mobile_plugin("requestLocalNotificationPermission", ())?)
    }

    #[cfg(not(mobile))]
    pub fn request_local_notification_permission(&self) -> Result<WatchPermissionState> {
        Err(Error::Unavailable)
    }

    /// Posts a reminder immediately, so a person can confirm that standalone
    /// delivery actually works on their device without waiting for a break.
    #[cfg(mobile)]
    pub fn post_test_reminder(&self) -> Result<NudgeResult> {
        Ok(self.0.run_mobile_plugin("postTestReminder", ())?)
    }

    #[cfg(not(mobile))]
    pub fn post_test_reminder(&self) -> Result<NudgeResult> {
        Err(Error::Unavailable)
    }
}

#[cfg(mobile)]
mod mobile {
    use serde::de::DeserializeOwned;
    use tauri::{AppHandle, Runtime, plugin::PluginApi};

    use super::Eyecare;

    #[cfg(target_os = "android")]
    const PLUGIN_IDENTIFIER: &str = "com.pausio.app.eyecare";

    #[cfg(target_os = "ios")]
    tauri::ios_plugin_binding!(init_plugin_eyecare);

    pub fn init<R: Runtime, C: DeserializeOwned>(
        _app: &AppHandle<R>,
        api: PluginApi<R, C>,
    ) -> crate::Result<Eyecare<R>> {
        #[cfg(target_os = "android")]
        let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "PausIOEyecarePlugin")?;
        #[cfg(target_os = "ios")]
        let handle = api.register_ios_plugin(init_plugin_eyecare)?;
        Ok(Eyecare(handle))
    }
}

/// Registers the native side. Desktop calls receive a stable `platform_unavailable`
/// error from the application command rather than pretending a watch was contacted.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("eyecare")
        .setup(|app, _api| {
            #[cfg(mobile)]
            let bridge = mobile::init(app, _api)?;
            #[cfg(not(mobile))]
            let bridge: Eyecare<R> = Eyecare(PhantomData);
            app.manage(bridge);
            Ok(())
        })
        .build()
}
