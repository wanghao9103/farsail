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
