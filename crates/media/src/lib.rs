//! Bounded JPEG baseline for remote desktop frames. Never call this H.265.
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::time::{Duration, Instant};

pub const MAX_PIXELS: u32 = 2_073_600;
pub const MAX_BYTES: usize = 1_000_000;
/// JPEG payload budget, excluding QUIC/TLS/IP headers and retransmissions.
pub const BASELINE_PAYLOAD_BPS: u64 = 1_500_000;
const HEADER: usize = 49;

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
        let mut jpeg = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, quality);
        if quality >= 80 {
            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
        }
        encoder
            .encode(
                rgb,
                meta.width as u16,
                meta.height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(|e| Error::Codec(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
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
