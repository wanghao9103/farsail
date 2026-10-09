//! The viewer uses the shared wire types, but this platform cannot host a desktop.
use super::{Display, Error, Input, JpegFrame, Result};

fn unsupported() -> Error {
    Error::Unavailable("local screen sharing is only supported on Windows".into())
}

pub fn displays() -> Result<Vec<Display>> {
    Err(unsupported())
}

pub struct Capture {
    display: Display,
}
impl Capture {
    pub fn new(_id: u32) -> Result<Self> {
        Err(unsupported())
    }
    pub fn new_with_profile(_id: u32, _profile: u8) -> Result<Self> {
        Err(unsupported())
    }
    pub fn display(&self) -> &Display {
        &self.display
    }
    pub fn next_frame(&mut self, _layout: u64, _sequence: u64) -> Result<Option<JpegFrame>> {
        Err(unsupported())
    }
}

#[derive(Default)]
pub struct InputSink {
    _private: (),
}
impl InputSink {
    pub fn apply(&mut self, _input: Input, _display: Option<&Display>, _layout: u64) -> Result<()> {
        Err(unsupported())
    }
    pub fn release_all(&mut self) -> Result<()> {
        Ok(())
    }
}
