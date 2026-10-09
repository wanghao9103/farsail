#[cfg(target_os = "linux")]
use farsail_client::LinuxStore as PlatformStore;
use farsail_client::NativeClient;
#[cfg(windows)]
use farsail_client::WindowsStore as PlatformStore;
use farsail_core::RemotePermission;
use farsail_transport::Config as TransportConfig;
use std::sync::Arc;
use tauri::Manager;
mod computer;
mod updater;
use updater::{update_check, update_download, update_install, update_preferences, update_status};
mod input_recovery;
mod remote;
mod viewer;
use fs2::FileExt;
use remote::RemoteRuntime;
#[cfg(debug_assertions)]
use viewer::ipc_viewer_state;
use viewer::{
    viewer_open, viewer_reconnect, viewer_recovery_status, viewer_window_action,
    viewer_window_active,
};
#[cfg(debug_assertions)]
static IPC_SMOKE_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
async fn state(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let mut state = client.public_state().await;
    state["computerName"] = serde_json::json!(computer::name());
    state["platform"] = serde_json::json!(std::env::consts::OS);
    state["canShareLocalScreen"] = serde_json::json!(cfg!(windows));
    state["administratorMode"] = serde_json::json!(farsail_windows::administrator_mode());
    Ok(state)
}
#[tauri::command]
async fn set_server(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    server: String,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    client.set_server(&server).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn call(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    op: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let preference = if matches!(
        op.as_str(),
        "logout" | "password" | "unbind_device" | "bind"
    ) {
        viewer::cancel_all(window.app_handle());
        // Cancel remembered permission before awaiting runtime cleanup.
        let preference = if op != "logout" {
            client.cancel_sharing_restore();
            Ok(())
        } else {
            client.forget_sharing_preferences()
        };
        remote.stop_all().await;
        preference
    } else {
        Ok(())
    };
    if op == "revoke_remote"
        && let Some(id) = args.get("id").and_then(serde_json::Value::as_str)
    {
        viewer::cancel_session(window.app_handle(), id);
        remote.stop(id).await;
    }
    // Local sign-out must still clear credentials when the optional preference store fails.
    let result = client.call(&op, args).await.map_err(|e| e.to_string());
    preference.map_err(|e| e.to_string())?;
    result
}
#[tauri::command]
fn monitors(window: tauri::WebviewWindow) -> Result<Vec<farsail_windows::Display>, String> {
    viewer::main_only(&window)?;
    farsail_windows::displays().map_err(|e| e.to_string())
}
#[tauri::command]
async fn share_enable(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    client
        .enable_sharing_preference(check_sharing_desktop)
        .await
        .map_err(|e| e.to_string())
}
fn check_sharing_desktop() -> farsail_client::Result<()> {
    let displays =
        farsail_windows::displays().map_err(|e| farsail_client::Error::Invalid(e.to_string()))?;
    let display = displays
        .first()
        .ok_or_else(|| farsail_client::Error::Invalid("no interactive display".into()))?;
    farsail_windows::Capture::new(display.id)
        .map_err(|e| farsail_client::Error::Invalid(e.to_string()))?;
    Ok(())
}
#[tauri::command]
async fn share_disable(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let preference = client.stop_sharing_preference();
    remote.stop_hosts().await;
    let result = client
        .set_host_capability(false)
        .await
        .map_err(|e| e.to_string());
    preference.map_err(|e| e.to_string())?;
    result
}
#[tauri::command]
async fn remote_status(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<serde_json::Value, String> {
    viewer::scoped(&window, &id)?;
    remote.status(&id).await
}
#[tauri::command]
async fn media_next(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    after: u64,
) -> Result<tauri::ipc::Response, String> {
    viewer::scoped(&window, &id)?;
    Ok(tauri::ipc::Response::new(
        remote.next_frame(&id, after).await?,
    ))
}
#[tauri::command]
async fn media_select(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    display: u32,
) -> Result<(), String> {
    viewer::scoped(&window, &id)?;
    remote.select(&id, display).await
}
#[tauri::command]
async fn remote_input(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    input: Option<serde_json::Value>,
    generation: Option<u64>,
) -> Result<(), String> {
    viewer::scoped(&window, &id)?;
    remote.input(&id, input, generation).await
}
#[tauri::command]
async fn media_profile(
    window: tauri::WebviewWindow,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    profile: u8,
) -> Result<u64, String> {
    viewer::scoped(&window, &id)?;
    remote.profile(&id, profile).await
}
#[tauri::command]
async fn transport_start(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    relay_url: Option<String>,
    force_relay: bool,
    bind_addr: String,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let relay = relay_url
        .filter(|s| !s.is_empty())
        .map(|s| {
            if !s.starts_with("https://") {
                return Err("relay URL must use verified HTTPS".to_owned());
            }
            s.parse::<iroh::RelayUrl>()
                .map_err(|_| "invalid relay URL".to_owned())
        })
        .transpose()?;
    let bind = bind_addr
        .parse()
        .map_err(|_| "invalid UDP bind address".to_owned())?;
    client
        .call("heartbeat", serde_json::Value::Null)
        .await
        .map_err(|e| e.to_string())?;
    client
        .start_transport(TransportConfig {
            bind,
            relay,
            force_relay,
            relay_ca_der: vec![],
        })
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn transport_connect(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    id: String,
    permission: String,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    let permission = match permission.as_str() {
        "view" => RemotePermission::View,
        "control" => RemotePermission::Control,
        "files" => RemotePermission::Files,
        _ => return Err("invalid permission".into()),
    };
    client
        .connect_transport(&id, permission)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn transport_status(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    id: String,
) -> Result<serde_json::Value, String> {
    viewer::main_only(&window)?;
    client
        .transport_status(&id)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn transport_close(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<(), String> {
    viewer::scoped(&window, &id)?;
    if window.label() != "main" {
        viewer::cancel(window.app_handle(), window.label());
    } else {
        viewer::cancel_session(window.app_handle(), &id);
    }
    remote.stop(&id).await;
    let _ = client
        .call("revoke_remote", serde_json::json!({"id":id}))
        .await;
    client
        .close_transport_session(&id)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn remote_watch(
    window: tauri::WebviewWindow,
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    enabled: bool,
) -> Result<(), String> {
    viewer::main_only(&window)?;
    if enabled {
        client
            .enable_watch_preference()
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let preference = client.stop_watch_preference();
        remote.stop_hosts().await;
        let result = client
            .revoke_host_approvals()
            .await
            .map_err(|e| e.to_string());
        preference.map_err(|e| e.to_string())?;
        result?;
    }
    Ok(())
}
#[cfg(debug_assertions)]
#[tauri::command]
fn ipc_smoke_report(result: String) -> Result<(), String> {
    let path = std::env::var("FARSAIL_IPC_SMOKE_PATH").map_err(|e| e.to_string())?;
    std::fs::write(path, result).map_err(|e| e.to_string())
}
#[cfg(debug_assertions)]
#[tauri::command]
fn ipc_media_smoke() -> Result<tauri::ipc::Response, String> {
    let frame = farsail_media::JpegFrame::encode_rgb(
        farsail_media::FrameMeta {
            monitor: 1,
            layout: 1,
            sequence: 1,
            captured_ms: 0,
            width: 16,
            height: 16,
            origin_x: 0,
            origin_y: 0,
        },
        &vec![90; 16 * 16 * 3],
        55,
    )
    .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(
        frame.to_wire().map_err(|e| e.to_string())?,
    ))
}
pub fn run() {
    #[cfg(windows)]
    farsail_windows::ensure_dpi_awareness().expect("FarSail requires per-monitor DPI awareness");
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_page_load(|webview, _| {
            #[cfg(all(debug_assertions, target_os = "linux"))]
            if std::env::var_os("FARSAIL_UBUNTU_SMOKE").is_some() {
                if webview.label() == "main" && !IPC_SMOKE_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let _ = webview.eval(include_str!("ubuntu_smoke.js"));
                }
                return;
            }
            #[cfg(debug_assertions)]
            if let Ok(mode) = std::env::var("FARSAIL_SHARE_SMOKE") {
                if webview.label() == "main" && !IPC_SMOKE_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let server=std::env::var("FARSAIL_SHARE_SMOKE_SERVER").unwrap_or_default();
                    let script=format!("const mode={};const server={};{}",serde_json::to_string(&mode).unwrap(),serde_json::to_string(&server).unwrap(),include_str!("share_smoke.js"));
                    let _=webview.eval(&script);
                }
                return;
            }
            #[cfg(debug_assertions)]
            if std::env::var_os("FARSAIL_VIEWER_SMOKE").is_some() { viewer::smoke_probe(webview); return; }
            #[cfg(not(debug_assertions))]
            let _ = webview;
            #[cfg(debug_assertions)]
            if std::env::var_os("FARSAIL_IPC_SMOKE_PATH").is_some()
                && !IPC_SMOKE_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                let login = match (
                    std::env::var("FARSAIL_IPC_SMOKE_EMAIL"),
                    std::env::var("FARSAIL_IPC_SMOKE_PASSWORD"),
                ) {
                    (Ok(email), Ok(password)) => format!(
                        "const me=await invoke('call',{{op:'login',args:{{email:{},password:{}}}}}); const signedIn=await invoke('state'); await invoke('call',{{op:'logout',args:{{}}}}); return {{ok:true,loginId:me.id,signedIn:signedIn.signedIn}};",
                        serde_json::to_string(&email).unwrap(),
                        serde_json::to_string(&password).unwrap()
                    ),
                    _ => "const state=await invoke('state'); const media=new Uint8Array(await invoke('ipc_media_smoke')); return {ok:true,state,mediaBytes:media.length,mediaMagic:String.fromCharCode(...media.slice(0,4))};".to_string(),
                };
                let script = format!("(async()=>{{const invoke=window.__TAURI_INTERNALS__.invoke; try {{const result=await (async()=>{{{login}}})(); await invoke('ipc_smoke_report',{{result:JSON.stringify(result)}});}} catch(e) {{await invoke('ipc_smoke_report',{{result:JSON.stringify({{ok:false,error:String(e)}})}});}}}})();");
                let _ = webview.eval(&script);
            }
        })
        .setup(|app| {
            let dir = app.path().app_local_data_dir()?;
            #[cfg(debug_assertions)]
            let dir=std::env::var_os("FARSAIL_TEST_PROFILE_DIR").map(std::path::PathBuf::from).unwrap_or(dir);
            std::fs::create_dir_all(&dir)?;
            let instance=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join("instance.lock"))?;
            instance.try_lock_exclusive().map_err(|_|std::io::Error::new(std::io::ErrorKind::AlreadyExists,"FarSail is already running for this profile"))?;
            app.manage(instance);
            let updater=updater::Runtime::new(dir.join("updates.json"),app.package_info().version.to_string()).map_err(std::io::Error::other)?;
            app.manage(updater.clone());
            let store = Arc::new(PlatformStore::new(dir)?);
            let client = Arc::new(NativeClient::new(store)?);
            let remote=RemoteRuntime::new(client.clone());
            let heartbeat = client.clone();
            tauri::async_runtime::spawn(async move {
                // Runs on every ordinary application start, regardless of the selected UI page.
                // Finish this one attempt before heartbeat stale-capability cleanup begins.
                let _ = heartbeat.restore_sharing(check_sharing_desktop).await;
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(25));
                loop {
                    interval.tick().await;
                    let state = heartbeat.public_state().await;
                    if state["signedIn"] == true
                        && state["deviceId"].is_string()
                        && heartbeat.call("heartbeat", serde_json::Value::Null).await.is_ok() {
                        let _ = heartbeat.refresh_transport_address().await;
                        let _=heartbeat.clear_stale_host_capability().await;
                    }
                }
            });
            app.manage(viewer::Windows::default());
            let watch_client = client.clone();
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
                loop { tick.tick().await; let _ = watch_client.approve_same_account_pending().await; }
            });
            app.manage(client);
            app.manage(remote);
            #[cfg(not(debug_assertions))] updater.start(app.handle().clone());
            #[cfg(debug_assertions)]
            viewer::smoke_setup(app.handle())?;
            Ok(())
        });
    let builder = builder.on_window_event(|window, event| {
        #[cfg(debug_assertions)]
        if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "viewer-smoke"
            && let Ok(path) = std::env::var("FARSAIL_IPC_SMOKE_PATH") {
                let _ = std::fs::write(format!("{path}.closed"), serde_json::json!({"mainExists":window.app_handle().get_webview_window("main").is_some()}).to_string());
            }
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if window.label() != "main" {
                if let Some(id) = viewer::cancel(window.app_handle(), window.label()) {
                    api.prevent_close();
                    let remote = window.state::<Arc<RemoteRuntime>>().inner().clone();
                    let client = window.state::<Arc<NativeClient>>().inner().clone();
                    let window = window.clone();
                    tauri::async_runtime::spawn(async move {
                        remote.stop(&id).await;
                        let _ = client
                            .call("revoke_remote", serde_json::json!({"id":id}))
                            .await;
                        let _ = window.destroy();
                    });
                }
                return;
            }
            viewer::cancel_all(window.app_handle());
            window.state::<Arc<NativeClient>>().cancel_sharing_restore();
            static CLOSING: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !CLOSING.swap(true, std::sync::atomic::Ordering::SeqCst) {
                api.prevent_close();
                let remote = window
                    .app_handle()
                    .state::<Arc<RemoteRuntime>>()
                    .inner()
                    .clone();
                let client = window
                    .app_handle()
                    .state::<Arc<NativeClient>>()
                    .inner()
                    .clone();
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    client.disable_host_local();
                    remote.stop_all().await;
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        client.set_host_capability(false),
                    )
                    .await;
                    client.stop_transport().await;
                    for (label, child) in window.app_handle().webview_windows() {
                        if label != "main" {
                            let _ = child.destroy();
                        }
                    }
                    let _ = window.destroy();
                });
            }
        }
    });
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        update_status,
        update_preferences,
        update_check,
        update_download,
        update_install,
        viewer_window_action,
        viewer_open,
        viewer_window_active,
        viewer_reconnect,
        viewer_recovery_status,
        remote_watch,
        state,
        set_server,
        call,
        transport_start,
        transport_connect,
        transport_status,
        transport_close,
        monitors,
        share_enable,
        share_disable,
        remote_status,
        media_next,
        media_select,
        media_profile,
        remote_input,
        ipc_viewer_state,
        ipc_smoke_report,
        ipc_media_smoke
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        update_status,
        update_preferences,
        update_check,
        update_download,
        update_install,
        viewer_window_action,
        viewer_open,
        viewer_window_active,
        viewer_reconnect,
        viewer_recovery_status,
        remote_watch,
        state,
        set_server,
        call,
        transport_start,
        transport_connect,
        transport_status,
        transport_close,
        monitors,
        share_enable,
        share_disable,
        remote_status,
        media_next,
        media_select,
        media_profile,
        remote_input
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("FarSail failed to start");
}

#[cfg(test)]
mod tests {
    use fs2::FileExt;
    #[test]
    fn profile_lock_rejects_second_process() {
        let dir = std::env::temp_dir().join(format!("farsail-lock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("instance.lock");
        let one = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let two = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        one.try_lock_exclusive().unwrap();
        assert!(two.try_lock_exclusive().is_err());
        FileExt::unlock(&one).unwrap();
        two.try_lock_exclusive().unwrap();
        FileExt::unlock(&two).unwrap();
        drop(one);
        drop(two);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
