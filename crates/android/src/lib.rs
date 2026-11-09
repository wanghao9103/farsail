//! Android host reuses device proofs, approval, authenticated QUIC and short leases.
//! Only a locally authorized capture service can arm capture. No secrets cross the UI.
mod bridge;
mod capture;
mod protocol;
mod updates;
mod viewer;

use farsail_client::NativeClient;
use farsail_core::RemotePermission;
use farsail_media::{
    BASELINE_PAYLOAD_BPS, ByteBudget, FrameMeta, JpegFrame, MediaWindow, StreamDemand,
    VideoProfile, ViewportRequest,
};
use farsail_transport::{Channel, Frame, Session};
use protocol::InputOrder;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

#[derive(Default)]
struct MediaDiagnostics {
    bytes: usize,
    sent: u64,
    acknowledged: u64,
    send_ms: u64,
    publish_to_send_ms: u64,
    ack_ms: u64,
    input_wait_ms: u64,
    encode_ms: u64,
    encoded_width: u32,
    encoded_height: u32,
    pending: VecDeque<(u64, Instant)>,
}

struct Local {
    epoch: u64,
    capture: bool,
    control: bool,
    layout: u64,
    width: u32,
    height: u32,
    sequence: u64,
    session: Option<Session>,
    inputs: VecDeque<(Instant, Value)>,
    media: MediaDiagnostics,
    video_demand: StreamDemand,
    video_profile: VideoProfile,
    viewport_edge: Option<u16>,
    delivery_quality: capture::DeliveryQuality,
}

