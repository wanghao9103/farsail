use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use farsail_media::{BASELINE_PAYLOAD_BPS, ByteBudget, JpegFrame, wait_for_ack};
use farsail_transport::{Channel, Frame, Session};
use farsail_windows::{Capture, Display, Input, InputSink, displays};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Semaphore, watch};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputState {
    generation: u64,
    blocked: bool,
    message: Option<String>,
}
impl InputState {
    fn update(&mut self, error: Option<&farsail_windows::Error>) {
        self.generation = self.generation.saturating_add(1);
        self.blocked = error.is_some();
        self.message = error.map(|e| match e {
            farsail_windows::Error::Injection { inserted, expected, code } => format!("被控端暂停鼠标键盘控制：Windows 已执行 {inserted}/{expected} 个输入，错误码 {code}。请让对方返回普通桌面或应用后，点击“重试控制”。画面连接仍保留。"),
            _ => "被控端暂停鼠标键盘控制，尚未能松开全部按键。请让对方返回普通桌面或应用后，点击“重试控制”。画面连接仍保留。".into(),
        });
    }
    fn packet(&self) -> Vec<u8> {
        let mut bytes = b"FSB1".to_vec();
        bytes.extend(serde_json::to_vec(self).unwrap());
        bytes
    }
    fn accept(&mut self, bytes: &[u8]) {
        if bytes.len() > 768 || !bytes.starts_with(b"FSB1") {
            return;
        }
        if let Ok(next) = serde_json::from_slice::<Self>(&bytes[4..])
            && next.generation > self.generation
            && next.blocked == next.message.is_some()
            && next
                .message
                .as_ref()
                .is_none_or(|message| message.len() <= 640)
        {
            *self = next;
        }
    }
}

fn ended_message(reason: &str) -> &'static str {
    match reason {
        "media_timeout" => "画面传输连续超时，正在等待重新授权连接",
        "network_timeout" => "网络连接中断，正在等待重新授权连接",
        "lease_expired" => "授权租约到期，已停止画面和输入；请检查协调服务连接",
        "authorization_failed" => "授权检查失败或已被撤销，会话已安全结束",
        "renewal_failed" => "授权续期失败，会话已安全结束；请检查双方到协调服务的连接",
        "input_failed" => "被控端无法执行输入；请检查窗口权限、UAC 或桌面切换",
        "input_sequence" => "输入消息缺失或顺序异常，会话已安全结束",
        "capture_failed" => "被控端屏幕采集已停止",
        "stopped" => "会话已结束，画面与输入已停止",
        _ => "对方已结束连接或授权失效，画面与输入已停止",
    }
}

/// Independent QUIC streams can arrive out of order. Retain at most 32 small
/// commands for 2 seconds; never apply a later keyup before its keydown.
#[derive(Default)]
struct InputOrder {
    last: u64,
    pending: BTreeMap<u64, Vec<u8>>,
    deadline: Option<tokio::time::Instant>,
}
impl InputOrder {
    fn push(&mut self, seq: u64, bytes: Vec<u8>) -> Result<Vec<Vec<u8>>, ()> {
        if seq <= self.last || seq - self.last > 32 || self.pending.contains_key(&seq) {
            return Err(());
        }
        self.pending.insert(seq, bytes);
        let mut ready = Vec::new();
        while let Some(bytes) = self.pending.remove(&(self.last + 1)) {
            self.last += 1;
            ready.push(bytes);
        }
        if self.pending.is_empty() {
            self.deadline = None;
        } else if self.deadline.is_none() {
            self.deadline = Some(tokio::time::Instant::now() + Duration::from_secs(2));
        }
        Ok(ready)
    }
}

struct Live {
    session: Session,
    host: bool,
    alive: Arc<AtomicBool>,
    selected: Arc<AtomicU32>,
    layout: Arc<AtomicU64>,
    current: Arc<StdMutex<Option<(Display, u64)>>>,
    input: Arc<StdMutex<InputSink>>,
    input_state: Arc<StdMutex<InputState>>,
    displays: Arc<StdMutex<Vec<Display>>>,
    error: Arc<StdMutex<Option<String>>>,
    frame: watch::Receiver<Option<Vec<u8>>>,
    send_sequence: AtomicU64,
    send_lock: Mutex<()>,
    send_permits: Semaphore,
}

