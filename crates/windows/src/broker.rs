//! Prefer the authenticated installed system host; portable/dev copies stay local.
use crate::{
    Display, Error, Input, JpegFrame, Result, native,
    system_host::Client,
    system_protocol::{Request, Response, input_result},
};
pub fn displays() -> Result<Vec<Display>> {
    if let Ok(mut client) = Client::connect() {
        return match client.request(&Request::Displays {})? {
            Response::Displays { displays } => Ok(displays),
            _ => Err(Error::Unavailable(
                "system display enumeration failed".into(),
            )),
        };
    }
    native::displays()
}
enum Source {
    System(Client),
    Local(native::Capture),
}
pub struct Capture {
    source: Source,
    display: Display,
    profile: u8,
    desktop: Vec<u16>,
}
impl Capture {
    pub fn new(id: u32) -> Result<Self> {
        Self::new_with_profile(id, 1)
    }
    pub fn new_with_profile(id: u32, profile: u8) -> Result<Self> {
        if let Ok(mut client) = Client::connect() {
            let Response::Displays { displays } = client.request(&Request::Displays {})? else {
                return Err(Error::Geometry);
            };
            let display = displays
                .into_iter()
                .find(|d| d.id == id)
                .ok_or(Error::Geometry)?;
            if profile > 3 {
                return Err(Error::Geometry);
            }
            return Ok(Self {
                source: Source::System(client),
                display,
                profile,
                desktop: Vec::new(),
            });
        }
        let capture = native::Capture::new_with_profile(id, profile)?;
        let display = capture.display().clone();
        Ok(Self {
            source: Source::Local(capture),
            display,
            profile,
            desktop: Vec::new(),
        })
    }
    pub fn desktop_context(&self) -> &[u16] {
        &self.desktop
    }
    pub fn display(&self) -> &Display {
        &self.display
    }
    pub fn next_frame(&mut self, layout: u64, sequence: u64) -> Result<Option<JpegFrame>> {
        match &mut self.source {
            Source::Local(c) => c.next_frame(layout, sequence),
            Source::System(c) => match c.request(&Request::Capture {
                display: self.display.id,
                profile: self.profile,
                layout,
                sequence,
            })? {
                Response::Frame { bytes, desktop } => {
                    self.desktop = desktop;
                    let frame = JpegFrame::from_wire(&bytes)?;
                    if frame.meta.monitor != self.display.id
                        || frame.meta.sequence != sequence
                        || frame.meta.layout != layout
                    {
                        return Err(Error::Geometry);
                    }
                    Ok(Some(frame))
                }
                Response::Empty => Ok(None),
                Response::Reset => Err(Error::CaptureReset("system desktop changed".into())),
                _ => Err(Error::CaptureReset(
                    "system desktop capture temporarily unavailable".into(),
                )),
            },
        }
    }
}
#[derive(Default)]
pub struct InputSink {
    system: Option<Client>,
    local: native::InputSink,
    initialized: bool,
    desktop: Vec<u16>,
    desktop_layout: u64,
}
impl InputSink {
    pub fn bind_desktop(&mut self, desktop: &[u16], layout: u64) {
        self.desktop = desktop.to_vec();
        self.desktop_layout = layout;
    }
    fn init(&mut self) {
        if !self.initialized {
            self.system = Client::connect().ok();
            self.initialized = true;
        }
    }
    pub fn apply(&mut self, input: Input, display: Option<&Display>, layout: u64) -> Result<()> {
        self.init();
        if let Some(c) = &mut self.system {
            if layout == 0 || layout != self.desktop_layout {
                return Err(Error::Geometry);
            }
            return input_result(c.request(&Request::Input {
                input,
                display: display.cloned(),
                layout,
                desktop: self.desktop.clone(),
            })?);
        }
        self.local.apply(input, display, layout)
    }
    pub fn release_all(&mut self) -> Result<()> {
        if let Some(c) = &mut self.system {
            return input_result(c.request(&Request::Release {})?);
        }
        self.local.release_all()
    }
}
impl Drop for InputSink {
    fn drop(&mut self) {
        let _ = self.release_all();
    }
}
