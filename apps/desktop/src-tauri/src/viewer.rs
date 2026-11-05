use crate::remote::RemoteRuntime;
use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Manager;

const MAX_RECOVERY_ATTEMPTS: u32 = 3;

fn next_window_instance() -> String {
    static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
    NEXT_INSTANCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
            previous.checked_add(1)
        })
        .expect("viewer instance counter exhausted")
        .to_string()
}

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
    host: String,
    name: String,
    selected: AtomicBool,
    pub id: Mutex<String>,
    instance: String,
    opened: AtomicBool,
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
    fn window_active(&self, native_exists: bool) -> bool {
        native_exists
            && self.opened.load(Ordering::SeqCst)
            && !self.cancelled.load(Ordering::SeqCst)
    }

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
pub struct Windows(
    pub Mutex<HashMap<String, Arc<Binding>>>,
    Mutex<HashSet<String>>,
);

// Work-area coordinates are logical, including negative origins on other monitors.
fn cascade_position(
    origin: (f64, f64),
    area: (f64, f64),
    size: (f64, f64),
    index: usize,
) -> (f64, f64) {
    let columns = (((area.0 - size.0 - 16.0).max(0.0) / 36.0).floor() as usize) + 1;
    let rows = (((area.1 - size.1 - 16.0).max(0.0) / 36.0).floor() as usize) + 1;
    (
        origin.0 + 8.0 + (index % columns) as f64 * 36.0,
        origin.1 + 8.0 + (index % rows) as f64 * 36.0,
    )
}

fn place_viewer(window: &tauri::WebviewWindow, index: usize) -> Result<(), String> {
    let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let factor = monitor.scale_factor();
    let area = monitor.work_area();
    let origin = area.position.to_logical::<f64>(factor);
    let available = area.size.to_logical::<f64>(factor);
    let size = window
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(factor);
    let (x, y) = cascade_position(
        (origin.x, origin.y),
        (available.width, available.height),
        (size.width, size.height),
        index,
    );
    window
        .set_position(tauri::LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn cascade_viewers(app: &tauri::AppHandle) -> Result<(), String> {
    let mut viewers: Vec<_> = app
        .state::<Windows>()
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, binding)| binding.window_active(true))
        .map(|(_, binding)| {
            (
                binding.instance.parse::<u64>().unwrap_or(0),
                binding.host.clone(),
            )
        })
        .collect();
    viewers.sort_by_key(|(instance, _)| *instance);
    let mut monitors = HashMap::new();
    let mut seen = HashSet::new();
    for (_, label) in viewers {
        if !seen.insert(label.clone()) {
            continue;
        }
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? else {
            continue;
        };
        let origin = monitor.work_area().position;
        let index = monitors.entry((origin.x, origin.y)).or_insert(0usize);
        window.set_fullscreen(false).map_err(|e| e.to_string())?;
        window.unmaximize().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        place_viewer(&window, *index)?;
        window.set_focus().map_err(|e| e.to_string())?;
        *index += 1;
    }
    Ok(())
}

