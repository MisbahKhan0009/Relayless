// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use relayless_core::{
    AccessRequest, ActivityDto, Core, CoreEvent, CoreOptions, CreatedInvitation, DownloadItem, GrantDto,
    JobSnapshot, JoinResult, ListResult, NetworkStatus, PeerStatus, RemoteDto, Settings, ShareDto,
};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

struct AppState {
    core: Core,
}

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------- window management

/// Opens (or focuses) the main window. Closing the window destroys the
/// WebView entirely so it uses no memory while transfers continue in the tray.
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Relayless")
        .inner_size(1200.0, 780.0)
        .min_inner_size(960.0, 620.0)
        .theme(Some(tauri::Theme::Dark))
        .background_color(tauri::window::Color(12, 13, 15, 255))
        .center()
        .build();
    if let Err(e) = built {
        tracing::error!("failed to open window: {e}");
    }
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .map(|n| {
            let mut c = n.to_lowercase().chars().collect::<Vec<_>>();
            if let Some(f) = c.first_mut() {
                *f = f.to_ascii_uppercase();
            }
            c.into_iter().collect()
        })
        .unwrap_or_else(|_| "This PC".to_string())
}

fn init_logging(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("relayless.log");
    if std::fs::metadata(&path).map(|m| m.len() > 5 * 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("relayless.old.log"));
    }
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_env("RELAYLESS_LOG")
                    .unwrap_or_else(|_| "relayless=info,relayless_core=info,iroh=warn".into()),
            )
            .with_ansi(false)
            .with_writer(Mutex::new(file))
            .try_init();
    }
}

// ---------------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    settings: Settings,
    network: NetworkStatus,
    version: String,
    data_dir: String,
}

