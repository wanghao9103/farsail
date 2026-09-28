use farsail_client::{NativeClient, WindowsStore};
use farsail_core::RemotePermission;
use farsail_transport::Config as TransportConfig;
use std::sync::Arc;
use tauri::Manager;
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
    op: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    client.call(&op, args).await.map_err(|e| e.to_string())
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
    id: String,
) -> Result<(), String> {
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
pub fn run() {
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
                    _ => "const state=await invoke('state'); return {ok:true,state};".to_string(),
                };
                let script = format!("(async()=>{{const invoke=window.__TAURI_INTERNALS__.invoke; try {{const result=await (async()=>{{{login}}})(); await invoke('ipc_smoke_report',{{result:JSON.stringify(result)}});}} catch(e) {{await invoke('ipc_smoke_report',{{result:JSON.stringify({{ok:false,error:String(e)}})}});}}}})();");
                let _ = webview.eval(&script);
            }
        })
        .setup(|app| {
            let dir = app.path().app_local_data_dir()?;
            let store = Arc::new(WindowsStore::new(dir)?);
            let client = Arc::new(NativeClient::new(store)?);
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
                    }
                }
            });
            app.manage(client);
            Ok(())
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
        ipc_smoke_report
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        state,
        set_server,
        call,
        transport_start,
        transport_connect,
        transport_status,
        transport_close
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("FarSail failed to start");
}
