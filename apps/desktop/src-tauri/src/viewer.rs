use crate::remote::RemoteRuntime;
use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tauri::Manager;

pub struct Binding {
    pub id: Mutex<String>,
    source: String,
    target: String,
    permission: RemotePermission,
    cancelled: AtomicBool,
    retrying: AtomicBool,
    attempts: AtomicU32,
}
impl Binding {
    async fn wait_cancelled(&self) {
        while !self.cancelled.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
#[derive(Default)]
pub struct Windows(pub Mutex<HashMap<String, Arc<Binding>>>);

#[tauri::command]
pub async fn viewer_window_action(
    window: tauri::WebviewWindow,
    action: String,
) -> Result<(), String> {
    if !window.label().starts_with("viewer-") {
        return Err("不是查看窗口".into());
    }
    let result = match action.as_str() {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized().map_err(|e| e.to_string())? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "fullscreen" => window.set_fullscreen(!window.is_fullscreen().map_err(|e| e.to_string())?),
        "close" => window.close(),
        _ => return Err("invalid window action".into()),
    };
    result.map_err(|e| e.to_string())
}

pub fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("此窗口无权操作账号或设备".into())
    }
}
pub fn scoped(window: &tauri::WebviewWindow, id: &str) -> Result<(), String> {
    if window.label() == "main" {
        return Ok(());
    }
    let windows = window.state::<Windows>();
    let map = windows.0.lock().unwrap();
    let binding = map.get(window.label()).ok_or("查看窗口已关闭")?;
    if binding.cancelled.load(Ordering::SeqCst) || *binding.id.lock().unwrap() != id {
        return Err("此窗口无权操作该会话".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn viewer_open(window: tauri::WebviewWindow, id: String) -> Result<(), String> {
    main_only(&window)?;
    if id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
        return Err("invalid session".into());
    }
    let app = window.app_handle();
    let label = format!("viewer-{id}");
    if let Some(viewer) = app.get_webview_window(&label) {
        return viewer.set_focus().map_err(|e| e.to_string());
    }
    let client = app.state::<Arc<NativeClient>>();
    let row = client
        .call("remote_status", json!({"id":id}))
        .await
        .map_err(|e| e.to_string())?;
    let state = client.public_state().await;
    if row["source_device_id"] != state["deviceId"] || row["state"] != "approved" {
        return Err("只能打开本机发起的有效会话".into());
    }
    let permission = match row["permission"].as_str() {
        Some("control") => RemotePermission::Control,
        Some("view") => RemotePermission::View,
        _ => return Err("不是屏幕会话".into()),
    };
    let binding = Arc::new(Binding {
        id: Mutex::new(id.clone()),
        source: row["source_device_id"]
            .as_str()
            .ok_or("source missing")?
            .into(),
        target: row["target_device_id"]
            .as_str()
            .ok_or("target missing")?
            .into(),
        permission,
        cancelled: AtomicBool::new(false),
        retrying: AtomicBool::new(false),
        attempts: AtomicU32::new(0),
    });
    {
        let windows = app.state::<Windows>();
        let mut map = windows.0.lock().unwrap();
        if map.contains_key(&label) {
            return Ok(());
        }
        map.insert(label.clone(), binding);
    }
    let result = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App(format!("index.html?viewer={id}").into()),
    )
    .title("FarSail · 远程桌面")
    .inner_size(1200.0, 800.0)
    .min_inner_size(640.0, 420.0)
    .maximized(true)
    .on_navigation(|url| {
        let origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
            || (cfg!(debug_assertions)
                && url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port() == Some(1420));
        origin && matches!(url.path(), "/" | "/index.html")
    })
    .build();
    if let Err(e) = result {
        app.state::<Windows>().0.lock().unwrap().remove(&label);
        app.state::<Arc<RemoteRuntime>>().stop(&id).await;
        return Err(e.to_string());
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub fn smoke_setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    if std::env::var_os("FARSAIL_VIEWER_SMOKE").is_none() {
        return Ok(());
    }
    let id = "00000000-0000-0000-0000-000000000001";
    app.state::<Windows>().0.lock().unwrap().insert(
        "viewer-smoke".into(),
        Arc::new(Binding {
            id: Mutex::new(id.into()),
            source: "synthetic".into(),
            target: "synthetic".into(),
            permission: RemotePermission::View,
            cancelled: AtomicBool::new(false),
            retrying: AtomicBool::new(false),
            attempts: AtomicU32::new(0),
        }),
    );
    tauri::WebviewWindowBuilder::new(
        app,
        "viewer-smoke",
        tauri::WebviewUrl::App(format!("index.html?viewer={id}").into()),
    )
    .title("FarSail isolated viewer test")
    .inner_size(1000.0, 700.0)
    .maximized(true)
    .build()?;
    Ok(())
}

#[cfg(debug_assertions)]
pub fn smoke_probe(webview: &tauri::Webview) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if webview.label() != "viewer-smoke" || STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = webview.eval(r#"setTimeout(async()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const denied=[];
      try {
        const initial=await invoke('ipc_viewer_state');
        if(!initial.maximized) throw Error('viewer did not open maximized');
        for(const [cmd,args] of [['state',{}],['call',{op:'admin_users',args:{}}],['set_server',{server:'http://127.0.0.1:1'}],['share_enable',{}],['remote_watch',{enabled:true}],['remote_input',{id:'other',input:null}],['media_profile',{id:'other',profile:1}],['transport_connect',{id:'other',permission:'view'}],['plugin:window|close',{label:'main'}]]) {
          let blocked=false; try { await invoke(cmd,args); } catch { blocked=true; }
          if(!blocked) throw Error('unexpected permission: '+cmd); denied.push(cmd);
        }
        await invoke('viewer_window_action',{action:'maximize'});
        await new Promise(r=>setTimeout(r,300));
        const restored=await invoke('ipc_viewer_state');
        if(restored.maximized) throw Error('restore failed');
        await invoke('viewer_window_action',{action:'maximize'});
        await new Promise(r=>setTimeout(r,300));
        const maximized=await invoke('ipc_viewer_state');
        if(!maximized.maximized) throw Error('maximize failed');
        await invoke('viewer_window_action',{action:'fullscreen'});
        await new Promise(r=>setTimeout(r,300));
        const fullscreen=await invoke('ipc_viewer_state');
        if(!fullscreen.fullscreen) throw Error('fullscreen failed');
        await invoke('viewer_window_action',{action:'fullscreen'});
        await invoke('viewer_window_action',{action:'minimize'});
        await new Promise(r=>setTimeout(r,300));
        const minimized=await invoke('ipc_viewer_state');
        if(!minimized.minimized || !minimized.mainExists) throw Error('minimize/main isolation failed');
        await invoke('ipc_smoke_report',{result:JSON.stringify({ok:true,denied,initial,restored,maximized,fullscreen,minimized})});
        await invoke('viewer_window_action',{action:'close'});
      } catch(e) { await invoke('ipc_smoke_report',{result:JSON.stringify({ok:false,error:String(e),denied})}); }
    },800)"#);
}