#[tauri::command]
fn bootstrap(s: State<AppState>) -> Bootstrap {
    Bootstrap {
        settings: s.core.settings(),
        network: s.core.network_status(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: s.core.data_dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn network_status(s: State<AppState>) -> NetworkStatus {
    s.core.network_status()
}

#[tauri::command]
fn update_settings(s: State<AppState>, settings: Settings) -> CmdResult<Settings> {
    let out = s.core.update_settings(settings).map_err(err)?;
    relayless_core::set_low_priority(out.low_priority);
    Ok(out)
}

#[tauri::command]
fn list_shares(s: State<AppState>) -> CmdResult<Vec<ShareDto>> {
    s.core.list_shares().map_err(err)
}

#[tauri::command]
fn create_share(s: State<AppState>, path: String, name: Option<String>) -> CmdResult<ShareDto> {
    s.core.create_share(&path, name).map_err(err)
}

#[tauri::command]
fn set_share_active(s: State<AppState>, id: String, active: bool) -> CmdResult<()> {
    s.core.set_share_active(&id, active).map_err(err)
}

#[tauri::command]
fn delete_share(s: State<AppState>, id: String) -> CmdResult<()> {
    s.core.delete_share(&id).map_err(err)
}

#[tauri::command]
async fn create_invitation(
    s: State<'_, AppState>,
    share_id: String,
    expires_in_secs: u64,
    max_uses: Option<u32>,
    require_approval: bool,
    with_code: bool,
) -> CmdResult<CreatedInvitation> {
    s.core
        .create_invitation(&share_id, expires_in_secs, max_uses, require_approval, with_code)
        .await
        .map_err(err)
}

#[tauri::command]
fn revoke_invitation(s: State<AppState>, id: String) -> CmdResult<()> {
    s.core.revoke_invitation(&id).map_err(err)
}

#[tauri::command]
fn list_grants(s: State<AppState>) -> CmdResult<Vec<GrantDto>> {
    s.core.list_grants().map_err(err)
}

#[tauri::command]
fn revoke_device(s: State<AppState>, peer_id: String) -> CmdResult<()> {
    s.core.revoke_device(&peer_id).map_err(err)
}

#[tauri::command]
fn pending_requests(s: State<AppState>) -> Vec<AccessRequest> {
    s.core.pending_requests()
}

#[tauri::command]
fn respond_access(s: State<AppState>, id: String, approve: bool) -> CmdResult<()> {
    s.core.respond_access(&id, approve).map_err(err)
}

#[tauri::command]
async fn join(s: State<'_, AppState>, input: String) -> CmdResult<JoinResult> {
    s.core.join(&input).await.map_err(err)
}

#[tauri::command]
fn list_remotes(s: State<AppState>) -> CmdResult<Vec<RemoteDto>> {
    s.core.list_remotes().map_err(err)
}

#[tauri::command]
async fn remove_remote(s: State<'_, AppState>, peer_id: String) -> CmdResult<()> {
    s.core.remove_remote(&peer_id).await.map_err(err)
}

#[tauri::command]
async fn connect_remote(s: State<'_, AppState>, peer_id: String) -> CmdResult<PeerStatus> {
    s.core.connect_remote(&peer_id).await.map_err(err)
}

#[tauri::command]
async fn list_remote_dir(
    s: State<'_, AppState>,
    peer_id: String,
    share_id: String,
    path: Vec<String>,
    offset: u32,
    limit: u32,
) -> CmdResult<ListResult> {
    s.core.list_remote_dir(&peer_id, &share_id, path, offset, limit).await.map_err(err)
}

#[tauri::command]
async fn start_download(
    s: State<'_, AppState>,
    peer_id: String,
    share_id: String,
    items: Vec<DownloadItem>,
    dest_dir: Option<String>,
) -> CmdResult<Vec<String>> {
    s.core.start_download(&peer_id, &share_id, items, dest_dir).await.map_err(err)
}

#[tauri::command]
fn list_jobs(s: State<AppState>) -> CmdResult<Vec<JobSnapshot>> {
    s.core.list_jobs().map_err(err)
}

#[tauri::command]
async fn pause_job(s: State<'_, AppState>, id: String) -> CmdResult<()> {
    s.core.pause_job(&id).await.map_err(err)
}

#[tauri::command]
async fn resume_job(s: State<'_, AppState>, id: String) -> CmdResult<()> {
    s.core.resume_job(&id).await.map_err(err)
}

#[tauri::command]
async fn restart_job(s: State<'_, AppState>, id: String) -> CmdResult<()> {
    s.core.restart_job(&id).await.map_err(err)
}

#[tauri::command]
async fn cancel_job(s: State<'_, AppState>, id: String, delete_partial: bool) -> CmdResult<()> {
    s.core.cancel_job(&id, delete_partial).await.map_err(err)
}

#[tauri::command]
async fn remove_job(s: State<'_, AppState>, id: String) -> CmdResult<()> {
    s.core.remove_job(&id).await.map_err(err)
}

#[tauri::command]
fn list_activity(s: State<AppState>, limit: u32) -> CmdResult<Vec<ActivityDto>> {
    s.core.list_activity(limit).map_err(err)
}

/// Opens Explorer with the file selected (or the folder itself).
#[tauri::command]
fn reveal_path(path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer.exe");
        if p.is_file() {
            cmd.raw_arg(format!("/select,\"{}\"", p.display()));
        } else {
            cmd.arg(&p);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        let target = if p.is_file() { p.parent().map(PathBuf::from).unwrap_or(p) } else { p };
        std::process::Command::new("xdg-open").arg(target).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------- main

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            init_logging(&data_dir.join("logs"));
            let downloads = app
                .path()
                .download_dir()
                .map(|d| d.join("Relayless"))
                .unwrap_or_else(|_| data_dir.join("Downloads"));
            let core = tauri::async_runtime::block_on(Core::start(CoreOptions {
                data_dir: data_dir.clone(),
                default_device_name: device_name(),
                default_download_dir: downloads,
                local_only: false,
            }))
            .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            relayless_core::set_low_priority(core.settings().low_priority);
            tracing::info!("Relayless {} started as {}", env!("CARGO_PKG_VERSION"), core.endpoint_id());

            // Forward engine events to the UI and to Windows notifications.
            let handle = app.handle().clone();
            let mut rx = core.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            if let CoreEvent::Notify { title, body } = &ev {
                                let _ = handle.notification().builder().title(title).body(body).show();
                                continue;
                            }
                            // Only emit when a window exists; nothing to render otherwise.
                            if handle.get_webview_window("main").is_some() {
                                let _ = handle.emit(ev.name(), ev.payload());
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            app.manage(AppState { core });

            // Tray icon: transfers keep running when the window is closed.
            let open = MenuItem::with_id(app, "open", "Open Relayless", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit (stops transfers)", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Relayless")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } =
                        event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            network_status,
            update_settings,
            list_shares,
            create_share,
            set_share_active,
            delete_share,
            create_invitation,
            revoke_invitation,
            list_grants,
            revoke_device,
            pending_requests,
            respond_access,
            join,
            list_remotes,
            remove_remote,
            connect_remote,
            list_remote_dir,
            start_download,
            list_jobs,
            pause_job,
            resume_job,
            restart_job,
            cancel_job,
            remove_job,
            list_activity,
            reveal_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Relayless");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            // The last window closing is not a request to quit: keep running in the tray.
            if code.is_none() {
                let keep = app.try_state::<AppState>().map(|s| s.core.settings().close_to_tray).unwrap_or(true);
                if keep {
                    api.prevent_exit();
                }
            }
        }
        RunEvent::Exit => {
            if let Some(s) = app.try_state::<AppState>() {
                let core = s.core.clone();
                tauri::async_runtime::block_on(core.shutdown());
            }
        }
        _ => {}
    });
}
