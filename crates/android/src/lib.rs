//! Android host reuses device proofs, approval, authenticated QUIC and short leases.
//! Only the local MediaProjection service can arm capture. No secrets cross the UI.
mod bridge;
mod capture;
mod protocol;
mod updates;
mod viewer;

use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use farsail_media::{BASELINE_PAYLOAD_BPS, ByteBudget, FrameMeta, JpegFrame, MediaWindow};
use farsail_transport::{Channel, Frame, Session};
use protocol::InputOrder;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

struct Local {
    epoch: u64,
    capture: bool,
    control: bool,
    layout: u64,
    width: u32,
    height: u32,
    sequence: u64,
    session: Option<Session>,
    inputs: VecDeque<Value>,
}

pub struct Host {
    client: Arc<NativeClient>,
    local: Mutex<Local>,
    frames: watch::Sender<Option<Arc<Vec<u8>>>>,
    viewer: Arc<viewer::Viewer>,
    endpoint: tokio::sync::Mutex<()>,
}
impl Host {
    pub fn new(client: Arc<NativeClient>) -> Arc<Self> {
        let (frames, _) = watch::channel(None);
        Arc::new(Self {
            viewer: viewer::Viewer::new(client.clone()),
            endpoint: tokio::sync::Mutex::new(()),
            client,
            frames,
            local: Mutex::new(Local {
                epoch: 0,
                capture: false,
                control: false,
                layout: 1,
                width: 0,
                height: 0,
                sequence: 0,
                session: None,
                inputs: VecDeque::new(),
            }),
        })
    }
    pub fn begin(&self) -> u64 {
        self.stop_local();
        let mut l = self.local.lock().unwrap();
        l.capture = true;
        l.layout += 1;
        l.epoch
    }
    pub fn stop_local(&self) {
        // Invalidate before any coordinator call or UI task can run.
        let mut l = self.local.lock().unwrap();
        l.capture = false;
        l.epoch += 1;
        l.inputs.clear();
        if let Some(s) = l.session.take() {
            s.close();
        }
        self.client.disable_host_local();
        self.frames.send_replace(None);
    }
    pub fn control(&self, enabled: bool) {
        let mut l = self.local.lock().unwrap();
        l.control = enabled;
        self.client
            .set_remote_watch(enabled && l.capture && self.client.hosting_enabled())
            .ok();
        l.inputs.clear();
        if !enabled
            && l.session
                .as_ref()
                .is_some_and(|s| s.permission() == RemotePermission::Control)
        {
            l.session.take().unwrap().close();
        }
    }
    pub fn geometry(&self, width: u32, height: u32) -> u64 {
        let mut l = self.local.lock().unwrap();
        if width != l.width || height != l.height {
            l.width = width;
            l.height = height;
            l.layout += 1;
            l.inputs.clear();
            self.frames.send_replace(None);
        }
        l.layout
    }
    fn current(&self, epoch: u64) -> bool {
        let l = self.local.lock().unwrap();
        l.capture && l.epoch == epoch
    }
    pub fn publish(&self, jpeg: Vec<u8>, width: u32, height: u32, layout: u64) -> bool {
        let mut l = self.local.lock().unwrap();
        if !l.capture || l.layout != layout || l.width != width || l.height != height {
            return false;
        }
        self.publish_locked(&mut l, jpeg, width, height)
    }
    /// Screenshot callbacks may outlive stop, account changes or a new capture.
    /// Validate their epoch and update geometry atomically before publishing.
    pub fn publish_accessibility(
        &self,
        jpeg: Vec<u8>,
        width: u32,
        height: u32,
        epoch: u64,
    ) -> bool {
        let mut l = self.local.lock().unwrap();
        if !l.capture || l.epoch != epoch {
            return false;
        }
        if l.width != width || l.height != height {
            l.width = width;
            l.height = height;
            l.layout += 1;
            l.inputs.clear();
        }
        self.publish_locked(&mut l, jpeg, width, height)
    }
    fn publish_locked(&self, l: &mut Local, jpeg: Vec<u8>, width: u32, height: u32) -> bool {
        l.sequence += 1;
        let image = JpegFrame {
            meta: FrameMeta {
                monitor: 1,
                layout: l.layout,
                sequence: l.sequence,
                captured_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                width,
                height,
                origin_x: 0,
                origin_y: 0,
            },
            jpeg,
        };
        match image.to_wire() {
            Ok(wire) => {
                self.frames.send_replace(Some(Arc::new(wire)));
                true
            }
            Err(_) => false,
        }
    }
    pub fn input_valid(&self, id: &str, epoch: u64, layout: u64) -> bool {
        let l = self.local.lock().unwrap();
        l.capture
            && l.control
            && l.epoch == epoch
            && l.layout == layout
            && self.client.hosting_enabled()
            && l.session.as_ref().is_some_and(|s| {
                s.id() == id && s.permission() == RemotePermission::Control && s.is_open_now()
            })
    }
    pub fn drain(&self) -> Value {
        let mut l = self.local.lock().unwrap();
        let inputs: Vec<_> = l.inputs.drain(..).collect();
        json!(inputs)
    }
    fn enqueue(&self, session: &Session, payload: &[u8]) -> Result<(), ()> {
        let mut l = self.local.lock().unwrap();
        if !l.capture || !l.control || !self.client.hosting_enabled() || !session.is_open_now() {
            return Ok(());
        }
        let input = if matches!(payload, b"FSR1" | b"FSR2" | b"FSR3") {
            json!({"kind":"release"})
        } else {
            let v: Value = serde_json::from_slice(payload).map_err(|_| ())?;
            if !protocol::valid_input(&v, l.layout) {
                return Ok(());
            }
            v
        };
        if l.inputs.len() >= 128 {
            return Err(());
        }
        let command =
            json!({"session":session.id(), "epoch":l.epoch, "layout":l.layout, "input":input});
        l.inputs.push_back(command);
        Ok(())
    }
    pub fn listen(self: &Arc<Self>) {
        let mut incoming = self.client.subscribe_sessions();
        let owner = self.clone();
        tokio::spawn(async move {
            loop {
                match incoming.recv().await {
                    Ok((s, true)) if s.permission() != RemotePermission::Files => {
                        let accepted = {
                            let mut l = owner.local.lock().unwrap();
                            if !l.capture
                                || !owner.client.hosting_enabled()
                                || (s.permission() == RemotePermission::Control && !l.control)
                                || l.session.as_ref().is_some_and(Session::is_open_now)
                            {
                                false
                            } else {
                                l.inputs.clear();
                                l.session = Some(s.clone());
                                true
                            }
                        };
                        if accepted {
                            let host = owner.clone();
                            tokio::spawn(async move {
                                host.run(s).await;
                            });
                        } else {
                            s.close();
                        }
                    }
                    // Outgoing sessions belong to the independent mobile viewer.
                    Ok((_, false)) => (),
                    Ok((s, true)) => s.close(),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        owner.stop_local();
                        owner.viewer.cancel();
                    }
                    Err(_) => break,
                }
            }
        });
    }
    async fn run(self: Arc<Self>, session: Session) {
        let (ack_tx, ack_rx) = watch::channel(0u64);
        let host = self.clone();
        let sent = session.clone();
        let sender = tokio::spawn(async move {
            host.send_frames(sent, ack_rx).await;
        });
        let mut order = InputOrder::default();
        let mut count = 0;
        let mut interval = Instant::now();
        while self.client.hosting_enabled() && session.is_open_now() {
            let frame =
                match tokio::time::timeout(Duration::from_millis(250), session.receive()).await {
                    Ok(Ok(f)) => f,
                    Ok(Err(farsail_transport::Error::Timeout)) | Err(_) => {
                        if order.expired() {
                            session.close_with_reason("input_sequence");
                            break;
                        }
                        continue;
                    }
                    _ => break,
                };
            if order.expired() {
                session.close_with_reason("input_sequence");
                break;
            }
            if frame.channel == Channel::Media
                && frame.bytes.len() == 12
                && frame.bytes.starts_with(b"FSA1")
            {
                let seq = u64::from_be_bytes(frame.bytes[4..].try_into().unwrap());
                ack_tx.send_if_modified(|a| {
                    if seq > *a {
                        *a = seq;
                        true
                    } else {
                        false
                    }
                });
            } else if frame.channel == Channel::Control
                && session.permission() == RemotePermission::Control
            {
                if frame.bytes.len() <= 12
                    || frame.bytes.len() > 1024
                    || !frame.bytes.starts_with(b"FSI1")
                {
                    continue;
                }
                let seq = u64::from_be_bytes(frame.bytes[4..12].try_into().unwrap());
                let Ok(ready) = order.push(seq, frame.bytes[12..].to_vec()) else {
                    session.close_with_reason("input_sequence");
                    break;
                };
                for payload in ready {
                    if interval.elapsed() >= Duration::from_secs(1) {
                        count = 0;
                        interval = Instant::now();
                    }
                    count += 1;
                    if count > 240 || self.enqueue(&session, &payload).is_err() {
                        session.close_with_reason("input_failed");
                        break;
                    }
                }
            }
        }
        sender.abort();
        session.close();
        let mut l = self.local.lock().unwrap();
        if l.session
            .as_ref()
            .is_some_and(|s| s.stable_id() == session.stable_id())
        {
            l.session = None;
            l.inputs.clear();
        }
    }
    async fn send_frames(&self, session: Session, mut ack: watch::Receiver<u64>) {
        if session
            .send(Frame {
                channel: Channel::Media,
                bytes: b"FSH1android".to_vec(),
            })
            .await
            .is_err()
        {
            session.close();
            return;
        }
        let mut frames = self.frames.subscribe();
        let mut window = MediaWindow::default();
        let mut budget = ByteBudget::new(BASELINE_PAYLOAD_BPS);
        let mut last = 0;
        let mut layout = 0;
        let mut progress = Instant::now();
        loop {
            if !self.client.hosting_enabled() || !session.is_open_now() {
                break;
            }
            match window.acknowledge(*ack.borrow_and_update()) {
                Ok(true) => progress = Instant::now(),
                Err(_) => {
                    session.close_with_reason("media_timeout");
                    break;
                }
                _ => (),
            }
            if !window.is_empty() && progress.elapsed() > Duration::from_secs(5) {
                session.close_with_reason("media_timeout");
                break;
            }
            if window.blocked_on().is_none() {
                let next = frames.borrow_and_update().clone();
                if let Some(wire) = next {
                    let Ok(image) = JpegFrame::from_wire(&wire) else {
                        break;
                    };
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if image.meta.sequence > last
                        && now.saturating_sub(image.meta.captured_ms) <= 700
                    {
                        let wait = budget.charge(wire.len());
                        if !wait.is_zero() {
                            tokio::time::sleep(wait.min(Duration::from_millis(100))).await;
                            continue;
                        }
                        if image.meta.layout != layout {
                            layout = image.meta.layout;
                            let list = json!([{"id":1,"name":"Android 屏幕","x":0,"y":0,"width":image.meta.width,"height":image.meta.height,"dpi":160,"rotation":0}]);
                            let mut bytes = b"FSL1".to_vec();
                            bytes.extend(serde_json::to_vec(&list).unwrap());
                            if session
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
                        let was_empty = window.is_empty();
                        if window.sent(image.meta.sequence).is_err() {
                            break;
                        }
                        last = image.meta.sequence;
                        if was_empty {
                            progress = Instant::now();
                        }
                        if session
                            .send(Frame {
                                channel: Channel::Media,
                                bytes: (*wire).clone(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            tokio::select! {
                _ = frames.changed() => (),
                _ = ack.changed() => (),
                _ = tokio::time::sleep(Duration::from_millis(50)) => (),
                _ = session.wait_closed() => break,
            }
        }
        session.close();
    }
    async fn ensure_transport(&self) -> Result<(), String> {
        let _guard = self.endpoint.lock().await;
        self.client
            .call("heartbeat", json!({}))
            .await
            .map_err(|e| e.to_string())?;
        if self.client.transport_running().await {
            return Ok(());
        }
        let state = self.client.public_state().await;
        let relay = state["defaultRelay"]
            .as_str()
            .map(str::parse)
            .transpose()
            .map_err(|_| "invalid relay")?;
        self.client
            .start_transport(farsail_transport::Config {
                bind: "0.0.0.0:0".parse().unwrap(),
                relay,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn call(self: &Arc<Self>, op: &str, args: Value) -> Result<Value, String> {
        let client = &self.client;
        if matches!(op, "logout" | "server" | "login" | "bind") {
            self.viewer.cancel();
        }
        if op.starts_with("viewer_") {
            let ticket = args["ticket"].as_u64().ok_or("连接编号无效")?;
            return match op {
                "viewer_state" => Ok(self.viewer.status(ticket)),
                "viewer_connect" => {
                    if !self.viewer.current(ticket) {
                        return Err("连接已取消".into());
                    }
                    self.ensure_transport().await?;
                    let permission = match args["permission"].as_str() {
                        Some("control") => RemotePermission::Control,
                        Some("view") => RemotePermission::View,
                        _ => return Err("连接权限无效".into()),
                    };
                    self.viewer
                        .connect(
                            ticket,
                            args["target"].as_str().ok_or("目标设备无效")?,
                            permission,
                        )
                        .await
                }
                "viewer_input" => {
                    self.viewer
                        .input(
                            ticket,
                            args["input"].clone(),
                            args["generation"].as_u64().unwrap_or(0),
                        )
                        .await
                }
                "viewer_select" => {
                    self.viewer
                        .select(
                            ticket,
                            args["display"]
                                .as_u64()
                                .and_then(|n| u32::try_from(n).ok())
                                .ok_or("屏幕编号无效")?,
                        )
                        .await
                }
                _ => Err("unsupported viewer operation".into()),
            };
        }
        let result = match op {
            "state" => {
                let mut state = client.public_state().await;
                let l = self.local.lock().unwrap();
                state["capture"] = json!(l.capture);
                state["control"] = json!(l.control);
                state["session"] = l.session.as_ref().filter(|s| s.is_open_now()).map(|s| {
                    json!({"id":s.id(),"permission":s.permission(),"code":s.verification_code().ok(),"path":s.path().0})
                }).unwrap_or(Value::Null);
                return Ok(state);
            }
            "server" => {
                client
                    .set_server(args["url"].as_str().ok_or("server required")?)
                    .await
            }
            "presence" => {
                self.ensure_transport().await?;
                client
                    .refresh_transport_address()
                    .await
                    .map_err(|e| e.to_string())?;
                client
                    .clear_stale_host_capability()
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(client.public_state().await);
            }
            "share" => {
                let epoch = args["epoch"].as_u64().ok_or("capture consent required")?;
                if !self.current(epoch) {
                    return Err("屏幕共享已取消".into());
                }
                self.ensure_transport().await?;
                if !self.current(epoch) {
                    return Err("屏幕共享已取消".into());
                }
                let result = client.set_host_capability(true).await;
                if !self.current(epoch) {
                    client.disable_host_local();
                    let _ = client.set_host_capability(false).await;
                    return Err("屏幕共享已取消".into());
                }
                if result.is_ok() {
                    let local = self.local.lock().unwrap();
                    client
                        .set_remote_watch(local.control && local.capture && local.epoch == epoch)
                        .ok();
                }
                result
            }
            "pending" => {
                {
                    let local = self.local.lock().unwrap();
                    client
                        .set_remote_watch(
                            local.control && local.capture && client.hosting_enabled(),
                        )
                        .ok();
                }
                client
                    .approve_same_account_pending()
                    .await
                    .map_err(|e| e.to_string())?;
                client.call("pending", args).await
            }
            "stop" => {
                self.stop_local();
                let result = client.set_host_capability(false).await;
                let _ = client.revoke_host_approvals().await;
                result
            }
            "decide" => {
                if args["approve"] == true {
                    if !client.hosting_enabled() {
                        return Err("请先开启屏幕共享".into());
                    }
                    let pending = client
                        .call("pending", json!({}))
                        .await
                        .map_err(|e| e.to_string())?;
                    let req = pending
                        .as_array()
                        .and_then(|a| a.iter().find(|r| r["id"] == args["id"]))
                        .ok_or("请求已过期")?;
                    if req["permission"] == "control" && !self.local.lock().unwrap().control {
                        return Err("请先开启辅助功能控制，或让电脑申请仅查看".into());
                    }
                }
                client.call(op, args).await
            }
            "login" | "logout" | "resume" | "bind" | "heartbeat" | "register" | "verify"
            | "resend" | "devices" => client.call(op, args).await,
            _ => return Err("unsupported operation".into()),
        };
        result.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod viewer_tests;
