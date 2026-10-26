use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<RelaylessAndroid<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("app.relayless.plugin", "RelaylessPlugin")?;
    #[cfg(target_os = "ios")]
    let handle: PluginHandle<R> = {
        let _ = api;
        return Err(crate::Error::Unsupported("iOS is not supported".into()));
    };
    Ok(RelaylessAndroid(handle))
}

/// Android APIs Relayless needs.
pub struct RelaylessAndroid<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> RelaylessAndroid<R> {
    pub fn has_all_files_access(&self) -> crate::Result<Granted> {
        Ok(self.0.run_mobile_plugin("hasAllFilesAccess", Empty {})?)
    }
    pub fn request_all_files_access(&self) -> crate::Result<Granted> {
        Ok(self.0.run_mobile_plugin("requestAllFilesAccess", Empty {})?)
    }
    pub fn device_info(&self) -> crate::Result<DeviceInfo> {
        Ok(self.0.run_mobile_plugin("deviceInfo", Empty {})?)
    }
    pub fn request_notification_permission(&self) -> crate::Result<Granted> {
        Ok(self.0.run_mobile_plugin("requestNotificationPermission", Empty {})?)
    }
    pub fn battery_status(&self) -> crate::Result<BatteryStatus> {
        Ok(self.0.run_mobile_plugin("batteryStatus", Empty {})?)
    }
    pub fn request_ignore_battery_optimizations(&self) -> crate::Result<BatteryStatus> {
        Ok(self.0.run_mobile_plugin("requestIgnoreBatteryOptimizations", Empty {})?)
    }
    pub fn open_file(&self, path: String) -> crate::Result<()> {
        let _: Empty = self.0.run_mobile_plugin("openFile", PathArg { path })?;
        Ok(())
    }
    /// Starts or updates the foreground service that keeps transfers alive.
    pub fn set_service(&self, title: String, text: String) -> crate::Result<()> {
        let _: Empty = self.0.run_mobile_plugin("setService", ServiceText { title, text })?;
        Ok(())
    }
    pub fn stop_service(&self) -> crate::Result<()> {
        let _: Empty = self.0.run_mobile_plugin("stopService", Empty {})?;
        Ok(())
    }
}
