use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use farsail_media::{BASELINE_PAYLOAD_BPS, ByteBudget, JpegFrame, wait_for_ack};
use farsail_transport::{Channel, Frame, Session};
use farsail_windows::{Capture, Display, Input, InputSink, displays};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Semaphore, watch};

struct Live {
    session: Session,
    host: bool,
    alive: Arc<AtomicBool>,
    selected: Arc<AtomicU32>,
    layout: Arc<AtomicU64>,
    current: Arc<StdMutex<Option<(Display, u64)>>>,
    input: Arc<StdMutex<InputSink>>,
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
            live.session.close();
            let reason = live
                .error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "session ended".into());
            let mut finished = owner.finished.lock().await;
            if finished.len() >= 32
                && let Some(old) = finished.keys().next().cloned()
            {
                finished.remove(&old);
            }
            finished.insert(live.session.id().into(),json!({"id":live.session.id(),"state":"closed","rtt_ms":null,"permission":live.session.permission(),"sharing":live.host,"verification_code":live.session.verification_code().ok(),"displays":*live.displays.lock().unwrap(),"error":reason}));
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
            'sending: while rx.changed().await.is_ok() {
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
                    if age > 700 {
                        continue 'sending;
                    }
                    let wait = budget.charge(bytes.len());
                    if !wait.is_zero() {
                        tokio::time::sleep(wait.min(Duration::from_millis(200))).await;
                        continue;
                    }
                    break (bytes, frame.meta.sequence);
                };
                if sender_live
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
                if !wait_for_ack(&mut ack_rx, sequence, Duration::from_secs(1)).await {
                    *sender_live.error.lock().unwrap() =
                        Some("viewer did not consume frame before deadline".into());
                    break;
                }
            }
            sender_live.alive.store(false, Ordering::SeqCst);
            sender_live.session.close();
        });
        let mut last_input = 0u64;
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
                let incoming = live.session.receive().await;
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
                    if seq != last_input.wrapping_add(1) {
                        *live.error.lock().unwrap() =
                            Some("input sequence gap; session stopped safely".into());
                        break;
                    }
                    last_input = seq;
                    if input_window.elapsed() >= Duration::from_secs(1) {
                        input_window = std::time::Instant::now();
                        input_count = 0;
                        move_count = 0;
                    }
                    input_count += 1;
                    if input_count > 240 {
                        *live.error.lock().unwrap() =
                            Some("input rate exceeded; session stopped safely".into());
                        break;
                    }
                    let mut sink = live.input.lock().unwrap();
                    if !live.alive.load(Ordering::SeqCst)
                        || !self.client.hosting_enabled()
                        || !live.session.is_open_now()
                    {
                        break;
                    }
                    if &frame.bytes[12..] == b"FSR1" {
                        if let Err(e) = sink.release_all() {
                            *live.error.lock().unwrap() =
                                Some(format!("input cleanup incomplete: {e}"));
                        }
                    } else if let Ok(input) = serde_json::from_slice::<Input>(&frame.bytes[12..]) {
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
                            *live.error.lock().unwrap() = Some(e.to_string());
                        }
                    }
                }
            }
        }
        sender.abort();
        if capture_failed.load(Ordering::SeqCst) {
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
            if frame.channel != Channel::Media {
                continue;
            }
            if frame.bytes.starts_with(b"FSL1") {
                if let Ok(list) = serde_json::from_slice::<Vec<Display>>(&frame.bytes[4..]) {
                    *live.displays.lock().unwrap() = list;
                }
            } else if let Ok(image) = JpegFrame::from_wire(&frame.bytes)
                && image.meta.sequence > last_sequence
            {
                last_sequence = image.meta.sequence;
                sender.send_replace(Some(frame.bytes));
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
            return self
                .finished
                .lock()
                .await
                .get(id)
                .cloned()
                .ok_or("remote session not active".into());
        };
        let (path, rtt) = live.session.path();
        Ok(
            json!({"id":id,"state":path,"rtt_ms":rtt,"permission":live.session.permission(),"sharing":live.host,"verification_code":live.session.verification_code().map_err(|e|e.to_string())?,"displays":*live.displays.lock().unwrap(),"error":*live.error.lock().unwrap()}),
        )
    }
    pub async fn next_frame(&self, id: &str, after: u64) -> Result<Vec<u8>, String> {
        let mut receiver = self
            .sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or("remote session not active")?
            .frame
            .clone();
        for _ in 0..2 {
            if let Some(bytes) = receiver.borrow_and_update().as_ref()
                && let Ok(frame) = JpegFrame::from_wire(bytes)
                && frame.meta.sequence > after
            {
                return Ok(bytes.clone());
            }
            if tokio::time::timeout(Duration::from_millis(450), receiver.changed())
                .await
                .is_err()
            {
                break;
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
        let payload = if let Some(value) = value {
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