#[tauri::command]
pub async fn viewer_window_action(
    window: tauri::WebviewWindow,
    action: String,
    landscape: Option<bool>,
    automatic: Option<bool>,
) -> Result<Value, String> {
    if action == "cascade" && (window.label() == "main" || window.label().starts_with("viewer-")) {
        cascade_viewers(window.app_handle())?;
        return Ok(json!({"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?}));
    }
    if !window.label().starts_with("viewer-") {
        return Err("不是查看窗口".into());
    }
    let result = match action.as_str() {
        "connect" => {
            let main = window
                .app_handle()
                .get_webview_window("main")
                .ok_or("主窗口不存在")?;
            main.eval("window.dispatchEvent(new Event('farsail:connect-device'))")
                .map_err(|e| e.to_string())?;
            main.show().map_err(|e| e.to_string())?;
            main.unminimize().map_err(|e| e.to_string())?;
            main.set_focus().map_err(|e| e.to_string())?;
            Ok(())
        }
        "phone_layout" => {
            // Window lifetime, rather than session lifetime: recovery, tab switches
            // and remote rotation must never override the user's chosen geometry.
            if automatic.unwrap_or(false)
                && !window
                    .state::<Windows>()
                    .1
                    .lock()
                    .unwrap()
                    .insert(window.label().to_owned())
            {
                return Ok(json!({"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?}));
            }
            let preferred = if landscape.unwrap_or(false) {
                (960.0_f64, 600.0_f64)
            } else {
                (440.0_f64, 900.0_f64)
            };
            let monitor = window.current_monitor().map_err(|e| e.to_string())?;
            let (width, height) = monitor
                .map(|m| {
                    let size = m.work_area().size.to_logical::<f64>(m.scale_factor());
                    (
                        preferred.0.min(size.width * 0.9),
                        preferred.1.min(size.height * 0.9),
                    )
                })
                .unwrap_or(preferred);
            window.unmaximize().map_err(|e| e.to_string())?;
            window
                .set_min_size(Some(tauri::LogicalSize::new(360.0, 420.0)))
                .map_err(|e| e.to_string())?;
            window
                .set_size(tauri::LogicalSize::new(width.max(360.0), height.max(420.0)))
                .map_err(|e| e.to_string())?;
            window
                .set_title("FarSail · 远程手机")
                .map_err(|e| e.to_string())?;
            let index = window
                .state::<Windows>()
                .0
                .lock()
                .unwrap()
                .get(window.label())
                .and_then(|binding| binding.instance.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1);
            place_viewer(&window, index)?;
            Ok(())
        }
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
    scoped_binding(&map, window.label(), id)
}
fn scoped_binding(
    map: &HashMap<String, Arc<Binding>>,
    label: &str,
    id: &str,
) -> Result<(), String> {
    let binding = map
        .values()
        .find(|binding| binding.host == label && *binding.id.lock().unwrap() == id)
        .ok_or("此窗口无权操作该会话")?;
    if binding.cancelled.load(Ordering::SeqCst) {
        return Err("此窗口无权操作该会话".into());
    }
    Ok(())
}
fn window_binding(
    window: &tauri::WebviewWindow,
    instance: Option<&str>,
) -> Result<Arc<Binding>, String> {
    let windows = window.state::<Windows>();
    let map = windows.0.lock().unwrap();
    let binding = if let Some(instance) = instance {
        map.values()
            .find(|binding| binding.host == window.label() && binding.instance == instance)
    } else {
        map.get(window.label())
            .filter(|binding| binding.host == window.label())
    };
    binding.cloned().ok_or("查看标签已关闭".into())
}

#[tauri::command]
pub fn viewer_tabs(window: tauri::WebviewWindow) -> Result<Value, String> {
    if !window.label().starts_with("viewer-") {
        return Err("不是查看窗口".into());
    }
    let windows = window.state::<Windows>();
    let map = windows.0.lock().unwrap();
    let mut bindings: Vec<_> = map.values().filter(|b| b.host == window.label()).collect();
    bindings.sort_by_key(|b| b.instance.parse::<u64>().unwrap_or(0));
    Ok(json!(bindings.iter().map(|b| json!({"instance":b.instance,"id":*b.id.lock().unwrap(),"name":b.name,"selected":b.selected.load(Ordering::SeqCst)})).collect::<Vec<_>>()))
}
#[tauri::command]
pub fn viewer_select_tab(window: tauri::WebviewWindow, instance: String) -> Result<(), String> {
    let binding = window_binding(&window, Some(&instance))?;
    let windows = window.state::<Windows>();
    let map = windows.0.lock().unwrap();
    for b in map.values().filter(|b| b.host == binding.host) {
        b.selected.store(b.instance == instance, Ordering::SeqCst);
    }
    Ok(())
}
#[tauri::command]
pub async fn viewer_close_tab(
    window: tauri::WebviewWindow,
    instance: String,
) -> Result<(), String> {
    let binding = window_binding(&window, Some(&instance))?;
    // Cancel synchronously before any await; late approval cannot revive a tab.
    binding.cancel_recovery();
    let empty = {
        let windows = window.state::<Windows>();
        let mut map = windows.0.lock().unwrap();
        map.retain(|_, b| !Arc::ptr_eq(b, &binding));
        let mut remaining: Vec<_> = map.values().filter(|b| b.host == window.label()).collect();
        remaining.sort_by_key(|b| b.instance.parse::<u64>().unwrap_or(0));
        if binding.selected.load(Ordering::SeqCst)
            && let Some(next) = remaining.first()
        {
            next.selected.store(true, Ordering::SeqCst);
        }
        remaining.is_empty()
    };
    let id = binding.id.lock().unwrap().clone();
    let remote = window.state::<Arc<RemoteRuntime>>().inner().clone();
    let client = window.state::<Arc<NativeClient>>().inner().clone();
    if empty {
        window.close().map_err(|e| e.to_string())?;
    }
    remote.stop(&id).await;
    let _ = client.call("revoke_remote", json!({"id":id})).await;
    Ok(())
}

fn focus_existing(
    app: &tauri::AppHandle,
    _label: &str,
    binding: &Binding,
) -> Result<String, String> {
    if !binding.opened.load(Ordering::SeqCst) {
        return Err("查看窗口正在打开，请稍后再试".into());
    }
    let viewer = app
        .get_webview_window(&binding.host)
        .ok_or("查看窗口已关闭")?;
    {
        let windows = app.state::<Windows>();
        for b in windows
            .0
            .lock()
            .unwrap()
            .values()
            .filter(|b| b.host == binding.host)
        {
            b.selected
                .store(b.instance == binding.instance, Ordering::SeqCst);
        }
    }
    viewer.unminimize().map_err(|e| e.to_string())?;
    if !binding.window_active(true) {
        return Err("查看窗口正在关闭，请重新连接".into());
    }
    viewer.set_focus().map_err(|e| e.to_string())?;
    if !binding.window_active(app.get_webview_window(&binding.host).is_some()) {
        return Err("查看窗口已关闭".into());
    }
    Ok(binding.instance.clone())
}

#[tauri::command]
pub fn viewer_window_active(
    window: tauri::WebviewWindow,
    instance: String,
) -> Result<bool, String> {
    main_only(&window)?;
    let app = window.app_handle();
    let windows = app.state::<Windows>();
    let tracked = {
        let map = windows.0.lock().unwrap();
        map.iter()
            .find(|(_, binding)| binding.instance == instance)
            .map(|(label, binding)| (label.clone(), binding.clone()))
    };
    Ok(tracked.is_some_and(|(_label, binding)| {
        binding.window_active(app.get_webview_window(&binding.host).is_some())
    }))
}

#[tauri::command]
pub async fn viewer_open(
    window: tauri::WebviewWindow,
    id: String,
    separate: Option<bool>,
) -> Result<String, String> {
    main_only(&window)?;
    if id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
        return Err("invalid session".into());
    }
    let app = window.app_handle();
    if app.state::<Arc<RemoteRuntime>>().is_updating() {
        return Err("正在安装更新，请稍后重新连接。".into());
    }
    let label = format!("viewer-{id}");
    let existing = app
        .state::<Windows>()
        .0
        .lock()
        .unwrap()
        .get(&label)
        .cloned();
    if let Some(binding) = existing {
        return focus_existing(app, &label, &binding);
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
    let host = if separate.unwrap_or(false) {
        None
    } else {
        let windows = app.state::<Windows>();
        let map = windows.0.lock().unwrap();
        map.values()
            .filter(|b| b.window_active(app.get_webview_window(&b.host).is_some()))
            .min_by_key(|b| b.instance.parse::<u64>().unwrap_or(0))
            .map(|b| b.host.clone())
    }
    .unwrap_or_else(|| label.clone());
    let devices = client.call("devices", json!({})).await.ok();
    let name = devices
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|d| d["id"] == row["target_device_id"]))
        .and_then(|d| d["name"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "设备 {}",
                row["target_device_id"]
                    .as_str()
                    .unwrap_or("未知")
                    .chars()
                    .take(8)
                    .collect::<String>()
            )
        });
    let binding = Arc::new(Binding {
        host: host.clone(),
        name,
        selected: AtomicBool::new(true),
        id: Mutex::new(id.clone()),
        instance: next_window_instance(),
        opened: AtomicBool::new(false),
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
        if let Some(existing) = map.get(&label).cloned() {
            drop(map);
            return focus_existing(app, &label, &existing);
        }
        for b in map.values().filter(|b| b.host == host) {
            b.selected.store(false, Ordering::SeqCst);
        }
        map.insert(label.clone(), binding.clone());
    }
    if host != label {
        binding.opened.store(true, Ordering::SeqCst);
        match focus_existing(app, &label, &binding) {
            Ok(instance) => return Ok(instance),
            Err(error) => {
                // The group can close while the target name is being loaded.
                // Never leave an invisible, unowned transport behind.
                binding.cancel_recovery();
                app.state::<Windows>()
                    .0
                    .lock()
                    .unwrap()
                    .retain(|_, b| !Arc::ptr_eq(b, &binding));
                app.state::<Arc<RemoteRuntime>>().stop(&id).await;
                return Err(error);
            }
        }
    }
    let monitor = window.current_monitor().map_err(|e| e.to_string())?;
    let (width, height) = monitor
        .as_ref()
        .map(|monitor| {
            let area = monitor
                .work_area()
                .size
                .to_logical::<f64>(monitor.scale_factor());
            (
                1000.0_f64.min((area.width - 96.0).max(640.0)),
                720.0_f64.min((area.height - 96.0).max(420.0)),
            )
        })
        .unwrap_or((1000.0, 720.0));
    let index = binding
        .instance
        .parse::<usize>()
        .unwrap_or(1)
        .saturating_sub(1);
    let position = monitor
        .as_ref()
        .map(|monitor| {
            let factor = monitor.scale_factor();
            let area = monitor.work_area();
            let origin = area.position.to_logical::<f64>(factor);
            let size = area.size.to_logical::<f64>(factor);
            cascade_position(
                (origin.x, origin.y),
                (size.width, size.height),
                (width, height),
                index,
            )
        })
        .unwrap_or((80.0, 80.0));
    let result = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App(format!("index.html?viewer={id}").into()),
    )
    .title("FarSail · 远程桌面")
    .inner_size(width, height)
    .position(position.0, position.1)
    .min_inner_size(640.0, 420.0)
    .maximized(false)
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
        {
            let windows = app.state::<Windows>();
            let mut map = windows.0.lock().unwrap();
            if map
                .get(&label)
                .is_some_and(|tracked| Arc::ptr_eq(tracked, &binding))
            {
                map.remove(&label);
            }
        }
        binding.cancel_recovery();
        app.state::<Arc<RemoteRuntime>>().stop(&id).await;
        return Err(e.to_string());
    }
    binding.opened.store(true, Ordering::SeqCst);
    // Creation succeeded even if the user immediately closed the new window.
    // The main page checks this instance before publishing its success notice.
    Ok(binding.instance.clone())
}

#[cfg(debug_assertions)]
pub fn smoke_setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    if std::env::var_os("FARSAIL_VIEWER_SMOKE").is_none() {
        return Ok(());
    }
    let id = "00000000-0000-0000-0000-000000000001";
    let binding = Arc::new(Binding {
        host: "viewer-smoke".into(),
        name: "合成手机".into(),
        selected: AtomicBool::new(true),
        id: Mutex::new(id.into()),
        instance: next_window_instance(),
        opened: AtomicBool::new(false),
        source: "synthetic".into(),
        target: "synthetic".into(),
        permission: RemotePermission::View,
        cancelled: AtomicBool::new(false),
        retrying: AtomicBool::new(false),
        attempts: AtomicU32::new(0),
        recovery_allowed: AtomicBool::new(false),
        progress: Mutex::new(RecoveryProgress::default()),
    });
    app.state::<Windows>()
        .0
        .lock()
        .unwrap()
        .insert("viewer-smoke".into(), binding.clone());
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
    binding.opened.store(true, Ordering::SeqCst);
    for number in 2..=3 {
        let id = format!("00000000-0000-0000-0000-{number:012}");
        let label = format!("viewer-cascade-{number}");
        let binding = Arc::new(Binding {
            host: label.clone(),
            name: "合成手机".into(),
            selected: AtomicBool::new(true),
            id: Mutex::new(id.clone()),
            instance: next_window_instance(),
            opened: AtomicBool::new(true),
            source: "synthetic".into(),
            target: "synthetic".into(),
            permission: RemotePermission::View,
            cancelled: AtomicBool::new(false),
            retrying: AtomicBool::new(false),
            attempts: AtomicU32::new(0),
            recovery_allowed: AtomicBool::new(false),
            progress: Mutex::new(RecoveryProgress::default()),
        });
        app.state::<Windows>()
            .0
            .lock()
            .unwrap()
            .insert(label.clone(), binding);
        tauri::WebviewWindowBuilder::new(
            app,
            &label,
            tauri::WebviewUrl::App(format!("index.html?viewer={id}").into()),
        )
        .title("FarSail cascade fixture")
        .inner_size(640.0, 620.0)
        .decorations(false)
        .build()?;
    }
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
        for(let n=0;n<100 && !document.querySelector('.viewer-titlebar');n++) await new Promise(r=>setTimeout(r,100));
        const initial=await invoke('ipc_viewer_state');
        if(!initial.maximized) throw Error('viewer did not open maximized');
        if(initial.decorations || !document.querySelector('.viewer-titlebar')) throw Error('custom titlebar missing');
        for(const [cmd,args] of [['state',{}],['update_status',{}],['update_preferences',{preferences:{autoCheck:false,autoInstall:true}}],['update_check',{}],['update_download',{}],['update_install',{}],['call',{op:'admin_users',args:{}}],['set_server',{server:'http://127.0.0.1:1'}],['files_status',{}],['app_exit',{}],['files_enable',{enabled:true}],['remote_paste_files',{id:'other'}],['remote_send_files',{id:'other'}],['files_send',{id:'other'}],['files_accept',{id:'other',transferId:'other'}],['files_reject',{id:'other',transferId:'other'}],['files_cancel',{id:'other',transferId:'other'}],['share_enable',{}],['remote_watch',{enabled:true}],['remote_input',{id:'other',input:null}],['media_profile',{id:'other',profile:1}],['transport_connect',{id:'other',permission:'view'}],['plugin:window|close',{label:'main'}]]) {
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
        const toggleFullscreen=()=>[...document.querySelectorAll('button')].find(b=>b.getAttribute('aria-label')==='全屏切换').click();
        toggleFullscreen();
        await new Promise(r=>setTimeout(r,300));
        const fullscreen=await invoke('ipc_viewer_state');
        if(!fullscreen.fullscreen) throw Error('fullscreen failed');
        if(document.querySelector('.viewer-titlebar')) throw Error('titlebar should hide in fullscreen');
        toggleFullscreen();
        await new Promise(r=>setTimeout(r,300));
        if(!document.querySelector('.viewer-titlebar')) throw Error('titlebar should return after fullscreen');
        await invoke('viewer_window_action',{action:'phone_layout'});
        await new Promise(r=>setTimeout(r,300));
        const phone=await invoke('ipc_viewer_state');
        if(phone.maximized || phone.width>641 || phone.height>901 || phone.height<=phone.width) throw Error('phone window sizing failed');
        await invoke('viewer_window_action',{action:'phone_layout',automatic:true});
        await invoke('viewer_window_action',{action:'phone_layout',landscape:true});
        await new Promise(r=>setTimeout(r,300));
        const game=await invoke('ipc_viewer_state');
        await invoke('viewer_window_action',{action:'phone_layout',landscape:false,automatic:true});
        const preserved=await invoke('ipc_viewer_state');
        if(Math.abs(preserved.width-game.width)>2 || Math.abs(preserved.height-game.height)>2) throw Error('automatic recovery layout overwrote manual window size');
        if(game.maximized || game.width<=game.height) throw Error('landscape game window sizing failed');
        await invoke('viewer_window_action',{action:'cascade'});
        await new Promise(r=>setTimeout(r,300));
        const cascade=await invoke('ipc_viewer_state');
        if(cascade.viewers.length!==3 || new Set(cascade.viewers.map(w=>`${w.x},${w.y}`)).size!==3) throw Error('independent viewers did not cascade');
        if(cascade.viewers.some(w=>w.maximized || w.fullscreen || w.minimized)) throw Error('cascade did not restore viewer windows');
        await invoke('viewer_window_action',{action:'connect'});
        const connectMain=await invoke('ipc_viewer_state');
        if(!connectMain.mainVisible || connectMain.mainMinimized) throw Error('connect did not restore main window');
        await invoke('viewer_window_action',{action:'minimize'});
        await new Promise(r=>setTimeout(r,300));
        const minimized=await invoke('ipc_viewer_state');
        if(!minimized.minimized || !minimized.mainExists) throw Error('minimize/main isolation failed');
        await invoke('ipc_smoke_report',{result:JSON.stringify({ok:true,denied,initial,restored,maximized,fullscreen,phone,game,preserved,cascade,connectMain,minimized})});
        await invoke('viewer_window_action',{action:'close'});
      } catch(e) { await invoke('ipc_smoke_report',{result:JSON.stringify({ok:false,error:String(e),denied})}); }
    },800)"#);
}