#[cfg(debug_assertions)]
#[tauri::command]
pub fn ipc_viewer_state(window: tauri::WebviewWindow) -> Result<Value, String> {
    Ok(
        json!({"maximized":window.is_maximized().map_err(|e|e.to_string())?,"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?,"minimized":window.is_minimized().map_err(|e|e.to_string())?,"mainExists":window.app_handle().get_webview_window("main").is_some()}),
    )
}
pub fn cancel(app: &tauri::AppHandle, label: &str) -> Option<String> {
    let binding = app.state::<Windows>().0.lock().unwrap().remove(label)?;
    binding.cancelled.store(true, Ordering::SeqCst);
    Some(binding.id.lock().unwrap().clone())
}
pub fn cancel_all(app: &tauri::AppHandle) {
    let bindings = app.state::<Windows>();
    for b in bindings.0.lock().unwrap().values() {
        b.cancelled.store(true, Ordering::SeqCst);
    }
}
pub fn cancel_session(app: &tauri::AppHandle, id: &str) {
    let bindings = app.state::<Windows>();
    for (label, binding) in bindings.0.lock().unwrap().iter() {
        if label == &format!("viewer-{id}") || *binding.id.lock().unwrap() == id {
            binding.cancelled.store(true, Ordering::SeqCst);
        }
    }
}
#[tauri::command]
pub async fn viewer_reconnect(window: tauri::WebviewWindow) -> Result<Value, String> {
    let app = window.app_handle();
    let binding = app
        .state::<Windows>()
        .0
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .ok_or("不是查看窗口")?;
    if binding.cancelled.load(Ordering::SeqCst) || binding.retrying.swap(true, Ordering::SeqCst) {
        return Err("重连已取消或正在进行".into());
    }
    let remote = app.state::<Arc<RemoteRuntime>>().inner().clone();
    let client = app.state::<Arc<NativeClient>>().inner().clone();
    let old = binding.id.lock().unwrap().clone();
    let result = async {
        let status = remote.status(&old).await?;
        if status["retryable"] != true { return Err("会话已结束，请手动发起新连接".into()); }
        for _ in 0..3 {
            let attempt = binding.attempts.fetch_add(1, Ordering::SeqCst) as u64 + 1;
            if attempt > 3 { break; }
            tokio::time::sleep(Duration::from_secs(attempt * 2)).await;
            if binding.cancelled.load(Ordering::SeqCst) { return Err("重连已取消".into()); }
            let state = client.public_state().await;
            if state["deviceId"].as_str() != Some(&binding.source) || state["signedIn"] != true { return Err("登录或设备身份已改变".into()); }
            // Fresh coordinator decision; no grant or approval from the old session is reused.
            let request = match client.call("request", json!({"source_device_id":binding.source,"target_device_id":binding.target,"permission":binding.permission})).await {
                Ok(r) => r,
                Err(farsail_client::Error::Network(_)) => continue,
                Err(_) => return Err("目标设备已停止共享、授权不可用或身份已失效；请手动重新连接".into()),
            };
            let id = request["id"].as_str().ok_or("invalid request")?.to_owned();
            *binding.id.lock().unwrap() = id.clone();
            let mut connected = false;
            for _ in 0..20 {
                if binding.cancelled.load(Ordering::SeqCst) { break; }
                tokio::time::sleep(Duration::from_secs(1)).await;
                match client.call("remote_status", json!({"id":id})).await {
                    Ok(r) if r["state"] == "approved" => {
                        if binding.cancelled.load(Ordering::SeqCst) { break; }
                        connected = tokio::select! {
                            biased;
                            _ = binding.wait_cancelled() => false,
                            result = client.connect_transport(&id, binding.permission) => result.is_ok(),
                        };
                        break;
                    }
                    Ok(r) if r["state"] != "pending" => {
                        // Explicit refusal/revocation/expiry ends this recovery cycle.
                        binding.cancelled.store(true, Ordering::SeqCst);
                        break;
                    }
                    _ => (),
                }
            }
            if connected && !binding.cancelled.load(Ordering::SeqCst) { return Ok(json!({"id":id})); }
            remote.stop(&id).await;
            let _ = client.call("revoke_remote", json!({"id":id})).await;
            if binding.cancelled.load(Ordering::SeqCst) { return Err("重连已取消或授权已结束".into()); }
        }
        Err("自动重连未成功，请检查网络并重新发起连接".into())
    }.await;
    binding.retrying.store(false, Ordering::SeqCst);
    result
}