pub struct Host {
    client: Arc<NativeClient>,
    local: Mutex<Local>,
    frames: watch::Sender<Option<Arc<Vec<u8>>>>,
    viewer: Arc<viewer::Viewer>,
    endpoint: tokio::sync::Mutex<()>,
    screenshot_encoder: Mutex<capture::ScreenshotEncoder>,
}
impl Host {
    fn settings(local: &Local) -> capture::Settings {
        capture::Settings {
            profile: local.video_profile,
            edge: local.viewport_edge,
            quality: if local.viewport_edge.is_some() {
                local.delivery_quality.value
            } else {
                90
            },
        }
    }
    fn profile_packet(local: &Local) -> Vec<u8> {
        local
            .viewport_edge
            .map(|edge| {
                ViewportRequest {
                    generation: local.video_profile.generation,
                    edge,
                }
                .packet(b"FSZ2")
            })
            .unwrap_or_else(|| local.video_profile.packet(b"FSP1"))
    }
    fn native_profile() -> VideoProfile {
        VideoProfile {
            profile: 2,
            generation: 0,
        }
    }
    pub fn new(client: Arc<NativeClient>) -> Arc<Self> {
        let (frames, _) = watch::channel(None);
        Arc::new(Self {
            viewer: viewer::Viewer::new(client.clone()),
            endpoint: tokio::sync::Mutex::new(()),
            screenshot_encoder: Mutex::new(capture::ScreenshotEncoder::default()),
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
                media: MediaDiagnostics::default(),
                video_demand: StreamDemand::default(),
                video_profile: Self::native_profile(),
                viewport_edge: None,
                delivery_quality: capture::DeliveryQuality::default(),
            }),
        })
    }
    pub fn begin(&self) -> u64 {
        self.stop_local();
        let mut l = self.local.lock().unwrap();
        l.capture = true;
        l.media = MediaDiagnostics::default();
        l.video_demand = StreamDemand::default();
        l.video_profile = Self::native_profile();
        l.viewport_edge = None;
        l.delivery_quality = capture::DeliveryQuality::default();
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
        drop(l);
        self.screenshot_encoder.lock().unwrap().clear();
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
    fn encode_screenshot(
        &self,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        epoch: u64,
    ) -> Option<(JpegFrame, capture::Settings)> {
        let profile = {
            let local = self.local.lock().unwrap();
            if !local.capture || local.epoch != epoch {
                return None;
            }
            Self::settings(&local)
        };
        let mut encoder = self.screenshot_encoder.lock().unwrap();
        if !self.current(epoch) {
            return None;
        }
        encoder
            .encode_profile(bytes, width, height, stride, profile)
            .map(|image| (image, profile))
    }
    fn publish_rgba(
        &self,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        layout: u64,
    ) -> bool {
        let profile = {
            let local = self.local.lock().unwrap();
            if !local.capture
                || (local.width, local.height, local.layout) != (width, height, layout)
            {
                return false;
            }
            if !local.video_demand.active {
                return true;
            }
            Self::settings(&local)
        };
        let started = Instant::now();
        let Some(image) =
            capture::encode_rgba_profile(bytes, width, height, stride, layout, profile)
        else {
            return false;
        };
        let mut local = self.local.lock().unwrap();
        if !local.capture
            || (local.width, local.height, local.layout) != (width, height, layout)
            || Self::settings(&local) != profile
        {
            return false;
        }
        local.media.encode_ms = started.elapsed().as_millis() as u64;
        self.publish_locked(&mut local, image.jpeg, image.meta.width, image.meta.height)
    }
    fn publish_accessibility_encoded(
        &self,
        image: JpegFrame,
        width: u32,
        height: u32,
        epoch: u64,
        profile: capture::Settings,
    ) -> bool {
        let mut local = self.local.lock().unwrap();
        if !local.capture || local.epoch != epoch || Self::settings(&local) != profile {
            return false;
        }
        if (local.width, local.height) != (width, height) {
            local.width = width;
            local.height = height;
            local.layout += 1;
            local.inputs.clear();
        }
        self.publish_locked(&mut local, image.jpeg, image.meta.width, image.meta.height)
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
                l.media.bytes = wire.len();
                l.media.encoded_width = width;
                l.media.encoded_height = height;
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
        if let Some((queued, _)) = inputs.first() {
            l.media.input_wait_ms = queued.elapsed().as_millis() as u64;
        }
        json!(
            inputs
                .into_iter()
                .map(|(_, command)| command)
                .collect::<Vec<_>>()
        )
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
        l.inputs.push_back((Instant::now(), command));
        Ok(())
    }
    fn accept_session(&self, s: &Session) -> bool {
        let mut l = self.local.lock().unwrap();
        if !l.capture
            || !self.client.hosting_enabled()
            || (s.permission() == RemotePermission::Control && !l.control)
            || l.session.as_ref().is_some_and(Session::is_open_now)
        {
            false
        } else {
            l.inputs.clear();
            l.media = MediaDiagnostics::default();
            l.session = Some(s.clone());
            // Desktop input needs a displayed frame. Wake on an
            // approved control connection so a sleeping projection
            // can produce that first frame without a mouse click.
            if s.permission() == RemotePermission::Control {
                let command = json!({"session":s.id(), "epoch":l.epoch, "layout":l.layout, "input":{"kind":"wake"}});
                l.inputs.push_back((Instant::now(), command));
            }
            true
        }
    }
    pub fn listen(self: &Arc<Self>) {
        let mut incoming = self.client.subscribe_sessions();
        let owner = self.clone();
        tokio::spawn(async move {
            loop {
                match incoming.recv().await {
                    Ok((s, true)) if s.permission() != RemotePermission::Files => {
                        let accepted = owner.accept_session(&s);
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
        let (demand_tx, demand_rx) = watch::channel(StreamDemand::default());
        {
            let mut local = self.local.lock().unwrap();
            if local
                .session
                .as_ref()
                .is_some_and(|s| s.stable_id() == session.stable_id())
            {
                local.video_demand = StreamDemand::default();
                if local.video_profile != Self::native_profile() {
                    local.video_profile = Self::native_profile();
                    local.viewport_edge = None;
                    local.delivery_quality = capture::DeliveryQuality::default();
                    self.frames.send_replace(None);
                }
            }
        }
        let host = self.clone();
        let sent = session.clone();
        let sender = tokio::spawn(async move {
            host.send_frames(sent, ack_rx, demand_rx).await;
        });
        let mut order = InputOrder::default();
        let mut count = 0;
        let mut interval = Instant::now();
        while self.client.hosting_enabled() && session.is_open_now() {
            // Keep the same receive future across housekeeping ticks. Cancelling
            // it after accept_uni would drop a partially received ACK/input stream.
            let receive = session.receive();
            tokio::pin!(receive);
            let result = loop {
                match tokio::time::timeout(Duration::from_millis(250), &mut receive).await {
                    Ok(result) => break Some(result),
                    Err(_) => {
                        if order.expired() {
                            session.close_with_reason("input_sequence");
                            break None;
                        }
                        if !self.client.hosting_enabled() || !session.is_open_now() {
                            break None;
                        }
                    }
                }
            };
            let frame = match result {
                Some(Ok(frame)) => frame,
                Some(Err(farsail_transport::Error::Timeout)) => continue,
                _ => break,
            };
            if order.expired() {
                session.close_with_reason("input_sequence");
                break;
            }
            if frame.channel == Channel::Media
                && let Some(next) = StreamDemand::parse(&frame.bytes)
            {
                if demand_tx.send_if_modified(|current| current.advance(next)) {
                    let mut local = self.local.lock().unwrap();
                    if local
                        .session
                        .as_ref()
                        .is_some_and(|s| s.stable_id() == session.stable_id())
                    {
                        local.video_demand = next;
                    }
                }
            } else if frame.channel == Channel::Media
                && let Some(request) = ViewportRequest::parse(&frame.bytes, b"FSZ1")
            {
                let bytes = {
                    let mut local = self.local.lock().unwrap();
                    if local
                        .session
                        .as_ref()
                        .is_none_or(|s| s.stable_id() != session.stable_id())
                    {
                        break;
                    }
                    if request.generation > local.video_profile.generation {
                        local.video_profile = VideoProfile {
                            profile: 2,
                            generation: request.generation,
                        };
                        local.viewport_edge = Some(request.edge);
                        local.delivery_quality =
                            capture::DeliveryQuality::with_rtt(session.path().1);
                        self.frames.send_replace(None);
                    }
                    Self::profile_packet(&local)
                };
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
            } else if frame.channel == Channel::Media
                && let Some(request) = VideoProfile::parse(&frame.bytes, b"FSQ1")
                && request.profile <= 2
            {
                let applied = {
                    let mut local = self.local.lock().unwrap();
                    if local
                        .session
                        .as_ref()
                        .is_none_or(|s| s.stable_id() != session.stable_id())
                    {
                        break;
                    }
                    if request.generation > local.video_profile.generation {
                        local.video_profile = request;
                        local.viewport_edge = None;
                        local.delivery_quality = capture::DeliveryQuality::default();
                        self.frames.send_replace(None);
                    }
                    Self::profile_packet(&local)
                };
                if session
                    .send(Frame {
                        channel: Channel::Media,
                        bytes: applied,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            } else if frame.channel == Channel::Media
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
    async fn send_frames(
        &self,
        session: Session,
        mut ack: watch::Receiver<u64>,
        mut demand: watch::Receiver<StreamDemand>,
    ) {
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
        if session
            .send(Frame {
                channel: Channel::Media,
                bytes: b"FSW0".to_vec(),
            })
            .await
            .is_err()
        {
            return;
        }
        for bytes in [
            b"FSV1\x02".to_vec(),
            b"FSZ0".to_vec(),
            Self::native_profile().packet(b"FSP1"),
        ] {
            if session
                .send(Frame {
                    channel: Channel::Media,
                    bytes,
                })
                .await
                .is_err()
            {
                return;
            }
        }
        let mut frames = self.frames.subscribe();
        let mut window = MediaWindow::default();
        let mut budget = ByteBudget::new(BASELINE_PAYLOAD_BPS);
        let mut last = 0;
        let mut layout = 0;
        let mut progress = Instant::now();
        let mut last_attempt = Instant::now();
        let mut send_failures = 0u8;
        loop {
            if !self.client.hosting_enabled() || !session.is_open_now() {
                break;
            }
            let sequence = *ack.borrow_and_update();
            match window.acknowledge(sequence) {
                Ok(true) => {
                    progress = Instant::now();
                    let mut l = self.local.lock().unwrap();
                    if l.session
                        .as_ref()
                        .is_some_and(|s| s.stable_id() == session.stable_id())
                    {
                        while l
                            .media
                            .pending
                            .front()
                            .is_some_and(|(seq, _)| *seq <= sequence)
                        {
                            let (_, at) = l.media.pending.pop_front().unwrap();
                            l.media.ack_ms = at.elapsed().as_millis() as u64;
                        }
                        l.media.acknowledged = sequence;
                        if l.viewport_edge.is_some() {
                            let (ack_ms, send_ms) = (l.media.ack_ms, l.media.send_ms);
                            l.delivery_quality.observe(
                                ack_ms,
                                send_ms,
                                session.path().1.unwrap_or(100),
                                Instant::now(),
                            );
                        }
                    }
                }
                Err(_) => {
                    session.close_with_reason("media_timeout");
                    break;
                }
                _ => (),
            }
            if !window.is_empty() && progress.elapsed() > Duration::from_secs(15) {
                session.close_with_reason("media_timeout");
                break;
            }
            let video_active = demand.borrow().active;
            // Receiver read timeouts discard a JPEG without producing FSA1.
            // If both slots were dropped, send one fresh frame on this same
            // authorized connection. Never restart the 15-second ACK deadline.
            if video_active
                && last_attempt.elapsed() >= Duration::from_secs(4)
                && let Some(sequence) = window.retry_stalled()
            {
                last_attempt = Instant::now();
                let mut local = self.local.lock().unwrap();
                if local
                    .session
                    .as_ref()
                    .is_some_and(|s| s.stable_id() == session.stable_id())
                {
                    local.media.pending.retain(|(seq, _)| *seq != sequence);
                }
            }
            let auto_mode = self.local.lock().unwrap().viewport_edge.is_some();
            // A small-window stream needs low latency, not two JPEGs queued
            // ahead of the latest capture. A bounded probe after four seconds
            // still recovers a lost single ACK without restarting the lease.
            let auto_waiting =
                auto_mode && !window.is_empty() && last_attempt.elapsed() < Duration::from_secs(4);
            if video_active && !auto_waiting && window.blocked_on().is_none() {
                let next = frames.borrow_and_update().clone();
                if let Some(wire) = next {
                    let Ok(image) = JpegFrame::from_wire(&wire) else {
                        break;
                    };
                    let source = {
                        let local = self.local.lock().unwrap();
                        if !local.capture
                            || local.layout != image.meta.layout
                            || Self::settings(&local).output_size(local.width, local.height)
                                != Some((image.meta.width, image.meta.height))
                            || local
                                .session
                                .as_ref()
                                .is_none_or(|s| s.stable_id() != session.stable_id())
                        {
                            None
                        } else {
                            Some((local.width, local.height))
                        }
                    };
                    let Some((source_width, source_height)) = source else {
                        // Re-reading this same rejected watch value without an
                        // await can monopolize a runtime worker indefinitely.
                        // Wait for fresh work, while still servicing cancellation.
                        tokio::select! {
                            _ = frames.changed() => (),
                            _ = ack.changed() => (),
                            _ = demand.changed() => (),
                            _ = tokio::time::sleep(Duration::from_millis(50)) => (),
                            _ = session.wait_closed() => break,
                        }
                        continue;
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
                            let list = json!([{"id":1,"name":"Android 屏幕","x":0,"y":0,"width":source_width,"height":source_height,"dpi":160,"rotation":0}]);
                            let mut bytes = b"FSL1".to_vec();
                            bytes.extend(serde_json::to_vec(&list).unwrap());
                            let sent = session
                                .send(Frame {
                                    channel: Channel::Media,
                                    bytes,
                                })
                                .await;
                            match sent {
                                Ok(()) => {
                                    layout = image.meta.layout;
                                }
                                Err(farsail_transport::Error::Timeout) => {
                                    send_failures += 1;
                                    if send_failures >= 3 {
                                        session.close_with_reason("media_timeout");
                                        break;
                                    }
                                    continue;
                                }
                                Err(_) => break,
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
                        let sent_at = Instant::now();
                        last_attempt = sent_at;
                        let sent = session
                            .send(Frame {
                                channel: Channel::Media,
                                bytes: (*wire).clone(),
                            })
                            .await;
                        match sent {
                            Ok(()) => send_failures = 0,
                            Err(farsail_transport::Error::Timeout) => {
                                window.failed(image.meta.sequence);
                                send_failures += 1;
                                if send_failures >= 3 {
                                    session.close_with_reason("media_timeout");
                                    break;
                                }
                                continue;
                            }
                            Err(_) => break,
                        }
                        let mut l = self.local.lock().unwrap();
                        if l.session
                            .as_ref()
                            .is_some_and(|s| s.stable_id() == session.stable_id())
                        {
                            l.media.sent = image.meta.sequence;
                            l.media.send_ms = sent_at.elapsed().as_millis() as u64;
                            l.media.publish_to_send_ms = now.saturating_sub(image.meta.captured_ms);
                            l.media.pending.push_back((image.meta.sequence, sent_at));
                        }
                    }
                }
            }
            tokio::select! {
                _ = frames.changed() => (),
                _ = ack.changed() => (),
                _ = demand.changed() => (),
                _ = tokio::time::sleep(Duration::from_millis(50)) => (),
                _ = session.wait_closed() => break,
            }
        }
        session.close();
    }
    #[cfg(test)]
    fn video_requested(&self) -> bool {
        let local = self.local.lock().unwrap();
        local.capture && local.video_demand.active
    }
    fn rgba_demand(&self, width: u32, height: u32, layout: u64) -> Option<bool> {
        let local = self.local.lock().unwrap();
        (local.capture && local.width == width && local.height == height && local.layout == layout)
            .then_some(local.video_demand.active)
    }
    fn screenshot_demand(&self, epoch: u64) -> Option<bool> {
        let local = self.local.lock().unwrap();
        (local.capture && local.epoch == epoch).then_some(local.video_demand.active)
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
            "diagnostics" => {
                let l = self.local.lock().unwrap();
                let session = l.session.as_ref().filter(|s| s.is_open_now());
                let (path, rtt) = session.map(|s| s.path()).unwrap_or(("none", None));
                return Ok(json!({"path":path,"rttMs":rtt,"frameBytes":l.media.bytes,
                    "published":l.sequence,"sent":l.media.sent,"acknowledged":l.media.acknowledged,
                    "sendMs":l.media.send_ms,"publishToSendMs":l.media.publish_to_send_ms,
                    "ackMs":l.media.ack_ms,"inputWaitMs":l.media.input_wait_ms,
                    "encodeMs":l.media.encode_ms,"encodedWidth":l.media.encoded_width,"encodedHeight":l.media.encoded_height,
                    "profile":l.video_profile.profile,"profileGeneration":l.video_profile.generation,
                    "viewportEdge":l.viewport_edge,
                    "jpegQuality":Self::settings(&l).quality,
                    "pendingFrames":l.media.pending.len(),
                    "oldestPendingMs":l.media.pending.front().map(|(_, at)| at.elapsed().as_millis() as u64)}));
            }
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
