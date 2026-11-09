//! Bounded JPEG baseline for remote desktop frames. Never call this H.265.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Cursor;
use std::time::{Duration, Instant};

pub const MAX_PIXELS: u32 = 8_294_400;
pub const MAX_BYTES: usize = 4_000_000;
pub const JPEG_ENCODER: &str = if cfg!(target_os = "android") {
    "libjpeg-turbo"
} else {
    "jpeg-encoder"
};
/// JPEG payload budget, excluding QUIC/TLS/IP headers and retransmissions.
pub const BASELINE_PAYLOAD_BPS: u64 = 6_000_000;
const HEADER: usize = 49;

/// Resolution presets shared by hosts and viewers. Capture geometry and input
/// coordinates remain physical; this changes only the encoded image size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoProfile {
    pub profile: u8,
    pub generation: u64,
}
impl Default for VideoProfile {
    fn default() -> Self {
        Self {
            profile: 1,
            generation: 0,
        }
    }
}
impl VideoProfile {
    pub fn packet(self, tag: &[u8; 4]) -> Vec<u8> {
        let mut bytes = tag.to_vec();
        bytes.extend(self.generation.to_be_bytes());
        bytes.push(self.profile);
        bytes
    }
    pub fn parse(bytes: &[u8], tag: &[u8; 4]) -> Option<Self> {
        if bytes.len() != 13 || &bytes[..4] != tag || bytes[12] > 3 {
            return None;
        }
        Some(Self {
            generation: u64::from_be_bytes(bytes[4..12].try_into().ok()?),
            profile: bytes[12],
        })
    }
    pub fn output_size(self, width: u32, height: u32) -> Option<(u32, u32)> {
        geometry(width, height).ok()?;
        let (long, short) = match self.profile {
            0 => (1280u64, 720u64),
            1 => (1920, 1080),
            2 => (2560, 1440),
            3 => (3840, 2160),
            _ => return None,
        };
        let (max_w, max_h) = if width >= height {
            (long, short)
        } else {
            (short, long)
        };
        let (w, h) = (u64::from(width), u64::from(height));
        if w <= max_w && h <= max_h {
            return Some((width, height));
        }
        if w * max_h >= h * max_w {
            Some((max_w as u32, (h * max_w / w).max(1) as u32))
        } else {
            Some(((w * max_h / h).max(1) as u32, max_h as u32))
        }
    }
}

/// Optional viewport-size negotiation. Peers advertise FSZ0 before accepting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportRequest {
    pub generation: u64,
    pub edge: u16,
}
impl ViewportRequest {
    pub fn valid_edge(edge: u16) -> bool {
        (320..=2560).contains(&edge)
    }
    pub fn packet(self, tag: &[u8; 4]) -> Vec<u8> {
        let mut bytes = tag.to_vec();
        bytes.extend(self.generation.to_be_bytes());
        bytes.extend(self.edge.to_be_bytes());
        bytes
    }
    pub fn parse(bytes: &[u8], tag: &[u8; 4]) -> Option<Self> {
        if bytes.len() != 14 || &bytes[..4] != tag {
            return None;
        }
        let edge = u16::from_be_bytes(bytes[12..14].try_into().ok()?);
        Self::valid_edge(edge).then_some(Self {
            generation: u64::from_be_bytes(bytes[4..12].try_into().ok()?),
            edge,
        })
    }
}

/// Optional per-session subscription. Peers advertise FSW0 before accepting it.
/// Pausing video does not pause authorization, input, files, or transport ACKs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamDemand {
    pub generation: u64,
    pub active: bool,
}
impl Default for StreamDemand {
    fn default() -> Self {
        Self {
            generation: 0,
            active: true,
        }
    }
}
impl StreamDemand {
    pub fn packet(self) -> Vec<u8> {
        let mut bytes = b"FSW1".to_vec();
        bytes.extend_from_slice(&self.generation.to_be_bytes());
        bytes.push(u8::from(self.active));
        bytes
    }
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 13 || !bytes.starts_with(b"FSW1") || bytes[12] > 1 {
            return None;
        }
        Some(Self {
            generation: u64::from_be_bytes(bytes[4..12].try_into().ok()?),
            active: bytes[12] == 1,
        })
    }
    pub fn advance(&mut self, next: Self) -> bool {
        if next.generation <= self.generation {
            return false;
        }
        *self = next;
        true
    }
}

