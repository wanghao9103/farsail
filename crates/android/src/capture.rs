//! Preserve RGBA capture color edges instead of Android's subsampled JPEG path.
use farsail_media::{FrameMeta, JpegFrame, MAX_PIXELS};

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

pub(crate) fn encode_rgba(
    bytes: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
    layout: u64,
) -> Option<JpegFrame> {
    let rgb = rgba_to_rgb(bytes, width, height, row_stride)?;
    let meta = FrameMeta {
        monitor: 1,
        layout,
        sequence: 0,
        captured_ms: 0,
        width,
        height,
        origin_x: 0,
        origin_y: 0,
    };
    // Both qualities use the media encoder's 4:4:4 sampling. Keep its byte bound.
    match JpegFrame::encode_rgb(meta.clone(), &rgb, 90) {
        Ok(frame) => Some(frame),
        Err(farsail_media::Error::TooLarge) => JpegFrame::encode_rgb(meta, &rgb, 85).ok(),
        Err(_) => None,
    }
}

/// Retain pixels only for the active capture, avoiding repeated DCT work on
/// identical screenshots. Every call still validates a newly obtained buffer.
#[derive(Default)]
pub(crate) struct ScreenshotEncoder {
    cached: Option<(u32, u32, Vec<u8>, JpegFrame)>,
}
impl ScreenshotEncoder {
    pub fn clear(&mut self) {
        self.cached = None;
    }
    pub fn encode(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
    ) -> Option<JpegFrame> {
        let span = rgba_span(width, height, stride, 4)?;
        let source = bytes.get(..span)?;
        let row_bytes = width as usize * 4;
        if let Some((w, h, pixels, image)) = &self.cached
            && (*w, *h) == (width, height)
            && (0..height as usize).all(|y| {
                source[y * stride..y * stride + row_bytes]
                    == pixels[y * row_bytes..(y + 1) * row_bytes]
            })
        {
            return Some(image.clone());
        }
        let image = encode_rgba(source, width, height, stride, 0)?;
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        for y in 0..height as usize {
            pixels.extend_from_slice(&source[y * stride..y * stride + row_bytes]);
        }
        self.cached = Some((width, height, pixels, image.clone()));
        Some(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
