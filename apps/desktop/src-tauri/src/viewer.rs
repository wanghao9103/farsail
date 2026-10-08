use crate::remote::RemoteRuntime;
use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Manager;

const MAX_RECOVERY_ATTEMPTS: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryPhase {
    #[default]
    Idle,
    Backoff,
    Requesting,
    AwaitingApproval,
    Connecting,
    Connected,
    Failed,
    Cancelled,
}

#[derive(Default)]
struct RecoveryProgress {
    cycle: u64,
    phase: RecoveryPhase,
    attempt: u32,
    retry_at: Option<Instant>,
}

pub struct Binding {
    pub id: Mutex<String>,
    source: String,
    target: String,
    permission: RemotePermission,
    cancelled: AtomicBool,
    retrying: AtomicBool,
    attempts: AtomicU32,
    recovery_allowed: AtomicBool,
    progress: Mutex<RecoveryProgress>,
}
impl Binding {
    fn begin_recovery(&self, manual: bool, retryable: bool) -> Result<(), String> {
        let mut progress = self.progress.lock().unwrap();
        if self.cancelled.load(Ordering::SeqCst)
            || (manual && !self.recovery_allowed.load(Ordering::SeqCst))
            || (!manual && !retryable)
        {
            return Err("会话已结束，请手动发起新连接".into());
        }
        self.recovery_allowed.store(true, Ordering::SeqCst);
        self.attempts.store(0, Ordering::SeqCst);
        *progress = RecoveryProgress {
            cycle: progress.cycle.saturating_add(1),
            phase: RecoveryPhase::Backoff,
            attempt: 0,
            retry_at: None,
        };
        Ok(())
    }
    fn update_progress(
        &self,
        phase: RecoveryPhase,
        attempt: Option<u32>,
        retry_at: Option<Instant>,
    ) {
        let mut progress = self.progress.lock().unwrap();
        if self.cancelled.load(Ordering::SeqCst) {
            progress.phase = RecoveryPhase::Cancelled;
            progress.retry_at = None;
            return;
        }
        progress.phase = phase;
        if let Some(attempt) = attempt {
            progress.attempt = attempt;
        }
        progress.retry_at = retry_at;
    }
    fn next_attempt(&self, now: Instant) -> Option<Duration> {
        if self.cancelled.load(Ordering::SeqCst) {
            return None;
        }
        let attempt = self
            .attempts
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |previous| {
                if previous < MAX_RECOVERY_ATTEMPTS {
                    Some(previous + 1)
                } else {
                    None
                }
            })
            .ok()?
            + 1;
        let delay = Duration::from_secs(u64::from(attempt) * 2);
        self.update_progress(RecoveryPhase::Backoff, Some(attempt), Some(now + delay));
        Some(delay)
    }
    fn set_phase(&self, phase: RecoveryPhase) {
        self.update_progress(phase, None, None);
    }
    fn finish_recovery(&self, connected: bool, exhausted: bool) {
        let mut progress = self.progress.lock().unwrap();
        self.retrying.store(false, Ordering::SeqCst);
        progress.retry_at = None;
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        self.recovery_allowed
            .store(!cancelled && !connected && exhausted, Ordering::SeqCst);
        progress.phase = if cancelled {
            RecoveryPhase::Cancelled
        } else if connected {
            RecoveryPhase::Connected
        } else {
            RecoveryPhase::Failed
        };
    }
    fn cancel_recovery(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.set_phase(RecoveryPhase::Cancelled);
    }
    fn recovery_status(&self, now: Instant) -> Value {
        let progress = self.progress.lock().unwrap();
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        json!({
            "cycle": progress.cycle,
            "phase": if cancelled { RecoveryPhase::Cancelled } else { progress.phase },
            "attempt": progress.attempt,
            "maxAttempts": MAX_RECOVERY_ATTEMPTS,
            "retryInMs": if cancelled { None } else { progress.retry_at.map(|deadline| deadline.saturating_duration_since(now).as_millis() as u64) },
            "manualRetryAllowed": !cancelled && !self.retrying.load(Ordering::SeqCst) && self.recovery_allowed.load(Ordering::SeqCst),
        })
    }
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
) -> Result<Value, String> {
    if !window.label().starts_with("viewer-") {
        return Err("不是查看窗口".into());
    }
    let result = match action.as_str() {
        "minimize" => window.minimize(),
        "drag" => window.start_dragging(),
        "state" => Ok(()),
        "maximize" => {
            if window.is_maximized().map_err(|e| e.to_string())? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "fullscreen" => {
            let fullscreen = !window.is_fullscreen().map_err(|e| e.to_string())?;
            window
                .set_fullscreen(fullscreen)
                .map_err(|e| e.to_string())?;
            return Ok(json!({"fullscreen":fullscreen}));
        }
        "close" => window.close(),
        _ => return Err("invalid window action".into()),
    };
    result.map_err(|e| e.to_string())?;
    if action == "close" {
        return Ok(json!({}));
    }
    Ok(json!({"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?}))
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
        recovery_allowed: AtomicBool::new(false),
        progress: Mutex::new(RecoveryProgress::default()),
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
    .decorations(false)
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
            recovery_allowed: AtomicBool::new(false),
            progress: Mutex::new(RecoveryProgress::default()),
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
    .decorations(false)
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
        if(initial.decorations || !document.querySelector('.viewer-titlebar')) throw Error('custom titlebar missing');
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
        const toggleFullscreen=()=>[...document.querySelectorAll('button')].find(b=>b.textContent==='全屏切换').click();
        toggleFullscreen();
        await new Promise(r=>setTimeout(r,300));
        const fullscreen=await invoke('ipc_viewer_state');
        if(!fullscreen.fullscreen) throw Error('fullscreen failed');
        if(document.querySelector('.viewer-titlebar')) throw Error('titlebar should hide in fullscreen');
        toggleFullscreen();
        await new Promise(r=>setTimeout(r,300));
        if(!document.querySelector('.viewer-titlebar')) throw Error('titlebar should return after fullscreen');
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
        json!({"decorations":window.is_decorated().map_err(|e|e.to_string())?,"maximized":window.is_maximized().map_err(|e|e.to_string())?,"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?,"minimized":window.is_minimized().map_err(|e|e.to_string())?,"mainExists":window.app_handle().get_webview_window("main").is_some()}),
    )
}
pub fn cancel(app: &tauri::AppHandle, label: &str) -> Option<String> {
    let binding = app.state::<Windows>().0.lock().unwrap().remove(label)?;
    binding.cancel_recovery();
    Some(binding.id.lock().unwrap().clone())
}
pub fn cancel_all(app: &tauri::AppHandle) {
    let bindings = app.state::<Windows>();
    for b in bindings.0.lock().unwrap().values() {
        b.cancel_recovery();
    }
}
pub fn cancel_session(app: &tauri::AppHandle, id: &str) {
    let bindings = app.state::<Windows>();
    for (label, binding) in bindings.0.lock().unwrap().iter() {
        if label == &format!("viewer-{id}") || *binding.id.lock().unwrap() == id {
            binding.cancel_recovery();
        }
    }
}
#[tauri::command]
pub fn viewer_recovery_status(window: tauri::WebviewWindow) -> Result<Value, String> {
    if !window.label().starts_with("viewer-") {
        return Err("不是查看窗口".into());
    }
    let binding = window
        .state::<Windows>()
        .0
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .ok_or("查看窗口已关闭")?;
    Ok(binding.recovery_status(Instant::now()))
}
#[tauri::command]
pub async fn viewer_reconnect(
    window: tauri::WebviewWindow,
    manual: Option<bool>,
) -> Result<Value, String> {
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
    let mut exhausted = false;
    let result = async {
        if manual.unwrap_or(false) {
            binding.begin_recovery(true, false)?;
        } else {
            let status = remote.status(&old).await?;
            binding.begin_recovery(false, status["retryable"] == true)?;
        }
        for _ in 0..MAX_RECOVERY_ATTEMPTS {
            let Some(delay) = binding.next_attempt(Instant::now()) else { break; };
            tokio::time::sleep(delay).await;
            if binding.cancelled.load(Ordering::SeqCst) { return Err("重连已取消".into()); }
            let state = client.public_state().await;
            if state["deviceId"].as_str() != Some(&binding.source) || state["signedIn"] != true { return Err("登录或设备身份已改变".into()); }
            binding.set_phase(RecoveryPhase::Requesting);
            // Fresh coordinator decision; no grant or approval from the old session is reused.
            let request = match client.call("request", json!({"source_device_id":binding.source,"target_device_id":binding.target,"permission":binding.permission})).await {
                Ok(r) => r,
                Err(farsail_client::Error::Network(_)) => continue,
                Err(_) => return Err("目标设备已停止共享、授权不可用或身份已失效；请手动重新连接".into()),
            };
            let id = request["id"].as_str().ok_or("invalid request")?.to_owned();
            *binding.id.lock().unwrap() = id.clone();
            binding.set_phase(RecoveryPhase::AwaitingApproval);
            let mut connected = false;
            for _ in 0..20 {
                if binding.cancelled.load(Ordering::SeqCst) { break; }
                tokio::time::sleep(Duration::from_secs(1)).await;
                match client.call("remote_status", json!({"id":id})).await {
                    Ok(r) if r["state"] == "approved" => {
                        if binding.cancelled.load(Ordering::SeqCst) { break; }
                        binding.set_phase(RecoveryPhase::Connecting);
                        connected = tokio::select! {
                            biased;
                            _ = binding.wait_cancelled() => false,
                            result = client.connect_transport(&id, binding.permission) => result.is_ok(),
                        };
                        break;
                    }
                    Ok(r) if r["state"] != "pending" => {
                        // Explicit refusal/revocation/expiry ends this recovery cycle.
                        binding.cancel_recovery();
                        break;
                    }
                    _ => (),
                }
            }
            if connected && !binding.cancelled.load(Ordering::SeqCst) {
                return Ok(json!({"id":id}));
            }
            remote.stop(&id).await;
            let _ = client.call("revoke_remote", json!({"id":id})).await;
            if binding.cancelled.load(Ordering::SeqCst) { return Err("重连已取消或授权已结束".into()); }
        }
        exhausted = binding.attempts.load(Ordering::SeqCst) == MAX_RECOVERY_ATTEMPTS;
        Err("自动重连未成功，请检查网络并重新发起连接".into())
    }.await;
    binding.finish_recovery(result.is_ok(), exhausted);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            id: Mutex::new("synthetic".into()),
            source: "source".into(),
            target: "target".into(),
            permission: RemotePermission::Control,
            cancelled: AtomicBool::new(false),
            retrying: AtomicBool::new(false),
            attempts: AtomicU32::new(0),
            recovery_allowed: AtomicBool::new(false),
            progress: Mutex::new(RecoveryProgress::default()),
        }
    }
    #[test]
    fn exhausted_recovery_can_restart_but_refusal_or_stop_cannot() {
        let binding = binding();
        assert!(binding.begin_recovery(true, false).is_err());
        assert!(binding.begin_recovery(false, false).is_err());
        binding.begin_recovery(false, true).unwrap();
        binding.attempts.store(3, Ordering::SeqCst);
        binding.begin_recovery(true, false).unwrap();
        assert_eq!(binding.attempts.load(Ordering::SeqCst), 0);
        binding.cancelled.store(true, Ordering::SeqCst);
        assert!(binding.begin_recovery(true, false).is_err());
        assert!(binding.begin_recovery(false, true).is_err());
    }
    #[test]
    fn progress_follows_attempt_stages_and_the_real_backoff_deadline() {
        let binding = binding();
        let now = Instant::now();
        assert_eq!(binding.recovery_status(now)["phase"], "idle");
        binding.retrying.store(true, Ordering::SeqCst);
        binding.begin_recovery(false, true).unwrap();
        assert_eq!(binding.next_attempt(now), Some(Duration::from_secs(2)));
        let backoff = binding.recovery_status(now + Duration::from_millis(500));
        assert_eq!(backoff["cycle"], 1);
        assert_eq!(backoff["attempt"], 1);
        assert_eq!(backoff["maxAttempts"], 3);
        assert_eq!(backoff["phase"], "backoff");
        assert_eq!(backoff["retryInMs"], 1500);
        assert_eq!(backoff["manualRetryAllowed"], false);
        assert_eq!(
            binding.recovery_status(now + Duration::from_secs(3))["retryInMs"],
            0
        );
        for (phase, expected) in [
            (RecoveryPhase::Requesting, "requesting"),
            (RecoveryPhase::AwaitingApproval, "awaiting_approval"),
            (RecoveryPhase::Connecting, "connecting"),
        ] {
            binding.set_phase(phase);
            let status = binding.recovery_status(now);
            assert_eq!(status["phase"], expected);
            assert_eq!(status["attempt"], 1);
            assert_eq!(status["cycle"], 1);
            assert!(status["retryInMs"].is_null());
        }
        binding.finish_recovery(true, false);
        assert_eq!(binding.recovery_status(now)["phase"], "connected");
        assert_eq!(binding.recovery_status(now)["manualRetryAllowed"], false);
        assert!(binding.begin_recovery(true, false).is_err());
    }
    #[test]
    fn only_exhausted_recoverable_attempts_offer_a_fresh_manual_cycle() {
        let binding = binding();
        let now = Instant::now();
        binding.retrying.store(true, Ordering::SeqCst);
        binding.begin_recovery(false, true).unwrap();
        for attempt in 1..=MAX_RECOVERY_ATTEMPTS {
            assert_eq!(
                binding.next_attempt(now),
                Some(Duration::from_secs(u64::from(attempt) * 2))
            );
        }
        assert!(binding.next_attempt(now).is_none());
        assert_eq!(binding.attempts.load(Ordering::SeqCst), 3);
        assert_eq!(binding.recovery_status(now)["manualRetryAllowed"], false);
        binding.finish_recovery(false, true);
        assert_eq!(binding.recovery_status(now)["phase"], "failed");
        assert_eq!(binding.recovery_status(now)["manualRetryAllowed"], true);
        binding.retrying.store(true, Ordering::SeqCst);
        binding.begin_recovery(true, false).unwrap();
        assert_eq!(binding.recovery_status(now)["cycle"], 2);
        assert_eq!(binding.recovery_status(now)["attempt"], 0);
        assert_eq!(binding.recovery_status(now)["manualRetryAllowed"], false);
        assert_eq!(binding.next_attempt(now), Some(Duration::from_secs(2)));
    }
    #[test]
    fn early_identity_or_authorisation_failure_cannot_offer_manual_recovery() {
        let binding = binding();
        let now = Instant::now();
        binding.retrying.store(true, Ordering::SeqCst);
        binding.begin_recovery(false, true).unwrap();
        binding.next_attempt(now).unwrap();
        binding.set_phase(RecoveryPhase::Requesting);
        binding.finish_recovery(false, false);
        let status = binding.recovery_status(now);
        assert_eq!(status["phase"], "failed");
        assert_eq!(status["attempt"], 1);
        assert_eq!(status["manualRetryAllowed"], false);
        assert!(binding.begin_recovery(true, false).is_err());
        // A later independently eligible network failure still starts its own cycle.
        binding.begin_recovery(false, true).unwrap();
        assert_eq!(binding.recovery_status(now)["cycle"], 2);
    }
    #[test]
    fn cancellation_overrides_late_phase_and_connection_results() {
        let binding = binding();
        let now = Instant::now();
        binding.retrying.store(true, Ordering::SeqCst);
        binding.begin_recovery(false, true).unwrap();
        binding.next_attempt(now).unwrap();
        binding.cancel_recovery();
        binding.set_phase(RecoveryPhase::AwaitingApproval);
        binding.set_phase(RecoveryPhase::Connecting);
        binding.finish_recovery(true, true);
        let status = binding.recovery_status(now);
        assert_eq!(status["phase"], "cancelled");
        assert_eq!(status["manualRetryAllowed"], false);
        assert!(status["retryInMs"].is_null());
        assert!(binding.next_attempt(now).is_none());
        assert!(binding.begin_recovery(true, false).is_err());
        assert!(binding.begin_recovery(false, true).is_err());
    }
    #[test]
    fn progress_is_preserved_while_a_fresh_request_replaces_the_session_id() {
        let binding = binding();
        let now = Instant::now();
        binding.begin_recovery(false, true).unwrap();
        binding.next_attempt(now).unwrap();
        binding.set_phase(RecoveryPhase::AwaitingApproval);
        let before = binding.recovery_status(now);
        *binding.id.lock().unwrap() = "fresh-request".into();
        assert_eq!(binding.recovery_status(now), before);
    }
}