/// Two frames can overlap a network round trip without an unbounded JPEG queue.
/// ACKs are cumulative: an older frame becomes obsolete when a newer one arrives.
#[derive(Default)]
pub struct MediaWindow {
    pending: VecDeque<u64>,
    last_sent: u64,
}

impl MediaWindow {
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn blocked_on(&self) -> Option<u64> {
        (self.pending.len() >= 2).then(|| self.pending[0])
    }

    pub fn sent(&mut self, sequence: u64) -> Result<()> {
        if sequence <= self.last_sent || self.blocked_on().is_some() {
            return Err(Error::Invalid);
        }
        self.last_sent = sequence;
        self.pending.push_back(sequence);
        Ok(())
    }

    pub fn failed(&mut self, sequence: u64) {
        self.pending.retain(|pending| *pending != sequence);
    }

    /// Reopen one slot after a bounded wait without acknowledging discarded
    /// frames. A receiver can drop both JPEG streams on read timeout; waiting
    /// only for their ACKs would then prevent any fresh frame from reaching it.
    /// Keep the other outstanding frame so the caller's no-progress deadline
    /// remains armed. Repeated retries must never reset that deadline.
    pub fn retry_stalled(&mut self) -> Option<u64> {
        self.blocked_on()?;
        self.pending.pop_front()
    }

    pub fn acknowledge(&mut self, sequence: u64) -> Result<bool> {
        if sequence > self.last_sent {
            return Err(Error::Invalid);
        }
        let before = self.pending.len();
        while self
            .pending
            .front()
            .is_some_and(|pending| *pending <= sequence)
        {
            self.pending.pop_front();
        }
        Ok(self.pending.len() < before)
    }
}

pub struct ByteBudget {
    rate: u64,
    available: f64,
    last: Instant,
}
impl ByteBudget {
    pub fn new(rate: u64) -> Self {
        Self {
            rate,
            available: rate as f64,
            last: Instant::now(),
        }
    }
    pub fn charge(&mut self, bytes: usize) -> Duration {
        self.charge_at(bytes, Instant::now())
    }
    fn charge_at(&mut self, bytes: usize, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.available = (self.available + elapsed * self.rate as f64).min(self.rate as f64);
        self.last = now;
        if self.rate == 0 || bytes as u64 > self.rate {
            return Duration::MAX;
        }
        if self.available >= bytes as f64 {
            self.available -= bytes as f64;
            Duration::ZERO
        } else {
            Duration::from_secs_f64((bytes as f64 - self.available) / self.rate as f64)
        }
    }
}