#[cfg(debug_assertions)]
#[tauri::command]
pub fn ipc_viewer_state(window: tauri::WebviewWindow) -> Result<Value, String> {
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    let viewers: Vec<Value> = window.app_handle().webview_windows().values()
        .filter(|viewer| viewer.label().starts_with("viewer-"))
        .filter_map(|viewer| {
            let position = viewer.outer_position().ok()?;
            Some(json!({"x":position.x,"y":position.y,"maximized":viewer.is_maximized().ok()?,"fullscreen":viewer.is_fullscreen().ok()?,"minimized":viewer.is_minimized().ok()?}))
        }).collect();
    Ok(
        json!({"width":size.width,"height":size.height,"decorations":window.is_decorated().map_err(|e|e.to_string())?,"maximized":window.is_maximized().map_err(|e|e.to_string())?,"fullscreen":window.is_fullscreen().map_err(|e|e.to_string())?,"minimized":window.is_minimized().map_err(|e|e.to_string())?,"mainExists":window.app_handle().get_webview_window("main").is_some(),"mainVisible":window.app_handle().get_webview_window("main").and_then(|main|main.is_visible().ok()),"mainMinimized":window.app_handle().get_webview_window("main").and_then(|main|main.is_minimized().ok()),"viewers":viewers}),
    )
}
pub fn cancel(app: &tauri::AppHandle, label: &str) -> Vec<String> {
    let windows = app.state::<Windows>();
    let mut map = windows.0.lock().unwrap();
    let mut ids = Vec::new();
    map.retain(|_, b| {
        if b.host != label {
            return true;
        }
        b.cancel_recovery();
        ids.push(b.id.lock().unwrap().clone());
        false
    });
    windows.1.lock().unwrap().remove(label);
    ids
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
pub fn viewer_recovery_status(
    window: tauri::WebviewWindow,
    instance: Option<String>,
) -> Result<Value, String> {
    let binding = window_binding(&window, instance.as_deref())?;
    Ok(binding.recovery_status(Instant::now()))
}
#[tauri::command]
pub async fn viewer_reconnect(
    window: tauri::WebviewWindow,
    manual: Option<bool>,
    instance: Option<String>,
) -> Result<Value, String> {
    let app = window.app_handle();
    let binding = window_binding(&window, instance.as_deref())?;
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
    #[test]
    fn cascade_keeps_titlebars_distinct_and_inside_scaled_work_areas() {
        for (origin, area, size) in [
            ((-1280.0, 40.0), (1280.0, 984.0), (640.0, 820.0)),
            ((0.0, 0.0), (1920.0, 1040.0), (1000.0, 720.0)),
        ] {
            let first = cascade_position(origin, area, size, 0);
            let second = cascade_position(origin, area, size, 1);
            assert_eq!((second.0 - first.0, second.1 - first.1), (36.0, 36.0));
            for index in 0..100 {
                let (x, y) = cascade_position(origin, area, size, index);
                assert!(x >= origin.0 && y >= origin.1);
                assert!(x + size.0 <= origin.0 + area.0);
                assert!(y + size.1 <= origin.1 + area.1);
            }
        }
    }
    fn binding() -> Binding {
        Binding {
            host: "viewer-a".into(),
            name: "测试设备".into(),
            selected: AtomicBool::new(true),
            id: Mutex::new("synthetic".into()),
            instance: next_window_instance(),
            opened: AtomicBool::new(true),
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
    fn concurrent_windows_keep_session_scope_and_recovery_independent() {
        let a = Arc::new(binding());
        let mut other = binding();
        other.host = "viewer-b".into();
        let b = Arc::new(other);
        *a.id.lock().unwrap() = "phone-a".into();
        *b.id.lock().unwrap() = "phone-b".into();
        let map = HashMap::from([
            ("viewer-a".into(), a.clone()),
            ("viewer-b".into(), b.clone()),
        ]);
        assert_ne!(a.instance, b.instance);
        assert!(scoped_binding(&map, "viewer-a", "phone-a").is_ok());
        assert!(scoped_binding(&map, "viewer-b", "phone-b").is_ok());
        assert!(scoped_binding(&map, "viewer-a", "phone-b").is_err());
        assert!(scoped_binding(&map, "viewer-b", "phone-a").is_err());
        a.begin_recovery(false, true).unwrap();
        *a.id.lock().unwrap() = "phone-a-new".into();
        assert!(scoped_binding(&map, "viewer-a", "phone-a").is_err());
        assert!(scoped_binding(&map, "viewer-a", "phone-a-new").is_ok());
        a.cancel_recovery();
        assert!(scoped_binding(&map, "viewer-a", "phone-a-new").is_err());
        assert!(scoped_binding(&map, "viewer-b", "phone-b").is_ok());
        assert!(b.window_active(true));
        assert_eq!(b.recovery_status(Instant::now())["phase"], "idle");
    }
    #[test]
    fn tabs_share_only_their_own_window_and_close_independently() {
        let a = Arc::new(binding());
        let b = Arc::new(binding());
        *a.id.lock().unwrap() = "phone-a".into();
        *b.id.lock().unwrap() = "phone-b".into();
        let map = HashMap::from([
            ("viewer-a".into(), a.clone()),
            ("viewer-b".into(), b.clone()),
        ]);
        assert!(scoped_binding(&map, "viewer-a", "phone-b").is_ok());
        assert!(scoped_binding(&map, "viewer-other", "phone-b").is_err());
        a.cancel_recovery();
        assert!(scoped_binding(&map, "viewer-a", "phone-a").is_err());
        assert!(scoped_binding(&map, "viewer-a", "phone-b").is_ok());
        b.begin_recovery(false, true).unwrap();
        *b.id.lock().unwrap() = "phone-b-new".into();
        assert!(scoped_binding(&map, "viewer-a", "phone-b").is_err());
        assert!(scoped_binding(&map, "viewer-a", "phone-b-new").is_ok());
    }
    #[test]
    fn window_instance_survives_fresh_session_recovery() {
        let binding = binding();
        let instance = binding.instance.clone();
        binding.begin_recovery(false, true).unwrap();
        binding.next_attempt(Instant::now()).unwrap();
        *binding.id.lock().unwrap() = "fresh-request".into();
        binding.finish_recovery(true, false);
        assert_eq!(binding.instance, instance);
        assert!(binding.window_active(true));
    }
    #[test]
    fn only_a_built_present_uncancelled_window_is_active() {
        let binding = binding();
        assert!(binding.window_active(true));
        assert!(!binding.window_active(false));
        binding.opened.store(false, Ordering::SeqCst);
        assert!(!binding.window_active(true));
        binding.opened.store(true, Ordering::SeqCst);
        binding.cancel_recovery();
        assert!(!binding.window_active(true));
    }
    #[test]
    fn replacement_window_never_reuses_the_old_instance() {
        let old = binding();
        old.cancel_recovery();
        let replacement = binding();
        assert_eq!(*old.id.lock().unwrap(), *replacement.id.lock().unwrap());
        assert_ne!(old.instance, replacement.instance);
        assert!(!old.window_active(true));
        assert!(replacement.window_active(true));
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
