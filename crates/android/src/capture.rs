//! Preserve RGBA capture color edges instead of Android's subsampled JPEG path.
use farsail_media::{FrameMeta, JpegFrame, MAX_PIXELS, VideoProfile};
use std::time::{Duration, Instant};

/// Keep full-color sampling and the requested pixel size. Back off DCT quality
/// on delivery queueing; recover slowly to avoid repeatedly filling a shared link.
pub(crate) struct DeliveryQuality {
    pub value: u8,
    pub overlap_allowed: bool,
    changed: Instant,
    healthy: Option<Instant>,
    base_rtt: Option<u64>,
}
impl Default for DeliveryQuality {
    fn default() -> Self {
        Self {
            value: 90,
            overlap_allowed: false,
            changed: Instant::now(),
            healthy: None,
            base_rtt: None,
        }
    }
}
impl DeliveryQuality {
    pub fn with_rtt(rtt: Option<u64>) -> Self {
        Self {
            base_rtt: rtt,
            ..Self::default()
        }
    }
    pub fn observe(&mut self, ack_ms: u64, send_ms: u64, rtt_ms: u64, now: Instant) {
        let base = self.base_rtt.map_or(rtt_ms, |base| base.min(rtt_ms));
        self.base_rtt = Some(base);
        let high = base.saturating_mul(2).saturating_add(60).clamp(120, 220);
        // Overlap a healthy network round trip, but return to one outstanding
        // frame immediately when delivery queues. MediaWindow still caps at two.
        self.overlap_allowed = ack_ms <= high && send_ms < 80 && rtt_ms <= base.saturating_add(20);
        if ack_ms > high || send_ms > high / 2 || rtt_ms > base.saturating_add(60) {
            self.healthy = None;
            if now.saturating_duration_since(self.changed) >= Duration::from_secs(1) {
                self.value = self.value.saturating_sub(5).max(80);
                self.changed = now;
            }
        } else if ack_ms < high * 3 / 4 && send_ms < 80 && rtt_ms <= base.saturating_add(20) {
            let since = *self.healthy.get_or_insert(now);
            if now.saturating_duration_since(since) >= Duration::from_secs(5)
                && now.saturating_duration_since(self.changed) >= Duration::from_secs(2)
            {
                self.value = self.value.saturating_add(1).min(90);
                self.changed = now;
            }
        } else {
            self.healthy = None;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Settings {
    pub profile: VideoProfile,
    pub edge: Option<u16>,
    pub quality: u8,
}
impl Settings {
    pub fn output_size(self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = self.profile.output_size(width, height)?;
        let edge = u32::from(self.edge.unwrap_or(u16::MAX));
        if w.max(h) <= edge {
            return Some((w, h));
        }
        if w >= h {
            Some((
                edge,
                (u64::from(h) * u64::from(edge) / u64::from(w)).max(1) as u32,
            ))
        } else {
            Some((
                (u64::from(w) * u64::from(edge) / u64::from(h)).max(1) as u32,
                edge,
            ))
        }
    }
}

pub(crate) fn rgba_span(
    width: u32,
    height: u32,
    row_stride: usize,
    pixel_stride: usize,
) -> Option<usize> {
    if width == 0
        || height == 0
        || width > u16::MAX as u32
        || height > u16::MAX as u32
        || width.checked_mul(height)? > MAX_PIXELS
        || pixel_stride != 4
    {
        return None;
    }
    let row_bytes = (width as usize).checked_mul(4)?;
    if row_stride < row_bytes {
        return None;
    }
    (height as usize - 1)
        .checked_mul(row_stride)?
        .checked_add(row_bytes)
}

fn rgba_to_rgb(bytes: &[u8], width: u32, height: u32, row_stride: usize) -> Option<Vec<u8>> {
    let span = rgba_span(width, height, row_stride, 4)?;
    let bytes = bytes.get(..span)?;
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height as usize {
        let row = &bytes[y * row_stride..y * row_stride + width as usize * 4];
        for pixel in row.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    Some(rgb)
}

#[cfg(test)]
pub(crate) fn encode_rgba(
    bytes: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
    layout: u64,
) -> Option<JpegFrame> {
    encode_rgba_profile(
        bytes,
        width,
        height,
        row_stride,
        layout,
        Settings {
            profile: VideoProfile {
                profile: 2,
                generation: 0,
            },
            edge: None,
            quality: 90,
        },
    )
}

pub(crate) fn encode_rgba_profile(
    bytes: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
    layout: u64,
    profile: Settings,
) -> Option<JpegFrame> {
    encode_rgba_profile_timed(bytes, width, height, row_stride, layout, profile)
        .map(|out| out.frame)
}

pub(crate) struct EncodedCapture {
    pub frame: JpegFrame,
    pub convert_ms: u64,
    pub jpeg_ms: u64,
}

pub(crate) fn encode_rgba_profile_timed(
    bytes: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
    layout: u64,
    profile: Settings,
) -> Option<EncodedCapture> {
    let started = Instant::now();
    let span = rgba_span(width, height, row_stride, 4)?;
    let bytes = bytes.get(..span)?;
    let (out_w, out_h) = profile.output_size(width, height)?;
    let rgb = if (out_w, out_h) == (width, height) {
        rgba_to_rgb(bytes, width, height, row_stride)?
    } else {
        // Bilinear sampling keeps colored text edges while encoding only the
        // pixels needed by this viewer. No extra full-resolution RGB copy.
        let axis = |index: u32, source: u32, output: u32| {
            let fixed = ((u64::from(index) * 2 + 1) * u64::from(source) * 256
                / (u64::from(output) * 2))
                .saturating_sub(128)
                .min(u64::from(source - 1) * 256);
            let lo = (fixed / 256) as usize;
            (lo, (lo + 1).min(source as usize - 1), (fixed % 256) as u32)
        };
        let xs: Vec<_> = (0..out_w).map(|x| axis(x, width, out_w)).collect();
        let mut rgb = Vec::with_capacity(out_w as usize * out_h as usize * 3);
        for y in 0..out_h {
            let (y0, y1, wy) = axis(y, height, out_h);
            for &(x0, x1, wx) in &xs {
                for channel in 0..3 {
                    let p = |x, y| u32::from(bytes[y * row_stride + x * 4 + channel]);
                    let top = p(x0, y0) * (256 - wx) + p(x1, y0) * wx;
                    let bottom = p(x0, y1) * (256 - wx) + p(x1, y1) * wx;
                    rgb.push(((top * (256 - wy) + bottom * wy + 32768) >> 16) as u8);
                }
            }
        }
        rgb
    };
    let meta = FrameMeta {
        monitor: 1,
        layout,
        sequence: 0,
        captured_ms: 0,
        width: out_w,
        height: out_h,
        origin_x: 0,
        origin_y: 0,
    };
    let convert_ms = started.elapsed().as_millis() as u64;
    let jpeg_started = Instant::now();
    // All fallback qualities retain 4:4:4 sampling and the existing byte bound.
    // A native noisy frame may need Q80 even when Q85 is still too large.
    let mut quality = profile.quality;
    loop {
        match JpegFrame::encode_rgb(meta.clone(), &rgb, quality) {
            Ok(frame) => {
                return Some(EncodedCapture {
                    frame,
                    convert_ms,
                    jpeg_ms: jpeg_started.elapsed().as_millis() as u64,
                });
            }
            Err(farsail_media::Error::TooLarge) if quality > 80 => {
                quality = (quality - 5).max(80);
            }
            Err(_) => return None,
        }
    }
}

/// Retain pixels only for the active capture, avoiding repeated DCT work on
/// identical screenshots. Every call still validates a newly obtained buffer.
#[derive(Default)]
pub(crate) struct ScreenshotEncoder {
    cached: Option<CachedScreenshot>,
}
struct CachedScreenshot {
    source: (u32, u32),
    encoding: (u8, Option<u16>, u8),
    pixels: Vec<u8>,
    image: JpegFrame,
}
impl ScreenshotEncoder {
    pub fn clear(&mut self) {
        self.cached = None;
    }
    #[cfg(test)]
    pub fn encode(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
    ) -> Option<JpegFrame> {
        self.encode_profile(
            bytes,
            width,
            height,
            stride,
            Settings {
                profile: VideoProfile {
                    profile: 2,
                    generation: 0,
                },
                edge: None,
                quality: 90,
            },
        )
    }
    pub fn encode_profile(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        profile: Settings,
    ) -> Option<JpegFrame> {
        let span = rgba_span(width, height, stride, 4)?;
        let source = bytes.get(..span)?;
        let row_bytes = width as usize * 4;
        let encoding = (profile.profile.profile, profile.edge, profile.quality);
        if let Some(cached) = &self.cached
            && cached.source == (width, height)
            && cached.encoding == encoding
            && (0..height as usize).all(|y| {
                source[y * stride..y * stride + row_bytes]
                    == cached.pixels[y * row_bytes..(y + 1) * row_bytes]
            })
        {
            return Some(cached.image.clone());
        }
        let image = encode_rgba_profile(source, width, height, stride, 0, profile)?;
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        for y in 0..height as usize {
            pixels.extend_from_slice(&source[y * stride..y * stride + row_bytes]);
        }
        self.cached = Some(CachedScreenshot {
            source: (width, height),
            encoding,
            pixels,
            image: image.clone(),
        });
        Some(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_native_frame_retries_the_supported_quality_floor() {
        let (width, height) = (1080, 2560);
        let mut state = 0x12345678u32;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            for _ in 0..3 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                rgba.push((state >> 24) as u8);
            }
            rgba.push(255);
        }
        let rgb = rgba_to_rgb(&rgba, width as u32, height as u32, width * 4).unwrap();
        let meta = FrameMeta {
            monitor: 1,
            layout: 1,
            sequence: 0,
            captured_ms: 0,
            width: width as u32,
            height: height as u32,
            origin_x: 0,
            origin_y: 0,
        };
        let middle = JpegFrame::encode_rgb(meta.clone(), &rgb, 85);
        eprintln!(
            "Q85 bytes/error: {:?}",
            middle.as_ref().map(|frame| frame.jpeg.len())
        );
        assert!(matches!(middle, Err(farsail_media::Error::TooLarge)));
        let floor =
            JpegFrame::encode_rgb(meta, &rgb, 80).expect("the supported quality floor fits");
        eprintln!("Q80 bytes: {}", floor.jpeg.len());
        let actual = encode_rgba_profile(
            &rgba,
            width as u32,
            height as u32,
            width * 4,
            1,
            Settings {
                profile: VideoProfile {
                    profile: 2,
                    generation: 0,
                },
                edge: None,
                quality: 90,
            },
        )
        .expect("must try the legal floor before dropping the image");
        assert_eq!(actual.jpeg, floor.jpeg);
        assert_eq!((actual.meta.width, actual.meta.height), (1080, 2560));
    }

    #[test]
    fn delivery_feedback_uses_base_rtt_and_recovers_quality_slowly() {
        let mut quality = DeliveryQuality::with_rtt(Some(20));
        let start = quality.changed;
        quality.observe(300, 100, 150, start + Duration::from_secs(1));
        assert_eq!(quality.value, 85);
        quality.observe(300, 100, 150, start + Duration::from_secs(2));
        assert_eq!(quality.value, 80);
        quality.observe(300, 100, 150, start + Duration::from_secs(3));
        assert_eq!(
            quality.value, 80,
            "never lose full-color sampling by dropping below 80"
        );
        quality.observe(35, 5, 20, start + Duration::from_secs(4));
        quality.observe(35, 5, 20, start + Duration::from_secs(8));
        assert_eq!(
            quality.value, 80,
            "avoid reacting to one briefly healthy interval"
        );
        quality.observe(35, 5, 20, start + Duration::from_secs(9));
        assert_eq!(quality.value, 81);
    }

    #[test]
    fn healthy_delivery_can_overlap_but_queueing_immediately_disables_it() {
        let mut quality = DeliveryQuality::with_rtt(Some(20));
        let start = quality.changed;
        assert!(!quality.overlap_allowed);
        quality.observe(60, 5, 20, start);
        assert!(quality.overlap_allowed);
        quality.observe(343, 0, 95, start);
        assert!(!quality.overlap_allowed);
        quality.observe(60, 90, 20, start);
        assert!(!quality.overlap_allowed);
        quality.observe(60, 5, 50, start);
        assert!(!quality.overlap_allowed);
        quality.observe(60, 5, 20, start);
        assert!(quality.overlap_allowed);
    }

    #[test]
    fn screenshot_cache_tracks_resolution_and_quality_without_reusing_old_encodes() {
        let mut encoder = ScreenshotEncoder::default();
        let rgba: Vec<u8> = (0..960 * 480)
            .flat_map(|i| [(i % 255) as u8, ((i / 3) % 255) as u8, 64, 255])
            .collect();
        let mut settings = Settings {
            profile: VideoProfile {
                profile: 2,
                generation: 1,
            },
            edge: Some(640),
            quality: 90,
        };
        let first = encoder
            .encode_profile(&rgba, 960, 480, 960 * 4, settings)
            .unwrap();
        assert_eq!((first.meta.width, first.meta.height), (640, 320));
        settings.quality = 80;
        let lower = encoder
            .encode_profile(&rgba, 960, 480, 960 * 4, settings)
            .unwrap();
        assert_ne!(lower.jpeg, first.jpeg);
        settings.edge = None;
        settings.quality = 90;
        let full = encoder
            .encode_profile(&rgba, 960, 480, 960 * 4, settings)
            .unwrap();
        assert_eq!((full.meta.width, full.meta.height), (960, 480));
        assert_eq!(
            full.jpeg,
            encode_rgba(&rgba, 960, 480, 960 * 4, 0).unwrap().jpeg
        );
        let sof = lower
            .jpeg
            .windows(2)
            .position(|bytes| bytes == [0xff, 0xc0])
            .unwrap();
        for offset in [11, 14, 17] {
            assert_eq!(lower.jpeg[sof + offset], 0x11);
        }
    }

    #[test]
    fn padded_rows_keep_rgb_order_and_allow_unmapped_final_padding() {
        let bytes = [
            1, 2, 3, 255, 4, 5, 6, 255, 99, 99, 99, 99, 7, 8, 9, 255, 10, 11, 12, 255,
        ];
        assert_eq!(rgba_span(2, 2, 12, 4), Some(20));
        assert_eq!(
            rgba_to_rgb(&bytes, 2, 2, 12).unwrap(),
            (1..=12).collect::<Vec<u8>>()
        );
        assert!(rgba_to_rgb(&bytes[..19], 2, 2, 12).is_none());
        assert!(rgba_span(2, 2, 7, 4).is_none());
        assert!(rgba_span(2, 2, 8, 3).is_none());
        assert!(rgba_span(0, 2, 8, 4).is_none());
        assert!(rgba_span(u32::MAX, 2, 8, 4).is_none());
        assert!(rgba_span(1, 2, usize::MAX, 4).is_none());
    }

    #[test]
    fn screenshot_cache_respects_content_padding_geometry_and_clear() {
        let mut encoder = ScreenshotEncoder::default();
        let mut rgba = vec![255u8; 16 * 16 * 4];
        let first = encoder.encode(&rgba, 16, 16, 64).unwrap();
        let mut padded = Vec::new();
        for row in rgba.chunks_exact(64) {
            padded.extend_from_slice(row);
            padded.extend_from_slice(&[7; 8]);
        }
        assert_eq!(
            encoder.encode(&padded, 16, 16, 72).unwrap().jpeg,
            first.jpeg
        );
        assert!(
            encoder
                .encode(&rgba[..rgba.len() - 1], 16, 16, 64)
                .is_none()
        );
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[1] = 0;
            pixel[2] = 0;
        }
        let changed = encoder.encode(&rgba, 16, 16, 64).unwrap();
        assert_ne!(changed.jpeg, first.jpeg);
        let rotated = encoder.encode(&rgba, 8, 32, 32).unwrap();
        assert_eq!((rotated.meta.width, rotated.meta.height), (8, 32));
        encoder.clear();
        assert!(encoder.cached.is_none());
    }

    #[test]
    fn colored_pixel_edges_survive_capture_encoding() {
        let rgba: Vec<u8> = (0..64 * 32)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        let frame = encode_rgba(&rgba, 64, 32, 64 * 4, 7).unwrap();
        let sof = frame
            .jpeg
            .windows(2)
            .position(|marker| marker == [0xff, 0xc0])
            .unwrap();
        assert_eq!(frame.jpeg[sof + 9], 3);
        for offset in [11, 14, 17] {
            assert_eq!(
                frame.jpeg[sof + offset],
                0x11,
                "all JPEG components must retain full color resolution"
            );
        }
        let expected = rgba_to_rgb(&rgba, 64, 32, 64 * 4).unwrap();
        let decoded = frame.decode_rgb().unwrap();
        let baseline = JpegFrame::encode_rgb(frame.meta.clone(), &expected, 60)
            .unwrap()
            .decode_rgb()
            .unwrap();
        let error = |image: &[u8]| {
            image
                .iter()
                .zip(&expected)
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).unsigned_abs() as u64)
                .sum::<u64>()
        };
        assert!(error(&decoded) * 2 < error(&baseline));
        assert_eq!(frame.meta.layout, 7);
        assert!(frame.jpeg.len() <= farsail_media::MAX_BYTES);
    }
}