/// A frame is complete only after the viewer acknowledges its sequence.
/// A silent or slow consumer reaches the deadline and must be disconnected.
pub async fn wait_for_ack(
    receiver: &mut tokio::sync::watch::Receiver<u64>,
    sequence: u64,
    deadline: Duration,
) -> bool {
    tokio::time::timeout(deadline, async {
        loop {
            if *receiver.borrow_and_update() >= sequence {
                return true;
            }
            if receiver.changed().await.is_err() {
                return false;
            }
        }
    })
    .await
    .unwrap_or(false)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid media frame")]
    Invalid,
    #[error("encoded image exceeds the frame byte limit")]
    TooLarge,
    #[error("image codec: {0}")]
    Codec(String),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameMeta {
    pub monitor: u32,
    pub layout: u64,
    pub sequence: u64,
    pub captured_ms: u64,
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
}

#[derive(Clone, Debug)]
pub struct JpegFrame {
    pub meta: FrameMeta,
    pub jpeg: Vec<u8>,
}

fn geometry(width: u32, height: u32) -> Result<()> {
    if width == 0
        || height == 0
        || width > u16::MAX as u32
        || height > u16::MAX as u32
        || width.checked_mul(height).is_none_or(|n| n > MAX_PIXELS)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

impl JpegFrame {
    pub fn encode_rgb(meta: FrameMeta, rgb: &[u8], quality: u8) -> Result<Self> {
        geometry(meta.width, meta.height)?;
        if rgb.len() != meta.width as usize * meta.height as usize * 3
            || !(20..=90).contains(&quality)
        {
            return Err(Error::Invalid);
        }
        #[cfg(target_os = "android")]
        let jpeg = android_jpeg(rgb, meta.width, meta.height, quality)?;
        #[cfg(not(target_os = "android"))]
        let jpeg = portable_jpeg(rgb, meta.width, meta.height, quality)?;
        let frame = Self { meta, jpeg };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> Result<()> {
        geometry(self.meta.width, self.meta.height)?;
        if self.jpeg.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        if self.jpeg.is_empty() {
            return Err(Error::Invalid);
        }
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(&self.jpeg));
        decoder.read_info().map_err(|_| Error::Invalid)?;
        let info = decoder.info().ok_or(Error::Invalid)?;
        if u32::from(info.width) != self.meta.width || u32::from(info.height) != self.meta.height {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    /// Version 1, codec 1 = JPEG, followed by fixed big endian metadata and JPEG bytes.
    pub fn to_wire(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(HEADER + self.jpeg.len());
        out.extend_from_slice(b"FSM1");
        out.push(1);
        out.extend_from_slice(&self.meta.monitor.to_be_bytes());
        out.extend_from_slice(&self.meta.layout.to_be_bytes());
        out.extend_from_slice(&self.meta.sequence.to_be_bytes());
        out.extend_from_slice(&self.meta.captured_ms.to_be_bytes());
        out.extend_from_slice(&self.meta.width.to_be_bytes());
        out.extend_from_slice(&self.meta.height.to_be_bytes());
        out.extend_from_slice(&self.meta.origin_x.to_be_bytes());
        out.extend_from_slice(&self.meta.origin_y.to_be_bytes());
        out.extend_from_slice(&self.jpeg);
        Ok(out)
    }

    pub fn from_wire(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER + 1
            || &bytes[..4] != b"FSM1"
            || bytes[4] != 1
            || bytes.len() > HEADER + MAX_BYTES
        {
            return Err(Error::Invalid);
        }
        let u32_at = |n: usize| u32::from_be_bytes(bytes[n..n + 4].try_into().unwrap());
        let u64_at = |n: usize| u64::from_be_bytes(bytes[n..n + 8].try_into().unwrap());
        let frame = Self {
            meta: FrameMeta {
                monitor: u32_at(5),
                layout: u64_at(9),
                sequence: u64_at(17),
                captured_ms: u64_at(25),
                width: u32_at(33),
                height: u32_at(37),
                origin_x: u32_at(41) as i32,
                origin_y: u32_at(45) as i32,
            },
            jpeg: bytes[HEADER..].to_vec(),
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn decode_rgb(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(&self.jpeg));
        decoder.decode().map_err(|e| Error::Codec(e.to_string()))
    }
}

// Keep one compressor per capture thread. The library's ARM NEON path keeps
// full-color JPEG affordable without changing the media protocol or quality floor.
#[cfg(target_os = "android")]
fn android_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    thread_local! {
        static ENCODER: std::cell::RefCell<Option<turbojpeg::Compressor>> = const { std::cell::RefCell::new(None) };
    }
    ENCODER.with(|slot| {
        let codec = |error: turbojpeg::Error| Error::Codec(error.to_string());
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(turbojpeg::Compressor::new().map_err(codec)?);
        }
        let encoder = slot
            .as_mut()
            .ok_or_else(|| Error::Codec("JPEG encoder unavailable".into()))?;
        encoder.set_quality(i32::from(quality)).map_err(codec)?;
        encoder
            .set_subsamp(if quality >= 80 {
                turbojpeg::Subsamp::None
            } else {
                turbojpeg::Subsamp::Sub2x2
            })
            .map_err(codec)?;
        encoder
            .compress_to_vec(turbojpeg::Image {
                pixels: rgb,
                width: width as usize,
                pitch: width as usize * 3,
                height: height as usize,
                format: turbojpeg::PixelFormat::RGB,
            })
            .map_err(codec)
    })
}

#[cfg(any(not(target_os = "android"), test))]
fn portable_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    let mut jpeg = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, quality);
    if quality >= 80 {
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    }
    encoder
        .encode(
            rgb,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|error| Error::Codec(error.to_string()))?;
    Ok(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "android")]
    #[test]
    #[ignore = "explicit short benchmark on an attached ARM phone; synthetic pixels only"]
    fn android_codec_comparison() {
        let (width, height) = (1088, 502);
        let rgb: Vec<u8> = (0..width * height * 3)
            .map(|n| ((n * 37 + n / 17) % 256) as u8)
            .collect();
        let _ = android_jpeg(&rgb, width, height, 80).unwrap();
        let mut portable_us = 0;
        let mut accelerated_us = 0;
        let mut portable_bytes = 0;
        let mut accelerated_bytes = 0;
        for _ in 0..5 {
            let start = Instant::now();
            portable_bytes = portable_jpeg(&rgb, width, height, 80).unwrap().len();
            portable_us += start.elapsed().as_micros();
            let start = Instant::now();
            accelerated_bytes = android_jpeg(&rgb, width, height, 80).unwrap().len();
            accelerated_us += start.elapsed().as_micros();
        }
        eprintln!(
            "codec_comparison width={width} height={height} quality=80 frames=5 portable_us={portable_us} accelerated_us={accelerated_us} portable_bytes={portable_bytes} accelerated_bytes={accelerated_bytes}"
        );
    }

    #[test]
    fn viewport_and_preset_requests_are_bounded_and_preserve_aspect_ratio() {
        let request = ViewportRequest {
            generation: 7,
            edge: 704,
        };
        assert_eq!(
            ViewportRequest::parse(&request.packet(b"FSZ1"), b"FSZ1"),
            Some(request)
        );
        assert!(ViewportRequest::parse(&request.packet(b"FSZ1"), b"FSZ2").is_none());
        for edge in [0, 319, 2561, u16::MAX] {
            assert!(
                ViewportRequest::parse(
                    &ViewportRequest { edge, ..request }.packet(b"FSZ1"),
                    b"FSZ1"
                )
                .is_none()
            );
        }
        let mut extra = request.packet(b"FSZ1");
        extra.push(0);
        assert!(ViewportRequest::parse(&extra, b"FSZ1").is_none());
        for (source, preset, expected) in [
            ((2340, 1080), 0, (1280, 590)),
            ((1080, 2340), 0, (590, 1280)),
            ((2340, 1080), 2, (2340, 1080)),
            ((640, 360), 3, (640, 360)),
        ] {
            let profile = VideoProfile {
                profile: preset,
                generation: 2,
            };
            assert_eq!(profile.output_size(source.0, source.1), Some(expected));
            assert_eq!(
                VideoProfile::parse(&profile.packet(b"FSQ1"), b"FSQ1"),
                Some(profile)
            );
        }
        assert!(
            VideoProfile {
                profile: 4,
                generation: 0
            }
            .output_size(1920, 1080)
            .is_none()
        );
    }
    #[test]
    fn video_subscription_rejects_stale_reordered_and_malformed_commands() {
        let mut first = StreamDemand::default();
        let second = StreamDemand::default();
        assert!(first.advance(StreamDemand {
            generation: 2,
            active: false
        }));
        assert!(!first.advance(StreamDemand {
            generation: 1,
            active: true
        }));
        assert!(!first.advance(StreamDemand {
            generation: 2,
            active: true
        }));
        assert!(!first.active && second.active);
        assert!(first.advance(StreamDemand {
            generation: 3,
            active: true
        }));
        assert_eq!(StreamDemand::parse(&first.packet()), Some(first));
        let mut bad = first.packet();
        bad[12] = 2;
        assert!(StreamDemand::parse(&bad).is_none());
        assert!(StreamDemand::parse(&bad[..12]).is_none());
        bad.extend_from_slice(b"extra");
        assert!(StreamDemand::parse(&bad).is_none());
        assert!(StreamDemand::parse(b"FSW0").is_none());
    }
    #[test]
    fn media_window_bounds_stalled_receivers_and_rejects_unsent_ack() {
        let mut window = MediaWindow::default();
        window.sent(1).unwrap();
        window.sent(2).unwrap();
        assert_eq!(window.blocked_on(), Some(1));
        assert!(window.sent(3).is_err());
        assert!(window.acknowledge(3).is_err());
        assert_eq!(window.blocked_on(), Some(1));
        assert!(window.acknowledge(1).unwrap());
        window.sent(20).unwrap();
        assert_eq!(window.blocked_on(), Some(2));
        assert!(!window.acknowledge(1).unwrap());
        assert!(window.acknowledge(20).unwrap());
        assert_eq!(window.blocked_on(), None);
        assert!(window.sent(20).is_err());
    }

    #[test]
    fn failed_media_send_releases_capacity_without_reusing_sequence() {
        let mut window = MediaWindow::default();
        window.sent(1).unwrap();
        window.sent(2).unwrap();
        window.failed(2);
        assert_eq!(window.blocked_on(), None);
        assert!(!window.acknowledge(0).unwrap());
        assert!(window.sent(2).is_err());
        window.sent(3).unwrap();
        assert_eq!(window.blocked_on(), Some(1));
        assert!(window.acknowledge(2).unwrap());
        window.sent(4).unwrap();
        assert_eq!(window.blocked_on(), Some(3));
    }

    #[test]
    fn dropped_receive_window_allows_one_fresh_probe_without_faking_progress() {
        let mut window = MediaWindow::default();
        window.sent(1).unwrap();
        window.sent(2).unwrap();
        assert_eq!(window.retry_stalled(), Some(1));
        assert!(
            !window.is_empty(),
            "the hard ACK deadline must remain armed"
        );
        assert!(!window.acknowledge(1).unwrap());
        assert!(window.sent(2).is_err(), "never reuse discarded sequences");
        window.sent(30).unwrap();
        assert!(window.sent(31).is_err(), "only one retry slot is available");
        assert!(window.acknowledge(31).is_err());
        assert!(window.acknowledge(30).unwrap());
        assert!(window.is_empty());
        assert_eq!(window.retry_stalled(), None);
    }
    #[test]
    fn hd_sampling_retains_colored_text_edges_and_byte_limit_still_applies() {
        let meta = FrameMeta {
            monitor: 1,
            layout: 1,
            sequence: 1,
            captured_ms: 0,
            width: 64,
            height: 32,
            origin_x: 0,
            origin_y: 0,
        };
        let rgb: Vec<u8> = (0..64 * 32)
            .flat_map(|pixel| {
                if pixel % 2 == 0 {
                    [255, 0, 0]
                } else {
                    [0, 0, 255]
                }
            })
            .collect();
        let low = JpegFrame::encode_rgb(meta.clone(), &rgb, 60)
            .unwrap()
            .decode_rgb()
            .unwrap();
        let high = JpegFrame::encode_rgb(meta.clone(), &rgb, 85)
            .unwrap()
            .decode_rgb()
            .unwrap();
        let error = |decoded: &[u8]| {
            decoded
                .iter()
                .zip(&rgb)
                .map(|(actual, expected)| {
                    (i32::from(*actual) - i32::from(*expected)).unsigned_abs() as u64
                })
                .sum::<u64>()
        };
        assert!(
            error(&high) * 2 < error(&low),
            "desktop color edges must survive HD chroma encoding"
        );
        assert!(matches!(
            JpegFrame {
                meta,
                jpeg: vec![0; MAX_BYTES + 1]
            }
            .validate(),
            Err(Error::TooLarge)
        ));
    }
    #[test]
    fn roundtrip_and_reject_mismatched_dimensions() {
        let meta = FrameMeta {
            monitor: 2,
            layout: 3,
            sequence: 4,
            captured_ms: 5,
            width: 32,
            height: 16,
            origin_x: -100,
            origin_y: 20,
        };
        let frame = JpegFrame::encode_rgb(meta.clone(), &vec![127; 32 * 16 * 3], 60).unwrap();
        assert_eq!(
            JpegFrame::from_wire(&frame.to_wire().unwrap())
                .unwrap()
                .meta,
            meta
        );
        assert_eq!(frame.decode_rgb().unwrap().len(), 32 * 16 * 3);
        let mut bad = frame.to_wire().unwrap();
        bad[36] = 33;
        assert!(JpegFrame::from_wire(&bad).is_err());
    }
    #[test]
    fn native_2k_and_4k_roundtrip_stay_bounded() {
        for (width, height) in [(2560, 1440), (3840, 2160)] {
            let meta = FrameMeta {
                monitor: 1,
                layout: 1,
                sequence: 1,
                captured_ms: 1,
                width,
                height,
                origin_x: 0,
                origin_y: 0,
            };
            let rgb = vec![127; width as usize * height as usize * 3];
            let frame = JpegFrame::encode_rgb(meta.clone(), &rgb, 85).unwrap();
            let wire = frame.to_wire().unwrap();
            let decoded = JpegFrame::from_wire(&wire).unwrap();
            assert_eq!(decoded.meta, meta);
            assert_eq!(decoded.decode_rgb().unwrap().len(), rgb.len());
            assert!(wire.len() <= MAX_BYTES + HEADER);
            let mut oversized = meta;
            oversized.width += 1;
            if height == 2160 {
                assert!(JpegFrame::encode_rgb(oversized, &rgb, 85).is_err());
            }
        }
        let mut budget = ByteBudget::new(BASELINE_PAYLOAD_BPS);
        assert_eq!(budget.charge(MAX_BYTES + HEADER), Duration::ZERO);
        assert_ne!(budget.charge(MAX_BYTES + HEADER), Duration::MAX);
    }
    #[test]
    fn payload_budget_limits_sustained_rate() {
        let now = Instant::now();
        let mut budget = ByteBudget::new(1_000_000);
        assert_eq!(budget.charge_at(900_000, now), Duration::ZERO);
        assert!(budget.charge_at(300_000, now) >= Duration::from_millis(200));
        assert_eq!(
            budget.charge_at(300_000, now + Duration::from_millis(200)),
            Duration::ZERO
        );
        assert!(
            budget.charge_at(900_000, now + Duration::from_millis(200))
                >= Duration::from_millis(900)
        );
    }
    #[tokio::test]
    async fn missing_consumer_ack_expires_but_later_ack_advances() {
        let (sender, mut receiver) = tokio::sync::watch::channel(0u64);
        assert!(!wait_for_ack(&mut receiver, 1, Duration::from_millis(10)).await);
        sender.send_replace(2);
        assert!(wait_for_ack(&mut receiver, 1, Duration::from_millis(10)).await);
        assert!(!wait_for_ack(&mut receiver, 3, Duration::from_millis(10)).await);
    }
}
