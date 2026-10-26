const COMMANDS: &[&str] = &[
    "has_all_files_access",
    "request_all_files_access",
    "device_info",
    "request_notification_permission",
    "battery_status",
    "request_ignore_battery_optimizations",
    "open_file",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").build();
}
