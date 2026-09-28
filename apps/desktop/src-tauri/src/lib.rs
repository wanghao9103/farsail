use farsail_client::{NativeClient, WindowsStore};
use farsail_core::RemotePermission;
use farsail_transport::Config as TransportConfig;
use std::sync::Arc;
use tauri::Manager;
mod remote;
use fs2::FileExt;
use remote::RemoteRuntime;
#[cfg(debug_assertions)]
static IPC_SMOKE_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
async fn state(client: tauri::State<'_, Arc<NativeClient>>) -> Result<serde_json::Value, String> {
    Ok(client.public_state().await)
}
#[tauri::command]
async fn set_server(
    client: tauri::State<'_, Arc<NativeClient>>,
    server: String,
) -> Result<serde_json::Value, String> {
    client.set_server(&server).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn call(
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    op: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if matches!(
        op.as_str(),
        "logout" | "password" | "unbind_device" | "bind"
    ) {
        client.disable_host_local();
        remote.stop_all().await;
    }
    if op == "revoke_remote"
        && let Some(id) = args.get("id").and_then(serde_json::Value::as_str)
    {
        remote.stop(id).await;
    }
    client.call(&op, args).await.map_err(|e| e.to_string())
}
#[tauri::command]
fn monitors() -> Result<Vec<farsail_windows::Display>, String> {
    farsail_windows::displays().map_err(|e| e.to_string())
}
#[tauri::command]
async fn share_enable(
    client: tauri::State<'_, Arc<NativeClient>>,
) -> Result<serde_json::Value, String> {
    if !client.transport_running().await {
        return Err("start the transport endpoint first".into());
    }
    let displays = farsail_windows::displays().map_err(|e| e.to_string())?;
    let display = displays.first().ok_or("no interactive display")?;
    farsail_windows::Capture::new(display.id).map_err(|e| e.to_string())?;
    client
        .set_host_capability(true)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn share_disable(
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
) -> Result<serde_json::Value, String> {
    client.disable_host_local();
    remote.stop_hosts().await;
    client
        .set_host_capability(false)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn remote_status(
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<serde_json::Value, String> {
    remote.status(&id).await
}
#[tauri::command]
async fn media_next(
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    after: u64,
) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(
        remote.next_frame(&id, after).await?,
    ))
}
#[tauri::command]
async fn media_select(
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    display: u32,
) -> Result<(), String> {
    remote.select(&id, display).await
}
#[tauri::command]
async fn remote_input(
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
    input: Option<serde_json::Value>,
) -> Result<(), String> {
    remote.input(&id, input).await
}
#[tauri::command]
async fn transport_start(
    client: tauri::State<'_, Arc<NativeClient>>,
    relay_url: Option<String>,
    force_relay: bool,
    bind_addr: String,
) -> Result<serde_json::Value, String> {
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
    client: tauri::State<'_, Arc<NativeClient>>,
    id: String,
    permission: String,
) -> Result<serde_json::Value, String> {
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
    client: tauri::State<'_, Arc<NativeClient>>,
    id: String,
) -> Result<serde_json::Value, String> {
    client
        .transport_status(&id)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn transport_close(
    client: tauri::State<'_, Arc<NativeClient>>,
    remote: tauri::State<'_, Arc<RemoteRuntime>>,
    id: String,
) -> Result<(), String> {
    remote.stop(&id).await;
    client
        .close_transport_session(&id)
        .await
        .map_err(|e| e.to_string())
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
    farsail_windows::ensure_dpi_awareness().expect("FarSail requires per-monitor DPI awareness");
    let builder = tauri::Builder::default()
        .on_page_load(|webview, _| {
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
            let store = Arc::new(WindowsStore::new(dir)?);
            let client = Arc::new(NativeClient::new(store)?);
            let remote=RemoteRuntime::new(client.clone());
            let heartbeat = client.clone();
            tauri::async_runtime::spawn(async move {
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
            app.manage(client);
            app.manage(remote);
            Ok(())
        });
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
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
                    let _ = window.destroy();
                });
            }
        }
    });
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
        remote_input,
        ipc_smoke_report,
        ipc_media_smoke
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
