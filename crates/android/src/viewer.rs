//! Outgoing mobile viewer. No capture or accessibility permission is needed here.
use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use farsail_media::JpegFrame;
use farsail_transport::{Channel, Frame, Session};
use farsail_windows::{Display, Input};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

#[derive(Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputState {
    generation: u64,
    blocked: bool,
    message: Option<String>,
}
struct Image {
    display: u32,
    layout: u64,
    sequence: u64,
    at: Instant,
}
struct State {
    phase: String,
    error: Option<String>,
    android: bool,
    displays: Vec<Display>,
    image: Option<Image>,
    requested: Option<u32>,
    latest: Option<Vec<u8>>,
    input: InputState,
}
impl Default for State {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            error: None,
            android: false,
            displays: vec![],
            image: None,
            requested: None,
            latest: None,
            input: InputState::default(),
        }
    }
}
struct Live {
    session: Session,
    send: AsyncMutex<u64>,
    permits: Semaphore,
}
pub struct Viewer {
    client: Arc<NativeClient>,
    epoch: AtomicU64,
    live: Mutex<Option<Arc<Live>>>,
    state: Mutex<State>,
}
impl Viewer {
    pub fn new(client: Arc<NativeClient>) -> Arc<Self> {
        Arc::new(Self {
            client,
            epoch: AtomicU64::new(0),
            live: Mutex::new(None),
            state: Mutex::new(State::default()),
        })
    }
    pub fn begin(&self) -> u64 {
        let mut live = self.live.lock().unwrap();
        let ticket = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(old) = live.take() {
            old.session.close();
        }
        *self.state.lock().unwrap() = State {
            phase: "requesting".into(),
            ..State::default()
        };
        ticket
    }
    pub fn current(&self, ticket: u64) -> bool {
        ticket != 0 && self.epoch.load(Ordering::SeqCst) == ticket
    }
    pub fn close(&self, ticket: u64) {
        let mut live = self.live.lock().unwrap();
        if !self.current(ticket) {
            return;
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if let Some(old) = live.take() {
            old.session.close();
        }
        *self.state.lock().unwrap() = State {
            phase: "closed".into(),
            ..State::default()
        };
    }
    pub fn cancel(&self) {
        self.close(self.epoch.load(Ordering::SeqCst));
    }
    fn phase(&self, ticket: u64, phase: &str, error: Option<String>) {
        let _live = self.live.lock().unwrap();
        if self.current(ticket) {
            let mut s = self.state.lock().unwrap();
            s.phase = phase.into();
            s.error = error;
        }
    }
    pub fn frame(&self, ticket: u64) -> Option<Vec<u8>> {
        let _live = self.live.lock().unwrap();
        if !self.current(ticket) {
            return None;
        }
        self.state.lock().unwrap().latest.take()
    }
    pub fn status(&self, ticket: u64) -> Value {
        let live = self.live.lock().unwrap();
        if !self.current(ticket) {
            return json!({"phase":"closed","control":false});
        }
        let s = self.state.lock().unwrap();
        let session = live.as_ref().map(|l| &l.session);
        let open = session.is_some_and(Session::is_open_now);
        let control = open
            && session.is_some_and(|s| s.permission() == RemotePermission::Control)
            && !s.input.blocked
            && s.image
                .as_ref()
                .is_some_and(|i| i.at.elapsed() < Duration::from_secs(5));
        let path = session.map(Session::path);
        json!({"phase":if session.is_some() && !open { "closed" } else { &s.phase },"error":s.error,
            "platform":if s.android {"android"} else {"desktop"},"control":control,"input":s.input,
            "path":path.map(|p|p.0),"rtt_ms":path.and_then(|p|p.1),"displays":s.displays,
            "verification_code":session.and_then(|s|s.verification_code().ok())})
    }
    pub async fn connect(
        self: &Arc<Self>,
        ticket: u64,
        target: &str,
        permission: RemotePermission,
    ) -> Result<Value, String> {
        if !self.current(ticket) {
            return Err("连接已取消".into());
        }
        let owner = self.client.public_state().await;
        let source = owner["deviceId"].as_str().ok_or("请先添加这台手机")?;
        if !owner["signedIn"].as_bool().unwrap_or(false) || source == target {
            return Err("请选择另一台已授权设备".into());
        }
        let request = self.client.call("request", json!({"source_device_id":source,"target_device_id":target,"permission":permission})).await.map_err(|e|e.to_string())?;
        let id = request["id"].as_str().ok_or("连接请求无效")?.to_owned();
        let result: Result<Value, String> = async {
            self.phase(ticket, "waiting", None);
            let deadline = Instant::now() + Duration::from_secs(90);
            loop {
                if !self.current(ticket) {
                    return Err("连接已取消".into());
                }
                if Instant::now() >= deadline {
                    return Err("等待批准超时，请重新连接".into());
                }
                let r = self
                    .client
                    .call("remote_status", json!({"id":id}))
                    .await
                    .map_err(|e| e.to_string())?;
                match r["state"].as_str() {
                    Some("approved") => break,
                    Some("pending") => tokio::time::sleep(Duration::from_millis(500)).await,
                    _ => return Err("对方拒绝、取消或结束了连接".into()),
                }
            }
            if !self.current(ticket) {
                return Err("连接已取消".into());
            }
            self.phase(ticket, "connecting", None);
            self.client
                .connect_transport(&id, permission)
                .await
                .map_err(|e| e.to_string())?;
            let session = self
                .client
                .transport_session(&id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("连接已结束")?;
            if session.permission() != permission {
                session.close();
                return Err("连接身份不匹配".into());
            }
            let live = Arc::new(Live {
                session,
                send: AsyncMutex::new(0),
                permits: Semaphore::new(32),
            });
            {
                let mut slot = self.live.lock().unwrap();
                if !self.current(ticket) {
                    live.session.close();
                    return Err("连接已取消".into());
                }
                *slot = Some(live.clone());
                self.state.lock().unwrap().phase = "connected".into();
            }
            let viewer = self.clone();
            let id = id.clone();
            let owner = owner.clone();
            tokio::spawn(async move {
                viewer.receive(ticket, live).await;
                viewer.cleanup(&id, &owner).await;
            });
            Ok(self.status(ticket))
        }
        .await;
        if let Err(error) = &result {
            self.phase(ticket, "closed", Some(error.clone()));
            self.cleanup(&id, &owner).await;
        }
        result
    }
    async fn cleanup(&self, id: &str, owner: &Value) {
        let _ = self.client.close_transport_session(id).await;
        let now = self.client.public_state().await;
        if now["server"] == owner["server"] && now["deviceId"] == owner["deviceId"] {
            let _ = self.client.call("revoke_remote", json!({"id":id})).await;
        }
    }
    async fn receive(self: &Arc<Self>, ticket: u64, live: Arc<Live>) {
        let viewer = self.clone();
        let heartbeat = tokio::spawn(async move {
            while viewer.current(ticket) {
                tokio::time::sleep(Duration::from_secs(15)).await;
                if !viewer.current(ticket) {
                    break;
                }
                if viewer.client.call("heartbeat", json!({})).await.is_err() {
                    viewer.close(ticket);
                    break;
                }
            }
        });
        while self.current(ticket) && live.session.is_open_now() {
            let frame = match live.session.receive().await {
                Ok(f) => f,
                Err(farsail_transport::Error::Timeout) => continue,
                Err(_) => break,
            };
            if frame.channel == Channel::Control {
                if frame.bytes.len() <= 768
                    && frame.bytes.starts_with(b"FSB1")
                    && let Ok(next) = serde_json::from_slice::<InputState>(&frame.bytes[4..])
                    && next.blocked == next.message.is_some()
                    && next.message.as_ref().is_none_or(|s| s.len() <= 640)
                {
                    let mut state = self.state.lock().unwrap();
                    if self.current(ticket) && next.generation > state.input.generation {
                        state.input = next;
                    }
                }
                continue;
            }
            if frame.channel != Channel::Media {
                continue;
            }
            if frame.bytes == b"FSH1android" {
                let mut s = self.state.lock().unwrap();
                if self.current(ticket) {
                    s.android = true;
                }
                continue;
            }
            if frame.bytes.starts_with(b"FSL1") && frame.bytes.len() <= 65536 {
                if let Ok(list) = serde_json::from_slice::<Vec<Display>>(&frame.bytes[4..])
                    && list.len() <= 32
                    && list.iter().all(|d| d.width > 0 && d.height > 0)
                {
                    let mut s = self.state.lock().unwrap();
                    if self.current(ticket) {
                        s.displays = list;
                    }
                }
                continue;
            }
            if !frame.bytes.starts_with(b"FSM1") {
                continue;
            }
            let image = match JpegFrame::from_wire(&frame.bytes) {
                Ok(i) => i,
                Err(_) => break,
            };
            let sequence = image.meta.sequence;
            {
                let mut s = self.state.lock().unwrap();
                if self.current(ticket)
                    && s.requested.is_none_or(|id| id == image.meta.monitor)
                    && s.image.as_ref().is_none_or(|old| sequence > old.sequence)
                {
                    s.image = Some(Image {
                        display: image.meta.monitor,
                        layout: image.meta.layout,
                        sequence,
                        at: Instant::now(),
                    });
                    s.latest = Some(frame.bytes);
                }
            }
            let mut bytes = b"FSA1".to_vec();
            bytes.extend_from_slice(&sequence.to_be_bytes());
            if live
                .session
                .send(Frame {
                    channel: Channel::Media,
                    bytes,
                })
                .await
                .is_err()
            {
                break;
            }
        }
        heartbeat.abort();
        live.session.close();
        self.phase(ticket, "closed", Some("远程连接已结束，请重新连接".into()));
    }
    fn active(&self, ticket: u64) -> Result<Arc<Live>, String> {
        let live = self.live.lock().unwrap();
        if !self.current(ticket) {
            return Err("连接已结束".into());
        }
        live.as_ref()
            .filter(|l| l.session.is_open_now())
            .cloned()
            .ok_or("连接未就绪".into())
    }
    pub async fn input(&self, ticket: u64, value: Value, observed: u64) -> Result<Value, String> {
        let live = self.active(ticket)?;
        if live.session.permission() != RemotePermission::Control {
            return Err("当前连接仅可查看".into());
        }
        let _permit = live.permits.try_acquire().map_err(|_| {
            live.session.close();
            "输入队列已满".to_string()
        })?;
        let mut sequence = live.send.lock().await;
        if !self.current(ticket) || !live.session.is_open_now() {
            return Err("连接已结束".into());
        }
        let payload = if value.is_null() {
            b"FSR1".to_vec()
        } else if value == json!({"kind":"resume_control"}) {
            b"FSR2".to_vec()
        } else if value == json!({"kind":"resume_mouse"}) {
            b"FSR3".to_vec()
        } else {
            let input: Input = serde_json::from_value(value).map_err(|_| "输入无效")?;
            let s = self.state.lock().unwrap();
            if s.input.blocked
                || s.input.generation != observed
                || s.image
                    .as_ref()
                    .is_none_or(|i| i.at.elapsed() >= Duration::from_secs(5))
            {
                return Ok(Value::Null);
            }
            let valid = match &input {
                Input::Move {
                    display,
                    layout,
                    x,
                    y,
                }
                | Input::Button {
                    display,
                    layout,
                    x,
                    y,
                    ..
                }
                | Input::Wheel {
                    display,
                    layout,
                    x,
                    y,
                    ..
                } => {
                    s.image
                        .as_ref()
                        .is_some_and(|i| i.display == *display && i.layout == *layout)
                        && x.is_finite()
                        && y.is_finite()
                        && (0.0..=1.0).contains(x)
                        && (0.0..=1.0).contains(y)
                }
                Input::Key { vk, .. } => *vk <= 255,
                Input::Text { text } => text.len() <= 768,
            };
            if !valid {
                return Ok(Value::Null);
            }
            if let Input::Wheel {
                vertical,
                horizontal,
                ..
            } = &input
                && (!(-1200..=1200).contains(vertical) || !(-1200..=1200).contains(horizontal))
            {
                return Err("滚动输入无效".into());
            }
            serde_json::to_vec(&input).map_err(|_| "输入无效")?
        };
        if payload.len() + 12 > 1024 {
            return Err("输入内容过长".into());
        }
        *sequence += 1;
        let mut bytes = b"FSI1".to_vec();
        bytes.extend_from_slice(&sequence.to_be_bytes());
        bytes.extend(payload);
        live.session
            .send(Frame {
                channel: Channel::Control,
                bytes,
            })
            .await
            .map_err(|e| {
                live.session.close();
                e.to_string()
            })?;
        Ok(Value::Null)
    }
    pub async fn select(&self, ticket: u64, display: u32) -> Result<Value, String> {
        let live = self.active(ticket)?;
        {
            let mut s = self.state.lock().unwrap();
            if !self.current(ticket) || !s.displays.iter().any(|d| d.id == display) {
                return Err("屏幕已变化，请重新选择".into());
            }
            s.requested = Some(display);
            s.latest = None;
            s.image = None;
        }
        if live.session.permission() == RemotePermission::Control {
            self.input(ticket, Value::Null, 0).await?;
        }
        let mut bytes = b"FSS1".to_vec();
        bytes.extend_from_slice(&display.to_be_bytes());
        live.session
            .send(Frame {
                channel: Channel::Media,
                bytes,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(Value::Null)
    }
}
