use crate::{remote::RemoteRuntime, viewer};
use farsail_client::{NativeClient, file_runtime::NativeFiles};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub struct Desktop {
    tray_created: bool,
    quitting: AtomicBool,
    cleanup_finished: AtomicBool,
}

pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "linux")]
fn tray_host_available() -> bool {
    use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
    dbus::blocking::Connection::new_session()
        .ok()
        .is_some_and(|connection| {
            connection
                .with_proxy(
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    Duration::from_millis(300),
                )
                .get::<bool>(
                    "org.kde.StatusNotifierWatcher",
                    "IsStatusNotifierHostRegistered",
                )
                .unwrap_or(false)
        })
}

#[cfg(not(target_os = "linux"))]
fn tray_host_available() -> bool {
    true
}

fn create_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // tray-icon loads AppIndicator dynamically and panics if its library is absent.
    // A desktop without that optional integration must still be able to run FarSail.
    #[cfg(target_os = "linux")]
    if ![
        "libayatana-appindicator3.so.1",
        "libappindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so",
    ]
    .iter()
    .any(|name| unsafe { libloading::Library::new(name) }.is_ok())
    {
        return Err("AppIndicator is unavailable".into());
    }
    let open = MenuItem::with_id(app, "farsail-open", "打开 FarSail", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "farsail-quit", "退出 FarSail", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;
    TrayIconBuilder::with_id("farsail")
        .icon(
            app.default_window_icon()
                .ok_or("FarSail icon is unavailable")?
                .clone(),
        )
        .tooltip("FarSail")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "farsail-open" => show(app),
            "farsail-quit" => request_exit(app, 0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn setup(app: &tauri::App) {
    let tray_created = match create_tray(app) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("FarSail tray is unavailable: {error}");
            false
        }
    };
    app.manage(Desktop {
        tray_created,
        quitting: AtomicBool::new(false),
        cleanup_finished: AtomicBool::new(false),
    });
}

pub fn close_main(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    api.prevent_close();
    let state = window.state::<Desktop>();
    if state.quitting.load(Ordering::SeqCst) {
        return;
    }
    if state.tray_created && tray_host_available() && window.hide().is_ok() {
        return;
    }
    // No host (including GNOME without an indicator extension): retain a taskbar entry.
    // The main window's explicit Quit action remains available after restoring it.
    let _ = window.minimize();
}

pub fn request_exit(app: &AppHandle, code: i32) {
    let state = app.state::<Desktop>();
    if state.quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    let client = app.state::<Arc<NativeClient>>().inner().clone();
    let remote = app.state::<Arc<RemoteRuntime>>().inner().clone();
    let files = app.state::<Arc<NativeFiles>>().inner().clone();
    viewer::cancel_all(app);
    client.cancel_sharing_restore();
    client.disable_remote_access_local();
    files.stop_all();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        remote.stop_all().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), client.disable_remote_access()).await;
        client.stop_transport().await;
        app.state::<Desktop>()
            .cleanup_finished
            .store(true, Ordering::SeqCst);
        app.exit(code);
    });
}

pub fn on_run_event(app: &AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested { api, code, .. } = event
        && code != Some(tauri::RESTART_EXIT_CODE)
        && let Some(state) = app.try_state::<Desktop>()
        && !state.cleanup_finished.load(Ordering::SeqCst)
    {
        api.prevent_exit();
        request_exit(app, code.unwrap_or(0));
    }
}

#[tauri::command]
pub fn app_exit(window: tauri::WebviewWindow) -> Result<(), String> {
    viewer::main_only(&window)?;
    request_exit(window.app_handle(), 0);
    Ok(())
}

#[cfg(debug_assertions)]
#[tauri::command]
pub fn ipc_desktop_state(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let state = window.state::<Desktop>();
    Ok(serde_json::json!({
        "trayCreated": state.tray_created,
        "trayHost": tray_host_available(),
        "visible": window.is_visible().map_err(|e| e.to_string())?,
        "minimized": window.is_minimized().map_err(|e| e.to_string())?,
        "quitting": state.quitting.load(Ordering::SeqCst),
    }))
}

// Give isolated debug profiles their own single-instance identity on both Windows and Linux.
// Production keeps its stable app identifier and the existing per-profile file lock.
#[cfg(debug_assertions)]
pub fn isolate_test_context(context: &mut tauri::Context<tauri::Wry>) {
    use std::hash::{Hash, Hasher};
    if let Some(dir) = std::env::var_os("FARSAIL_TEST_PROFILE_DIR") {
        let dir = std::path::absolute(&dir).unwrap_or_else(|_| std::path::PathBuf::from(dir));
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        dir.hash(&mut hash);
        context
            .config_mut()
            .identifier
            .push_str(&format!(".test{:016x}", hash.finish()));
    }
}
