use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::{RelaylessAndroidExt, Result};

#[command]
pub(crate) async fn has_all_files_access<R: Runtime>(app: AppHandle<R>) -> Result<Granted> {
    app.relayless_android().has_all_files_access()
}

#[command]
pub(crate) async fn request_all_files_access<R: Runtime>(app: AppHandle<R>) -> Result<Granted> {
    app.relayless_android().request_all_files_access()
}

#[command]
pub(crate) async fn device_info<R: Runtime>(app: AppHandle<R>) -> Result<DeviceInfo> {
    app.relayless_android().device_info()
}

#[command]
pub(crate) async fn request_notification_permission<R: Runtime>(app: AppHandle<R>) -> Result<Granted> {
    app.relayless_android().request_notification_permission()
}

#[command]
pub(crate) async fn battery_status<R: Runtime>(app: AppHandle<R>) -> Result<BatteryStatus> {
    app.relayless_android().battery_status()
}

#[command]
pub(crate) async fn request_ignore_battery_optimizations<R: Runtime>(app: AppHandle<R>) -> Result<BatteryStatus> {
    app.relayless_android().request_ignore_battery_optimizations()
}

#[command]
pub(crate) async fn open_file<R: Runtime>(app: AppHandle<R>, path: String) -> Result<()> {
    app.relayless_android().open_file(path)
}