pub struct RemoteRuntime {
    client: Arc<NativeClient>,
    sessions: Mutex<HashMap<String, Arc<Live>>>,
    finished: Mutex<HashMap<String, Value>>,
}
impl RemoteRuntime {
    pub fn new(client: Arc<NativeClient>) -> Arc<Self> {
        let runtime = Arc::new(Self {
            client,
            sessions: Mutex::new(HashMap::new()),
            finished: Mutex::new(HashMap::new()),
        });
        let mut receiver = runtime.client.subscribe_sessions();
        let owner = runtime.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok((session, host)) => owner.attach(session, host).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        runtime
    }

    async fn attach(self: &Arc<Self>, session: Session, host: bool) {
        if session.permission() == RemotePermission::Files {
            return;
        }
        if host && !self.client.hosting_enabled() {
            session.close();
            return;
        }
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(session.id())
            || (host
                && sessions
                    .values()
                    .any(|s| s.host && s.alive.load(Ordering::SeqCst)))
        {
            session.close();
            return;
        }
        let layout = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |t| t.as_millis() as u64);
        let (tx, rx) = watch::channel(None);
        let live = Arc::new(Live {
            session: session.clone(),
            host,
            alive: Arc::new(AtomicBool::new(true)),
            selected: Arc::new(AtomicU32::new(1)),
            layout: Arc::new(AtomicU64::new(layout)),
            current: Arc::new(StdMutex::new(None)),
            input: Arc::new(StdMutex::new(InputSink::default())),
            input_state: Arc::new(StdMutex::new(InputState::default())),
            displays: Arc::new(StdMutex::new(Vec::new())),
            error: Arc::new(StdMutex::new(None)),
            frame: rx,
            send_sequence: AtomicU64::new(0),
            send_lock: Mutex::new(()),
            send_permits: Semaphore::new(32),
        });
        sessions.insert(session.id().into(), live.clone());
        drop(sessions);
        let owner = self.clone();
        tokio::spawn(async move {
            if host {
                owner.run_host(live.clone()).await;
            } else {
                owner.run_viewer(live.clone(), tx).await;
            }
            live.alive.store(false, Ordering::SeqCst);
            if let Err(e) = live.input.lock().unwrap().release_all() {
                *live.error.lock().unwrap() = Some(format!("input cleanup incomplete: {e}"));
            }
            let code = live.session.end_reason();
            live.session.close();
            let reason = live
                .error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| ended_message(code).into());
            let mut finished = owner.finished.lock().await;
            if finished.len() >= 32
                && let Some(old) = finished.keys().next().cloned()
            {
                finished.remove(&old);
            }
            finished.insert(live.session.id().into(),json!({"id":live.session.id(),"state":"closed","rtt_ms":null,"permission":live.session.permission(),"sharing":live.host,"verification_code":live.session.verification_code().ok(),"displays":*live.displays.lock().unwrap(),"error":reason,"reason":code,"retryable":matches!(code,"network_timeout" | "media_timeout")}));
            drop(finished);
            let mut sessions = owner.sessions.lock().await;
            if sessions
                .get(live.session.id())
                .is_some_and(|s| Arc::ptr_eq(s, &live))
            {
                sessions.remove(live.session.id());
            }
        });
    }

    async fn run_host(&self, live: Arc<Live>) {
        let list = match displays() {
            Ok(list) if !list.is_empty() => list,
            Ok(_) => {
                *live.error.lock().unwrap() = Some("no interactive display".into());
                self.client.disable_host_local();
                let _ = self.client.set_host_capability(false).await;
                return;
            }
            Err(e) => {
                *live.error.lock().unwrap() = Some(e.to_string());
                self.client.disable_host_local();
                let _ = self.client.set_host_capability(false).await;
                return;
            }
        };
        *live.displays.lock().unwrap() = list.clone();
        let mut packet = b"FSL1".to_vec();
        packet.extend_from_slice(&serde_json::to_vec(&list).unwrap());
        if live
            .session
            .send(Frame {
                channel: Channel::Media,
                bytes: packet,
            })
            .await
            .is_err()
        {
            return;
        }
        let (tx, mut rx) = watch::channel::<Option<Vec<u8>>>(None);
        let (ack_tx, mut ack_rx) = watch::channel(0u64);
        let alive = live.alive.clone();
        let selected = live.selected.clone();
        let current = live.current.clone();
        let error = live.error.clone();
        let capture_failed = Arc::new(AtomicBool::new(false));
        let worker_failed = capture_failed.clone();
        let layout = live.layout.clone();
        tokio::task::spawn_blocking(move || {
            let mut capture: Option<Capture> = None;
            let mut last_id = 0;
            let mut sequence = 0;
            while alive.load(Ordering::SeqCst) {
                let id = selected.load(Ordering::SeqCst);
                if id != last_id || capture.is_none() {
                    *current.lock().unwrap() = None;
                    let version = layout.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
                    capture = match Capture::new(id) {
                        Ok(c) => {
                            *current.lock().unwrap() = Some((c.display().clone(), version));
                            Some(c)
                        }
                        Err(e) => {
                            *error.lock().unwrap() = Some(e.to_string());
                            worker_failed.store(true, Ordering::SeqCst);
                            break;
                        }
                    };
                    last_id = id;
                }
                if let Some(c) = capture.as_mut() {
                    sequence += 1;
                    match c.next_frame(layout.load(Ordering::SeqCst), sequence) {
                        Ok(Some(frame)) => match frame.to_wire() {
                            Ok(bytes) => {
                                tx.send_replace(Some(bytes));
                            }
                            Err(e) => {
                                *error.lock().unwrap() = Some(e.to_string());
                                worker_failed.store(true, Ordering::SeqCst);
                                break;
                            }
                        },
                        Ok(None) => (),
                        Err(e) => {
                            *error.lock().unwrap() = Some(e.to_string());
                            worker_failed.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(180));
            }
        });
        let sender_live = live.clone();
        let sender = tokio::spawn(async move {
            let mut budget = ByteBudget::new(BASELINE_PAYLOAD_BPS);
            let mut missed = 0;
            'sending: loop {
                if missed == 0 && rx.changed().await.is_err() {
                    break;
                }
                let (bytes, sequence) = loop {
                    if !sender_live.alive.load(Ordering::SeqCst) {
                        break 'sending;
                    }
                    let Some(bytes) = rx.borrow_and_update().clone() else {
                        continue 'sending;
                    };
                    let frame = match JpegFrame::from_wire(&bytes) {
                        Ok(frame) => frame,
                        Err(_) => break 'sending,
                    };
                    let age = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(u64::MAX, |t| t.as_millis() as u64)
                        .saturating_sub(frame.meta.captured_ms);
                    if age > 700 && missed == 0 {
                        continue 'sending;
                    }
                    let wait = budget.charge(bytes.len());
                    if !wait.is_zero() {
                        tokio::time::sleep(wait.min(Duration::from_millis(200))).await;
                        continue;
                    }
                    break (bytes, frame.meta.sequence);
                };
                let sent = sender_live
                    .session
                    .send(Frame {
                        channel: Channel::Media,
                        bytes,
                    })
                    .await;
                if sent.is_err() && !matches!(sent, Err(farsail_transport::Error::Timeout)) {
                    break;
                }
                if sent.is_err()
                    || !wait_for_ack(&mut ack_rx, sequence, Duration::from_secs(2)).await
                {
                    missed += 1;
                    if missed >= 3 {
                        sender_live.session.close_with_reason("media_timeout");
                        break;
                    }
                } else {
                    missed = 0;
                }
            }
            sender_live.alive.store(false, Ordering::SeqCst);
            sender_live.session.close();
        });
        let mut input_order = InputOrder::default();
        let mut input_window = std::time::Instant::now();
        let mut input_count = 0u32;
        let mut move_count = 0u32;
        loop {
            if !live.alive.load(Ordering::SeqCst)
                || !self.client.hosting_enabled()
                || !live.session.is_open().await
            {
                break;
            }
            {
                let incoming = if input_order.pending.is_empty() {
                    live.session.receive().await
                } else {
                    match tokio::time::timeout_at(
                        input_order.deadline.unwrap(),
                        live.session.receive(),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => {
                            live.session.close_with_reason("input_sequence");
                            break;
                        }
                    }
                };
                let frame = match incoming {
                    Ok(f) => f,
                    Err(farsail_transport::Error::Timeout) => continue,
                    Err(_) => break,
                };
                if frame.channel == Channel::Media
                    && frame.bytes.len() == 8
                    && &frame.bytes[..4] == b"FSS1"
                {
                    let id = u32::from_be_bytes(frame.bytes[4..8].try_into().unwrap());
                    if live.displays.lock().unwrap().iter().any(|d| d.id == id) {
                        live.selected.store(id, Ordering::SeqCst);
                    }
                } else if frame.channel == Channel::Media
                    && frame.bytes.len() == 12
                    && &frame.bytes[..4] == b"FSA1"
                {
                    let sequence = u64::from_be_bytes(frame.bytes[4..12].try_into().unwrap());
                    ack_tx.send_replace(sequence);
                } else if frame.channel == Channel::Control
                    && live.session.permission() == RemotePermission::Control
                    && frame.bytes.len() <= 1024
                    && frame.bytes.len() > 12
                    && &frame.bytes[..4] == b"FSI1"
                {
                    let seq = u64::from_be_bytes(frame.bytes[4..12].try_into().unwrap());
                    let ready = match input_order.push(seq, frame.bytes[12..].to_vec()) {
                        Ok(ready) => ready,
                        Err(()) => {
                            live.session.close_with_reason("input_sequence");
                            break;
                        }
                    };
                    for payload in ready {
                        if input_window.elapsed() >= Duration::from_secs(1) {
                            input_window = std::time::Instant::now();
                            input_count = 0;
                            move_count = 0;
                        }
                        input_count += 1;
                        if input_count > 240 {
                            live.session.close_with_reason("input_failed");
                            *live.error.lock().unwrap() =
                                Some("input rate exceeded; session stopped safely".into());
                            break;
                        }
                        if !live.alive.load(Ordering::SeqCst)
                            || !self.client.hosting_enabled()
                            || !live.session.is_open_now()
                        {
                            break;
                        }
                        let mut notice = None;
                        {
                            let mut sink = live.input.lock().unwrap();
                            if !live.alive.load(Ordering::SeqCst)
                                || !self.client.hosting_enabled()
                                || !live.session.is_open_now()
                            {
                                break;
                            }
                            if &payload == b"FSR2" {
                                let result = sink.release_all();
                                let mut state = live.input_state.lock().unwrap();
                                state.update(result.as_ref().err());
                                notice = Some(state.packet());
                            } else if &payload == b"FSR1" {
                                if let Err(e) = sink.release_all() {
                                    let mut state = live.input_state.lock().unwrap();
                                    state.update(Some(&e));
                                    notice = Some(state.packet());
                                }
                            } else if let Ok(input) = serde_json::from_slice::<Input>(&payload) {
                                if live.input_state.lock().unwrap().blocked {
                                    continue;
                                }
                                if matches!(&input, Input::Move { .. }) {
                                    move_count += 1;
                                    if move_count > 120 {
                                        continue;
                                    }
                                }
                                let current = live.current.lock().unwrap().clone();
                                if let Err(e) = sink.apply(
                                    input,
                                    current.as_ref().map(|x| &x.0),
                                    current.as_ref().map_or(0, |x| x.1),
                                ) {
                                    if matches!(
                                        e,
                                        farsail_windows::Error::Geometry
                                            | farsail_windows::Error::UnsupportedInput
                                    ) {
                                        continue;
                                    }
                                    let _ = sink.release_all();
                                    let mut state = live.input_state.lock().unwrap();
                                    state.update(Some(&e));
                                    notice = Some(state.packet());
                                }
                            }
                        }
                        if let Some(bytes) = notice
                            && live
                                .session
                                .send(Frame {
                                    channel: Channel::Control,
                                    bytes,
                                })
                                .await
                                .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        }
        sender.abort();
        if capture_failed.load(Ordering::SeqCst) {
            live.session.close_with_reason("capture_failed");
            self.client.disable_host_local();
            let _ = self.client.set_host_capability(false).await;
        }
    }

    async fn run_viewer(&self, live: Arc<Live>, sender: watch::Sender<Option<Vec<u8>>>) {
        let mut last_sequence = 0;
        loop {
            if !live.alive.load(Ordering::SeqCst) || !live.session.is_open().await {
                break;
            }
            let frame = match live.session.receive().await {
                Ok(f) => f,
                Err(farsail_transport::Error::Timeout) => continue,
                Err(_) => break,
            };
            if frame.channel == Channel::Control
                && live.session.permission() == RemotePermission::Control
            {
                live.input_state.lock().unwrap().accept(&frame.bytes);
                continue;
            }
            if frame.channel != Channel::Media {
                continue;
            }
            if frame.bytes.starts_with(b"FSL1") {
                if let Ok(list) = serde_json::from_slice::<Vec<Display>>(&frame.bytes[4..]) {
                    *live.displays.lock().unwrap() = list;
                }
            } else if let Ok(image) = JpegFrame::from_wire(&frame.bytes) {
                if image.meta.sequence > last_sequence {
                    last_sequence = image.meta.sequence;
                    sender.send_replace(Some(frame.bytes));
                }
                let mut ack = b"FSA1".to_vec();
                ack.extend_from_slice(&last_sequence.to_be_bytes());
                if live
                    .session
                    .send(Frame {
                        channel: Channel::Media,
                        bytes: ack,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }

    pub async fn status(&self, id: &str) -> Result<Value, String> {
        let live = self.sessions.lock().await.get(id).cloned();
        let Some(live) = live else {
            if let Ok(Some(session)) = self.client.transport_session(id).await
                && session.is_open().await
            {
                return Ok(
                    json!({"id":id,"state":"connecting","permission":session.permission(),"displays":[],"error":null}),
                );
            }
            return self
                .finished
                .lock()
                .await
                .get(id)
                .cloned()
                .ok_or("remote session not active".into());
        };
        if !live.alive.load(Ordering::SeqCst) || !live.session.is_open().await {
            let reason = live.session.end_reason();
            return Ok(
                json!({"id":id,"state":"closed","permission":live.session.permission(),"displays":[],"rtt_ms":null,"verification_code":null,"error":ended_message(reason),"reason":reason,"retryable":matches!(reason,"network_timeout" | "media_timeout")}),
            );
        }
        let (path, rtt) = live.session.path();
        Ok(
            json!({"id":id,"state":path,"rtt_ms":rtt,"permission":live.session.permission(),"sharing":live.host,"verification_code":live.session.verification_code().map_err(|e|e.to_string())?,"displays":*live.displays.lock().unwrap(),"error":*live.error.lock().unwrap(),"input":*live.input_state.lock().unwrap()}),
        )
    }
    pub async fn next_frame(&self, id: &str, after: u64) -> Result<Vec<u8>, String> {
        let live = self.sessions.lock().await.get(id).cloned();
        let Some(live) = live else {
            if let Ok(Some(session)) = self.client.transport_session(id).await
                && session.is_open().await
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
                return Ok(Vec::new());
            }
            return Err("会话已结束，画面与输入已停止".into());
        };
        if !live.alive.load(Ordering::SeqCst) || !live.session.is_open().await {
            return Err("会话已结束，画面与输入已停止".into());
        }
        let mut receiver = live.frame.clone();
        for _ in 0..2 {
            if let Some(bytes) = receiver.borrow_and_update().as_ref()
                && let Ok(frame) = JpegFrame::from_wire(bytes)
                && frame.meta.sequence > after
            {
                return Ok(bytes.clone());
            }
            match tokio::time::timeout(Duration::from_millis(450), receiver.changed()).await {
                Ok(Ok(())) => (),
                Ok(Err(_)) => return Err("会话已结束，画面与输入已停止".into()),
                Err(_) => break,
            }
        }
        Ok(Vec::new())
    }
    pub async fn select(&self, id: &str, display: u32) -> Result<(), String> {
        let live = self
            .sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or("remote session not active")?;
        if live.host
            || !live
                .displays
                .lock()
                .unwrap()
                .iter()
                .any(|d| d.id == display)
        {
            return Err("display unavailable".into());
        }
        let mut bytes = b"FSS1".to_vec();
        bytes.extend_from_slice(&display.to_be_bytes());
        live.session
            .send(Frame {
                channel: Channel::Media,
                bytes,
            })
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn input(&self, id: &str, value: Option<Value>) -> Result<(), String> {
        let live = self
            .sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or("remote session not active")?;
        if live.host || live.session.permission() != RemotePermission::Control {
            return Err("control permission required".into());
        }
        let is_move = value
            .as_ref()
            .is_some_and(|v| v.get("kind").and_then(Value::as_str) == Some("move"));
        let resume = value.as_ref().and_then(Value::as_object).is_some_and(|v| {
            v.len() == 1 && v.get("kind").and_then(Value::as_str) == Some("resume_control")
        });
        let payload = if resume {
            b"FSR2".to_vec()
        } else if let Some(value) = value {
            let input: Input = serde_json::from_value(value).map_err(|_| "invalid input")?;
            serde_json::to_vec(&input).unwrap()
        } else {
            b"FSR1".to_vec()
        };
        if payload.len() + 12 > 1024 {
            return Err("input too large".into());
        }
        let _permit = match live.send_permits.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                live.session.close();
                return Err("input queue full; session stopped".into());
            }
        };
        let _send_guard = if is_move {
            match live.send_lock.try_lock() {
                Ok(guard) => guard,
                Err(_) => return Ok(()),
            }
        } else {
            live.send_lock.lock().await
        };
        if !live.alive.load(Ordering::SeqCst) || !live.session.is_open().await {
            return Err("会话已结束，输入已停止".into());
        }
        if live.input_state.lock().unwrap().blocked && payload != b"FSR1" && payload != b"FSR2" {
            return Ok(());
        }
        let mut bytes = b"FSI1".to_vec();
        bytes.extend_from_slice(
            &live
                .send_sequence
                .fetch_add(1, Ordering::SeqCst)
                .wrapping_add(1)
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&payload);
        live.session
            .send(Frame {
                channel: Channel::Control,
                bytes,
            })
            .await
            .map_err(|e| {
                live.session.close();
                e.to_string()
            })
    }
    pub async fn stop(&self, id: &str) {
        let live = self.sessions.lock().await.remove(id);
        if let Some(live) = live {
            live.session.close_with_reason("stopped");
            live.alive.store(false, Ordering::SeqCst);
            if let Err(e) = live.input.lock().unwrap().release_all() {
                *live.error.lock().unwrap() = Some(format!("input cleanup incomplete: {e}"));
            }
            live.session.close();
        }
        let _ = self.client.close_transport_session(id).await;
    }
    pub async fn stop_hosts(&self) {
        let ids: Vec<_> = self
            .sessions
            .lock()
            .await
            .iter()
            .filter(|(_, s)| s.host)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.stop(&id).await;
        }
    }
    pub async fn stop_all(&self) {
        let ids: Vec<_> = self.sessions.lock().await.keys().cloned().collect();
        for id in ids {
            self.stop(&id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct InputAuthority {
        source: iroh::EndpointId,
        target: iroh::EndpointId,
    }
    impl farsail_transport::Authority for InputAuthority {
        async fn inspect(
            &self,
            id: &str,
            _token: &str,
        ) -> farsail_transport::Result<farsail_transport::Claims> {
            let encode = |key: iroh::EndpointId| {
                key.as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            };
            Ok(farsail_transport::Claims {
                session_id: id.into(),
                permission: RemotePermission::Control,
                source_public_key: encode(self.source),
                target_public_key: encode(self.target),
                nonce: "ab".repeat(32),
                expires_in: 20,
                checked_at: std::time::Instant::now(),
            })
        }
        async fn issue(&self, _id: &str, _renew: bool) -> farsail_transport::Result<String> {
            Ok("synthetic-input-grant".into())
        }
    }
    #[tokio::test]
    async fn authenticated_input_pause_keeps_media_and_explicit_resume_available() {
        use farsail_transport::{Config, Transport};
        let viewer = Transport::bind([41; 32], Config::default()).await.unwrap();
        let host = Transport::bind([42; 32], Config::default()).await.unwrap();
        let authority = Arc::new(InputAuthority {
            source: viewer.id(),
            target: host.id(),
        });
        let accepting = tokio::spawn({
            let host = host.clone();
            let authority = authority.clone();
            async move { host.accept(authority).await.unwrap() }
        });
        let source = viewer
            .connect(
                host.addr(),
                "input-pause-test",
                RemotePermission::Control,
                authority,
            )
            .await
            .unwrap();
        let target = accepting.await.unwrap();
        let mut input = InputState::default();
        let mut received = InputState::default();
        input.update(Some(&farsail_windows::Error::Injection {
            inserted: 0,
            expected: 1,
            code: 0,
        }));
        target
            .send(Frame {
                channel: Channel::Control,
                bytes: input.packet(),
            })
            .await
            .unwrap();
        let report = source.receive().await.unwrap();
        received.accept(&report.bytes);
        assert!(received.blocked);
        assert!(source.is_open_now() && target.is_open_now());
        target
            .send(Frame {
                channel: Channel::Media,
                bytes: b"synthetic-screen-after-input-rejection".to_vec(),
            })
            .await
            .unwrap();
        assert_eq!(
            source.receive().await.unwrap().bytes,
            b"synthetic-screen-after-input-rejection"
        );
        source
            .send(Frame {
                channel: Channel::Control,
                bytes: b"FSR2".to_vec(),
            })
            .await
            .unwrap();
        assert_eq!(target.receive().await.unwrap().bytes, b"FSR2");
        input.update(None);
        target
            .send(Frame {
                channel: Channel::Control,
                bytes: input.packet(),
            })
            .await
            .unwrap();
        received.accept(&source.receive().await.unwrap().bytes);
        assert!(!received.blocked);
        source.close_with_reason("stopped");
        assert!(!source.is_open_now());
        target.close();
        viewer.close().await;
        host.close().await;
    }
    #[test]
    fn input_pause_and_explicit_resume_keep_diagnostics_bounded_and_ordered() {
        let mut host = InputState::default();
        let mut viewer = InputState::default();
        host.update(Some(&farsail_windows::Error::Injection {
            inserted: 0,
            expected: 1,
            code: 5,
        }));
        let blocked_packet = host.packet();
        assert!(blocked_packet.len() < 768);
        viewer.accept(&blocked_packet);
        assert!(viewer.blocked);
        assert!(viewer.message.as_ref().unwrap().contains("错误码 5"));
        host.update(None);
        viewer.accept(&host.packet());
        assert!(!viewer.blocked);
        viewer.accept(&blocked_packet);
        assert!(
            !viewer.blocked,
            "late pause must not overwrite explicit resume"
        );
        let generation = viewer.generation;
        viewer.accept(&[0; 769]);
        viewer.accept(b"FSB1{\"generation\":999,\"blocked\":false,\"message\":null,\"extra\":1}");
        assert_eq!(viewer.generation, generation);
    }
    #[test]
    fn reordered_input_is_applied_once_in_order_and_bounded() {
        let mut order = InputOrder::default();
        assert!(order.push(2, vec![2]).unwrap().is_empty());
        assert!(order.deadline.is_some());
        assert_eq!(order.push(1, vec![1]).unwrap(), vec![vec![1], vec![2]]);
        assert!(order.deadline.is_none());
        assert!(order.push(2, vec![2]).is_err());
        assert!(order.push(35, vec![35]).is_err());
    }
}
