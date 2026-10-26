//! Android integration for Relayless. On desktop every call is a harmless no-op.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
pub use desktop::RelaylessAndroid;
#[cfg(mobile)]
pub use mobile::RelaylessAndroid;

pub trait RelaylessAndroidExt<R: Runtime> {
    fn relayless_android(&self) -> &RelaylessAndroid<R>;
}

impl<R: Runtime, T: Manager<R>> RelaylessAndroidExt<R> for T {
    fn relayless_android(&self) -> &RelaylessAndroid<R> {
        self.state::<RelaylessAndroid<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("relayless-android")
        .invoke_handler(tauri::generate_handler![
            commands::has_all_files_access,
            commands::request_all_files_access,
            commands::device_info,
            commands::request_notification_permission,
            commands::battery_status,
            commands::request_ignore_battery_optimizations,
            commands::open_file,
        ])
        .setup(|app, api| {
            #[cfg(mobile)]
            let plugin = mobile::init(app, api)?;
            #[cfg(desktop)]
            let plugin = desktop::init(app, api)?;
            app.manage(plugin);
            Ok(())
        })
        .build()
}
