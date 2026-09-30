//! Windows desktop capture and local system input. No credentials enter this crate.
use farsail_media::{FrameMeta, JpegFrame};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Windows interactive desktop unavailable: {0}")]
    Unavailable(String),
    #[error("invalid monitor or coordinates")]
    Geometry,
    #[error("input denied or desktop changed")]
    InputDenied,
    #[error("unsupported keyboard, wheel or text input")]
    UnsupportedInput,
    #[error("Windows input rejected: inserted {inserted}/{expected}, code {code}")]
    Injection {
        inserted: u32,
        expected: u32,
        code: u32,
    },
    #[error("image: {0}")]
    Image(#[from] farsail_media::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Display {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub rotation: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Move {
        display: u32,
        layout: u64,
        x: f64,
        y: f64,
    },
    Button {
        display: u32,
        layout: u64,
        x: f64,
        y: f64,
        button: Button,
        down: bool,
    },
    Wheel {
        display: u32,
        layout: u64,
        x: f64,
        y: f64,
        vertical: i32,
        horizontal: i32,
    },
    Key {
        vk: u16,
        down: bool,
        #[serde(default)]
        repeat: bool,
    },
    Text {
        text: String,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
}

pub fn map_point(display: &Display, x: f64, y: f64) -> Result<(i32, i32)> {
    if !x.is_finite()
        || !y.is_finite()
        || !(0.0..=1.0).contains(&x)
        || !(0.0..=1.0).contains(&y)
        || display.width == 0
        || display.height == 0
    {
        return Err(Error::Geometry);
    }
    let px = display.x as i64 + (x * (display.width - 1) as f64).round() as i64;
    let py = display.y as i64 + (y * (display.height - 1) as f64).round() as i64;
    Ok((
        i32::try_from(px).map_err(|_| Error::Geometry)?,
        i32::try_from(py).map_err(|_| Error::Geometry)?,
    ))
}

#[cfg(windows)]
mod dxgi;
#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::{Capture, InputSink, displays, ensure_dpi_awareness};

#[cfg(not(windows))]
pub fn displays() -> Result<Vec<Display>> {
    Err(Error::Unavailable("Windows required".into()))
}

pub fn encode_bgra(
    meta: FrameMeta,
    data: &[u8],
    row_pitch: usize,
    bgra: bool,
) -> Result<JpegFrame> {
    let width = meta.width as usize;
    let height = meta.height as usize;
    if width == 0
        || height == 0
        || width.checked_mul(4).is_none_or(|n| n > row_pitch)
        || data.len() < height.checked_mul(row_pitch).ok_or(Error::Geometry)?
    {
        return Err(Error::Geometry);
    }
    let mut rgb = vec![0; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let p = &data[y * row_pitch + x * 4..][..4];
            let out = &mut rgb[(y * width + x) * 3..][..3];
            if bgra {
                out.copy_from_slice(&[p[2], p[1], p[0]]);
            } else {
                out.copy_from_slice(&p[..3]);
            }
        }
    }
    Ok(JpegFrame::encode_rgb(meta, &rgb, 55)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_origin_and_invalid_coordinates() {
        let d = Display {
            id: 1,
            name: "test".into(),
            x: -1920,
            y: -40,
            width: 1920,
            height: 1080,
            dpi: 144,
            rotation: 0,
        };
        assert_eq!(map_point(&d, 0.0, 0.0).unwrap(), (-1920, -40));
        assert_eq!(map_point(&d, 1.0, 1.0).unwrap(), (-1, 1039));
        assert!(map_point(&d, f64::NAN, 0.0).is_err());
        assert!(map_point(&d, 1.1, 0.0).is_err());
    }
    #[test]
    fn input_protocol_rejects_extra_fields() {
        assert!(
            serde_json::from_str::<Input>(r#"{"kind":"key","vk":65,"down":true,"extra":9}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Input>(r#"{"kind":"button","button":"middle","down":true,"display":1,"layout":1,"x":0.5,"y":0.5}"#).is_err());
    }
}
