//! In-memory synthetic JPEG throughput, not a remote-desktop FPS measurement.
use farsail_media::{FrameMeta, JpegFrame};
use std::{hint::black_box, time::Instant};

fn main() {
    println!("scene,width,height,median_ms,p95_ms,jpeg_bytes");
    for (width, height) in [(1280, 720), (1920, 1080), (2560, 1440)] {
        for textured in [false, true] {
            let mut state = 17u32;
            let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
            for y in 0..height {
                for x in 0..width {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let detail = if textured { (state & 63) as u8 } else { 0 };
                    rgb.extend_from_slice(&[
                        ((x / 8 + y / 16) as u8).wrapping_add(detail),
                        ((x / 16 + y / 8) as u8).wrapping_add(detail),
                        ((x / 32 + y / 32) as u8).wrapping_add(detail),
                    ]);
                }
            }
            let meta = FrameMeta {
                monitor: 1,
                layout: 1,
                sequence: 1,
                captured_ms: 0,
                width,
                height,
                origin_x: 0,
                origin_y: 0,
            };
            let mut times = Vec::new();
            let mut bytes = 0;
            for sample in 0..18 {
                let start = Instant::now();
                let frame = JpegFrame::encode_rgb(meta.clone(), black_box(&rgb), 85).unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                bytes = black_box(frame.jpeg.len());
                if sample >= 3 {
                    times.push(elapsed);
                }
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{},{width},{height},{:.3},{:.3},{bytes}",
                if textured { "textured" } else { "desktop" },
                times[times.len() / 2],
                times[times.len() - 1],
            );
        }
    }
}
