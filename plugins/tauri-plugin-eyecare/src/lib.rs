//! Native Tauri bridge for PausIO's deliberately small mobile/watch contract.
//!
//! Timer decisions stay in `pausio-core`. This plugin only transports the latest
//! validated settings revision and exposes diagnostic delivery state.

#[cfg(not(mobile))]
use std::marker::PhantomData;

#[cfg(mobile)]
use pausio_protocol::WatchRuntimeActionV1;
use pausio_protocol::{NudgeResult, WatchSettingsEnvelopeV1, WatchStatus};
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
    #[error("watch bridges are only available in a mobile PausIO shell")]
    Unavailable,
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
