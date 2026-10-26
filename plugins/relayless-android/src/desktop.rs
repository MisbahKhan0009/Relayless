use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<RelaylessAndroid<R>> {
    Ok(RelaylessAndroid(app.clone()))
}

/// Desktop stand-ins: everything is already permitted and nothing needs a service.
pub struct RelaylessAndroid<R: Runtime>(#[allow(dead_code)] AppHandle<R>);

impl<R: Runtime> RelaylessAndroid<R> {
    pub fn has_all_files_access(&self) -> crate::Result<Granted> {
        Ok(Granted { granted: true })
    }
    pub fn request_all_files_access(&self) -> crate::Result<Granted> {
        Ok(Granted { granted: true })
    }
    pub fn device_info(&self) -> crate::Result<DeviceInfo> {
        Ok(DeviceInfo::default())
    }
    pub fn request_notification_permission(&self) -> crate::Result<Granted> {
        Ok(Granted { granted: true })
    }
    pub fn battery_status(&self) -> crate::Result<BatteryStatus> {
        Ok(BatteryStatus { unrestricted: true })
    }
    pub fn request_ignore_battery_optimizations(&self) -> crate::Result<BatteryStatus> {
        Ok(BatteryStatus { unrestricted: true })
    }
    pub fn open_file(&self, _path: String) -> crate::Result<()> {
        Err(crate::Error::Unsupported("Use Show in folder on desktop".into()))
    }
    pub fn set_service(&self, _title: String, _text: String) -> crate::Result<()> {
        Ok(())
    }
    pub fn stop_service(&self) -> crate::Result<()> {
        Ok(())
    }
}
