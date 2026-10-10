use super::dxgi::Duplication;
use super::{Button, Display, Error, Input, Result, map_point};
use farsail_media::{FrameMeta, JpegFrame};
use std::{
    collections::HashSet,
    mem::size_of,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::Win32::{
    Foundation::{GetLastError, RECT, SetLastError, WIN32_ERROR},
    Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO},
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_PER_MONITOR_AWARE,
            GetAwarenessFromDpiAwarenessContext, GetDpiForMonitor, GetThreadDpiAwarenessContext,
            MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
            MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
            VIRTUAL_KEY,
        },
        WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        },
    },
};
use windows_capture::monitor::Monitor;

pub fn ensure_dpi_awareness() -> Result<()> {
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let current = unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
    if current != DPI_AWARENESS_PER_MONITOR_AWARE {
        return Err(Error::Unavailable(
            "per-monitor DPI awareness required for screen coordinates".into(),
        ));
    }
    Ok(())
}

fn desc(m: Monitor, id: u32) -> Result<Display> {
    let h = HMONITOR(m.as_raw_hmonitor());
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(h, &mut info).ok() }.map_err(|e| Error::Unavailable(e.to_string()))?;
    let RECT {
        left,
        top,
        right,
        bottom,
    } = info.rcMonitor;
    let (mut dx, mut dy) = (96, 96);
    unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) }
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let width = u32::try_from(right - left).map_err(|_| Error::Geometry)?;
    let height = u32::try_from(bottom - top).map_err(|_| Error::Geometry)?;
    if width == 0 || height == 0 {
        return Err(Error::Geometry);
    }
    Ok(Display {
        id,
        name: m
            .name()
            .unwrap_or_else(|_| m.device_name().unwrap_or_else(|_| format!("Display {id}"))),
        x: left,
        y: top,
        width,
        height,
        dpi: dx,
        rotation: 0,
    })
}

pub fn displays() -> Result<Vec<Display>> {
    ensure_dpi_awareness()?;
    let monitors = Monitor::enumerate().map_err(|e| Error::Unavailable(e.to_string()))?;
    monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| desc(m, (i + 1) as u32))
        .collect()
}

pub struct Capture {
    display: Display,
    dxgi: Duplication,
    last_rgb: Option<Vec<u8>>,
    profile: u8,
}
fn output_size(width: usize, height: usize, profile: u8) -> Result<(usize, usize)> {
    if width == 0 || height == 0 || profile > 3 {
        return Err(Error::Geometry);
    }
    let (long, short) = match profile {
        0 => (1280.0, 720.0),
        1 => (1920.0, 1080.0),
        2 => (2560.0, 1440.0),
        _ => (3840.0, 2160.0),
    };
    let (max_w, max_h) = if width >= height {
        (long, short)
    } else {
        (short, long)
    };
    let factor = (width as f64 / max_w).max(height as f64 / max_h).max(1.0);
    Ok((
        (width as f64 / factor).floor().max(1.0) as usize,
        (height as f64 / factor).floor().max(1.0) as usize,
    ))
}
impl Capture {
    pub fn new(id: u32) -> Result<Self> {
        Self::new_with_profile(id, 1)
    }
    pub fn new_with_profile(id: u32, profile: u8) -> Result<Self> {
        if profile > 3 {
            return Err(Error::Geometry);
        }
        if id == 0 {
            return Err(Error::Geometry);
        }
        let monitor =
            Monitor::from_index(id as usize).map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut display = desc(monitor, id)?;
        let dxgi = Duplication::new(monitor.as_raw_hmonitor())?;
        display.rotation = dxgi.rotation;
        Ok(Self {
            display,
            dxgi,
            last_rgb: None,
            profile,
        })
    }
    pub fn display(&self) -> &Display {
        &self.display
    }
    pub fn next_frame(&mut self, layout: u64, sequence: u64) -> Result<Option<JpegFrame>> {
        let Some(frame) = self.dxgi.frame()? else {
            return Ok(None);
        };
        let source_width = frame.width;
        let source_height = frame.height;
        let pitch = frame.pitch;
        let raw = &frame.data;
        if source_width == 0
            || source_height == 0
            || source_width.checked_mul(4).is_none_or(|n| n > pitch)
            || raw.len() < pitch.saturating_mul(source_height)
        {
            return Err(Error::Geometry);
        }
        // DXGI's buffer stays unrotated. Rotate as indicated by the duplication descriptor,
        // then scale into a bounded JPEG baseline without making a full-size intermediate copy.
        let rotated = self.display.rotation;
        let (logical_w, logical_h) = if rotated == 2 || rotated == 4 {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if logical_w != self.display.width as usize || logical_h != self.display.height as usize {
            return Err(Error::Unavailable(
                "display geometry changed; reconnect after selecting the new layout".into(),
            ));
        }
        let (out_w, out_h) = output_size(logical_w, logical_h, self.profile)?;
        let rgb = convert_bgra(
            raw,
            source_width,
            source_height,
            pitch,
            rotated,
            out_w,
            out_h,
        )?;
        if self.last_rgb.as_ref() == Some(&rgb) {
            return Ok(None);
        }
        let captured_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .as_millis() as u64;
        let meta = FrameMeta {
            monitor: self.display.id,
            layout,
            sequence,
            captured_ms,
            width: out_w as u32,
            height: out_h as u32,
            origin_x: self.display.x,
            origin_y: self.display.y,
        };
        let frame = encode_profile(meta, &rgb, self.profile)?;
        self.last_rgb = Some(rgb);
        Ok(Some(frame))
    }
}

fn encode_profile(meta: FrameMeta, rgb: &[u8], profile: u8) -> Result<JpegFrame> {
    let qualities: &[u8] = if profile == 0 {
        &[60, 45, 30]
    } else {
        &[85, 65, 45, 30]
    };
    for &quality in qualities {
        match JpegFrame::encode_rgb(meta.clone(), rgb, quality) {
            Ok(frame) if profile >= 2 || frame.jpeg.len() <= 1_000_000 => {
                return Ok(frame);
            }
            Ok(_) => continue,
            Err(farsail_media::Error::TooLarge) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(farsail_media::Error::TooLarge.into())
}

#[derive(Default)]
pub struct InputSink {
    pressed: HashSet<u16>,
    left: bool,
    right: bool,
    pending_unicode: Option<u16>,
}
fn key(vk: u16, down: bool) -> INPUT {
    let mut flags = if down {
        Default::default()
    } else {
        KEYEVENTF_KEYUP
    };
    if matches!(vk, 0x21..=0x28 | 0x2d..=0x2e | 0x6f | 0xa3 | 0xa5) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}
fn unicode(unit: u16, down: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wScan: unit,
                dwFlags: if down {
                    KEYEVENTF_UNICODE
                } else {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                },
                ..Default::default()
            },
        },
    }
}
fn mouse(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    data: i32,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                mouseData: data as u32,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}
fn send(inputs: &[INPUT]) -> Result<()> {
    let (inserted, code) = send_raw(inputs);
    let expected = inputs.len() as u32;
    if inserted != expected {
        return Err(Error::Injection {
            inserted,
            expected,
            code,
        });
    }
    Ok(())
}
fn send_raw(inputs: &[INPUT]) -> (u32, u32) {
    #[cfg(test)]
    if let Some(result) = test_injection::record(inputs) {
        return result;
    }
    #[cfg(test)]
    if !test_injection::owned_input_allowed(inputs) {
        return (0, 0);
    }
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let inserted = SendInput(inputs, size_of::<INPUT>() as i32);
        (inserted, GetLastError().0)
    }
}
// Thread-local test recorder: ordinary regressions never inject into the user's desktop.
#[cfg(test)]
mod test_injection {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};
    #[derive(Default)]
    struct Recorder {
        flags: Vec<u32>,
        activations: Vec<(i32, i32)>,
        results: VecDeque<(u32, u32)>,
    }
    thread_local! { static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) }; }
    struct OwnedSafety {
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        windows: [usize; 2],
    }
    thread_local! { static OWNED: RefCell<Option<OwnedSafety>> = const { RefCell::new(None) }; }
    pub struct OwnedGuard;
    impl Drop for OwnedGuard {
        fn drop(&mut self) {
            OWNED.with(|owned| {
                owned.borrow_mut().take();
            });
        }
    }
    pub fn guard_owned(
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        windows: [usize; 2],
    ) -> OwnedGuard {
        OWNED.with(|owned| {
            assert!(owned.borrow().is_none());
            *owned.borrow_mut() = Some(OwnedSafety { cancelled, windows });
        });
        OwnedGuard
    }
    pub fn update_owned_windows(windows: [usize; 2]) {
        OWNED.with(|owned| {
            owned
                .borrow_mut()
                .as_mut()
                .expect("owned guard must be active")
                .windows = windows;
        });
    }
    fn owned_window(window: windows::Win32::Foundation::HWND, safety: &OwnedSafety) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(window, Some(&mut owner)) };
        safety.windows.contains(&(window.0 as usize)) && owner == std::process::id()
    }
    pub fn owned_activation_allowed(point: (i32, i32)) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GA_ROOT, GetAncestor, GetForegroundWindow, WindowFromPoint,
        };
        OWNED.with(|owned| {
            let owned = owned.borrow();
            let Some(safety) = owned.as_ref() else {
                return true;
            };
            !safety.cancelled.load(std::sync::atomic::Ordering::Acquire)
                && owned_window(unsafe { GetForegroundWindow() }, safety)
                && owned_window(
                    unsafe {
                        GetAncestor(
                            WindowFromPoint(windows::Win32::Foundation::POINT {
                                x: point.0,
                                y: point.1,
                            }),
                            GA_ROOT,
                        )
                    },
                    safety,
                )
        })
    }
    pub fn owned_input_allowed(inputs: &[INPUT]) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        OWNED.with(|owned| {
            let owned = owned.borrow();
            let Some(safety) = owned.as_ref() else {
                return true;
            };
            if !owned_window(unsafe { GetForegroundWindow() }, safety) {
                return false;
            }
            // Once the deadline expires no later move/down/activation is allowed.
            // Drop may still release owned keys/buttons while an owned window is
            // foreground; after destruction or external focus even cleanup skips.
            !safety.cancelled.load(std::sync::atomic::Ordering::Acquire)
                || inputs.iter().all(|input| unsafe {
                    match input.r#type {
                        INPUT_MOUSE => matches!(
                            input.Anonymous.mi.dwFlags,
                            MOUSEEVENTF_LEFTUP | MOUSEEVENTF_RIGHTUP
                        ),
                        INPUT_KEYBOARD => input.Anonymous.ki.dwFlags.0 & KEYEVENTF_KEYUP.0 != 0,
                        _ => false,
                    }
                })
        })
    }
    pub struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RECORDER.with(|r| {
                r.borrow_mut().take();
            });
        }
    }
    pub fn start(results: impl IntoIterator<Item = (u32, u32)>) -> Guard {
        RECORDER.with(|r| {
            assert!(r.borrow().is_none());
            *r.borrow_mut() = Some(Recorder {
                flags: vec![],
                activations: vec![],
                results: results.into_iter().collect(),
            });
        });
        Guard
    }
    pub fn flags() -> Vec<u32> {
        RECORDER.with(|r| r.borrow().as_ref().unwrap().flags.clone())
    }
    pub fn activations() -> Vec<(i32, i32)> {
        RECORDER.with(|r| r.borrow().as_ref().unwrap().activations.clone())
    }
    pub fn record_activation(point: (i32, i32)) -> bool {
        RECORDER.with(|r| {
            let mut r = r.borrow_mut();
            let Some(recorder) = r.as_mut() else {
                return false;
            };
            recorder.activations.push(point);
            true
        })
    }
    pub fn record(inputs: &[INPUT]) -> Option<(u32, u32)> {
        RECORDER.with(|r| {
            let mut r = r.borrow_mut();
            let recorder = r.as_mut()?;
            for input in inputs {
                if input.r#type == INPUT_MOUSE {
                    recorder.flags.push(unsafe { input.Anonymous.mi.dwFlags.0 });
                }
            }
            Some(
                recorder
                    .results
                    .pop_front()
                    .unwrap_or((inputs.len() as u32, 0)),
            )
        })
    }
}
fn absolute_axis(pixel: i32, origin: i32, extent: i32) -> Result<i32> {
    let offset = i64::from(pixel) - i64::from(origin);
    if extent <= 0 || offset < 0 || offset >= i64::from(extent) {
        return Err(Error::Geometry);
    }
    // Target the pixel centre. Windows maps absolute 16-bit coordinates over the virtual desktop.
    Ok(((offset * 2 + 1) * 65536 / (i64::from(extent) * 2)).min(65535) as i32)
}
fn rotated_point(x: usize, y: usize, width: usize, height: usize, rotation: u32) -> (usize, usize) {
    match rotation {
        2 => (y, height - 1 - x),
        3 => (width - 1 - x, height - 1 - y),
        4 => (width - 1 - y, x),
        _ => (x, y),
    }
}
fn convert_bgra(
    raw: &[u8],
    source_width: usize,
    source_height: usize,
    pitch: usize,
    rotation: u32,
    out_w: usize,
    out_h: usize,
) -> Result<Vec<u8>> {
    let (logical_w, logical_h) = if rotation == 2 || rotation == 4 {
        (source_height, source_width)
    } else {
        (source_width, source_height)
    };
    if out_w == 0
        || out_h == 0
        || out_w > logical_w
        || out_h > logical_h
        || pitch < source_width.checked_mul(4).ok_or(Error::Geometry)?
        || raw.len() < pitch.checked_mul(source_height).ok_or(Error::Geometry)?
    {
        return Err(Error::Geometry);
    }
    let mut rgb = vec![0u8; out_w * out_h * 3];
    for y in 0..out_h {
        for x in 0..out_w {
            let lx = x * logical_w / out_w;
            let ly = y * logical_h / out_h;
            let (sx, sy) = rotated_point(lx, ly, source_width, source_height, rotation);
            if sx >= source_width || sy >= source_height {
                return Err(Error::Geometry);
            }
            let p = &raw[sy * pitch + sx * 4..][..4];
            rgb[(y * out_w + x) * 3..][..3].copy_from_slice(&[p[2], p[1], p[0]]);
        }
    }
    Ok(rgb)
}
fn allowed_key(vk: u16) -> bool {
    matches!(vk, 0x08..=0x0d | 0x10..=0x14 | 0x1b | 0x20..=0x28 | 0x2c..=0x2e | 0x30..=0x5d | 0x60..=0x87 | 0x90..=0x91 | 0xa0..=0xb7 | 0xba..=0xc0 | 0xdb..=0xde | 0xe2)
}
impl InputSink {
    pub fn apply(&mut self, input: Input, display: Option<&Display>, layout: u64) -> Result<()> {
        match input {
            Input::Move {
                display: id,
                layout: expected,
                x,
                y,
            } => move_to(display, layout, id, expected, x, y),
            Input::Button {
                display: id,
                layout: expected,
                x,
                y,
                button,
                down,
            } => {
                let activate =
                    super::input_context::click_activation::fresh_down(down, self.left, self.right);
                let (slot, down_flag, up_flag) = match button {
                    Button::Left => (&mut self.left, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                    Button::Right => (&mut self.right, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                };
                // A release is owed only for this session's own injected button.
                // A stale/missing layout cannot prevent it or reposition the cursor.
                if !down {
                    if !*slot {
                        return Ok(());
                    }
                    let positioned = move_to(display, layout, id, expected, x, y);
                    send(&[mouse(up_flag, 0)])?;
                    *slot = false;
                    return match positioned {
                        Err(Error::Geometry) => Ok(()),
                        other => other,
                    };
                }
                move_to(display, layout, id, expected, x, y)?;
                if *slot == down {
                    return Ok(());
                }
                if activate {
                    let point = map_point(display.ok_or(Error::Geometry)?, x, y)?;
                    activate_at(point);
                }
                send(&[mouse(if down { down_flag } else { up_flag }, 0)])?;
                *slot = down;
                Ok(())
            }
            Input::Wheel {
                display: id,
                layout: expected,
                x,
                y,
                vertical,
                horizontal,
            } => {
                if vertical.unsigned_abs() > 1200 || horizontal.unsigned_abs() > 1200 {
                    return Err(Error::UnsupportedInput);
                }
                move_to(display, layout, id, expected, x, y)?;
                if vertical != 0 {
                    send(&[mouse(MOUSEEVENTF_WHEEL, vertical)])?;
                }
                if horizontal != 0 {
                    send(&[mouse(MOUSEEVENTF_HWHEEL, horizontal)])?;
                }
                Ok(())
            }
            Input::Key { vk, down, repeat } => {
                if !allowed_key(vk) {
                    return Err(Error::UnsupportedInput);
                }
                if self.pressed.contains(&vk) == down && !(down && repeat) {
                    return Ok(());
                }
                send(&[key(vk, down)])?;
                if down {
                    self.pressed.insert(vk);
                } else {
                    self.pressed.remove(&vk);
                }
                Ok(())
            }
            Input::Text { text } => {
                if text.chars().count() > 64 || text.chars().any(|c| c.is_control()) {
                    return Err(Error::UnsupportedInput);
                }
                for unit in text.encode_utf16() {
                    let pair = [unicode(unit, true), unicode(unit, false)];
                    let (count, code) = send_raw(&pair);
                    if count != 2 {
                        if count == 1 {
                            self.pending_unicode = Some(unit);
                            if send(&[unicode(unit, false)]).is_ok() {
                                self.pending_unicode = None;
                            }
                        }
                        return Err(Error::Injection {
                            inserted: count,
                            expected: 2,
                            code,
                        });
                    }
                }
                Ok(())
            }
        }
    }
    pub fn release_all(&mut self) -> Result<()> {
        let mut failed = false;
        for _ in 0..3 {
            if let Some(unit) = self.pending_unicode {
                if send(&[unicode(unit, false)]).is_ok() {
                    self.pending_unicode = None;
                } else {
                    failed = true;
                }
            }
            let keys: Vec<_> = self.pressed.iter().copied().collect();
            for vk in keys {
                if send(&[key(vk, false)]).is_ok() {
                    self.pressed.remove(&vk);
                } else {
                    failed = true;
                }
            }
            if self.left {
                if send(&[mouse(MOUSEEVENTF_LEFTUP, 0)]).is_ok() {
                    self.left = false;
                } else {
                    failed = true;
                }
            }
            if self.right {
                if send(&[mouse(MOUSEEVENTF_RIGHTUP, 0)]).is_ok() {
                    self.right = false;
                } else {
                    failed = true;
                }
            }
            if self.pressed.is_empty()
                && !self.left
                && !self.right
                && self.pending_unicode.is_none()
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if failed {
            Err(Error::InputDenied)
        } else {
            Ok(())
        }
    }
}
impl Drop for InputSink {
    fn drop(&mut self) {
        let _ = self.release_all();
    }
}
fn activate_at(point: (i32, i32)) {
    // Fake SendInput tests must also skip every real foreground side effect.
    #[cfg(test)]
    if test_injection::record_activation(point) {
        return;
    }
    #[cfg(test)]
    if !test_injection::owned_activation_allowed(point) {
        return;
    }
    super::input_context::try_activate_at(point.0, point.1);
}
fn move_to(
    display: Option<&Display>,
    layout: u64,
    id: u32,
    expected: u64,
    x: f64,
    y: f64,
) -> Result<()> {
    let d = display
        .filter(|d| d.id == id && layout == expected)
        .ok_or(Error::Geometry)?;
    let (px, py) = map_point(d, x, y)?;
    let mut event = mouse(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
    );
    unsafe {
        event.Anonymous.mi.dx = absolute_axis(
            px,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
        )?;
        event.Anonymous.mi.dy = absolute_axis(
            py,
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )?;
    }
    send(&[event])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pointer_display() -> Display {
        Display {
            id: 1,
            name: "test pointer origin".into(),
            x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
            y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
            width: 1,
            height: 1,
            dpi: 96,
            rotation: 1,
        }
    }
    fn pointer_button(button: Button, down: bool) -> Input {
        Input::Button {
            display: 1,
            layout: 7,
            x: 0.0,
            y: 0.0,
            button,
            down,
        }
    }
    fn stale_up(button: Button) -> Input {
        Input::Button {
            display: 1,
            layout: 7,
            x: 0.5,
            y: 0.5,
            button,
            down: false,
        }
    }
    #[test]
    fn only_fresh_pointer_down_attempts_activation_and_fake_input_never_activates_windows() {
        let _recording = test_injection::start([]);
        let display = pointer_display();
        let mut sink = InputSink::default();
        sink.apply(pointer_button(Button::Left, true), Some(&display), 7)
            .unwrap();
        assert_eq!(test_injection::activations(), [(display.x, display.y)]);
        sink.apply(pointer_button(Button::Left, true), Some(&display), 7)
            .unwrap();
        sink.apply(pointer_button(Button::Right, true), Some(&display), 7)
            .unwrap();
        sink.apply(
            Input::Move {
                display: 1,
                layout: 7,
                x: 0.0,
                y: 0.0,
            },
            Some(&display),
            7,
        )
        .unwrap();
        sink.apply(pointer_button(Button::Left, false), Some(&display), 7)
            .unwrap();
        sink.apply(pointer_button(Button::Right, false), Some(&display), 7)
            .unwrap();
        assert_eq!(test_injection::activations(), [(display.x, display.y)]);
        assert!(!sink.left && !sink.right);
        sink.apply(pointer_button(Button::Right, true), Some(&display), 7)
            .unwrap();
        assert_eq!(test_injection::activations().len(), 2);
        sink.release_all().unwrap();
    }

    #[test]
    fn invalid_layout_or_position_cannot_attempt_pointer_activation() {
        let _recording = test_injection::start([]);
        let display = pointer_display();
        let mut sink = InputSink::default();
        assert!(matches!(
            sink.apply(pointer_button(Button::Left, true), Some(&display), 8),
            Err(Error::Geometry)
        ));
        assert!(matches!(
            sink.apply(pointer_button(Button::Left, true), None, 7),
            Err(Error::Geometry)
        ));
        assert!(matches!(
            sink.apply(
                Input::Button {
                    display: 1,
                    layout: 7,
                    x: f64::NAN,
                    y: 0.0,
                    button: Button::Left,
                    down: true,
                },
                Some(&display),
                7,
            ),
            Err(Error::Geometry)
        ));
        assert!(test_injection::activations().is_empty());
        assert!(test_injection::flags().is_empty());
        assert!(!sink.left);
    }

    #[test]
    fn failed_pointer_move_does_not_activate_and_button_failure_retains_diagnostics() {
        let display = pointer_display();
        for (results, activation_count) in [(vec![(0, 5)], 0), (vec![(1, 0), (0, 5)], 1)] {
            let _recording = test_injection::start(results);
            let mut sink = InputSink::default();
            assert!(matches!(
                sink.apply(pointer_button(Button::Left, true), Some(&display), 7),
                Err(Error::Injection {
                    inserted: 0,
                    expected: 1,
                    code: 5,
                })
            ));
            assert_eq!(test_injection::activations().len(), activation_count);
            assert!(!sink.left);
        }
    }
    #[test]
    fn owned_mouse_up_after_layout_switch_releases_without_moving_cursor() {
        let _recording = test_injection::start([]);
        let mut sink = InputSink::default();
        sink.left = true;
        sink.right = true;
        sink.apply(stale_up(Button::Left), None, 8).unwrap();
        sink.apply(stale_up(Button::Right), None, 8).unwrap();
        assert!(!sink.left && !sink.right);
        assert_eq!(
            test_injection::flags(),
            vec![MOUSEEVENTF_LEFTUP.0, MOUSEEVENTF_RIGHTUP.0]
        );
    }
    #[test]
    fn unowned_mouse_up_does_not_inject_or_move() {
        let _recording = test_injection::start([]);
        let mut sink = InputSink::default();
        sink.apply(stale_up(Button::Left), None, 8).unwrap();
        assert!(test_injection::flags().is_empty());
    }
    #[test]
    fn rejected_stale_release_remains_owned_until_cleanup_succeeds() {
        let _recording = test_injection::start([(0, 5)]);
        let mut sink = InputSink::default();
        sink.left = true;
        assert!(matches!(
            sink.apply(stale_up(Button::Left), None, 8),
            Err(Error::Injection {
                inserted: 0,
                expected: 1,
                code: 5
            })
        ));
        assert!(sink.left);
        sink.release_all().unwrap();
        assert!(!sink.left);
        assert_eq!(
            test_injection::flags(),
            vec![MOUSEEVENTF_LEFTUP.0, MOUSEEVENTF_LEFTUP.0]
        );
    }
    #[test]
    fn noisy_4k_compresses_within_limit_and_legacy_hd_stays_compatible() {
        let mut seed = 17u32;
        for (width, height, profile, limit) in [
            (1920, 1080, 1, 1_000_000),
            (3840, 2160, 3, farsail_media::MAX_BYTES),
        ] {
            let rgb: Vec<u8> = (0..width * height * 3)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
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
            let frame = encode_profile(meta.clone(), &rgb, profile).unwrap();
            assert_eq!(frame.meta, meta);
            assert!(frame.jpeg.len() <= limit);
            frame.validate().unwrap();
        }
    }
    #[test]
    fn hd_profile_preserves_native_1080p_and_handles_portrait_without_upscaling() {
        assert_eq!(output_size(1920, 1080, 0).unwrap(), (1280, 720));
        assert_eq!(output_size(1920, 1080, 1).unwrap(), (1920, 1080));
        assert_eq!(output_size(1080, 1920, 1).unwrap(), (1080, 1920));
        assert_eq!(output_size(3840, 2160, 1).unwrap(), (1920, 1080));
        assert_eq!(output_size(800, 600, 1).unwrap(), (800, 600));
        assert!(output_size(0, 1080, 1).is_err());
        assert_eq!(output_size(3840, 2160, 2).unwrap(), (2560, 1440));
        assert_eq!(output_size(3840, 2160, 3).unwrap(), (3840, 2160));
        assert_eq!(output_size(2160, 3840, 3).unwrap(), (2160, 3840));
        assert_eq!(output_size(1920, 1080, 3).unwrap(), (1920, 1080));
        assert!(output_size(1920, 1080, 4).is_err());
    }
    #[test]
    fn absolute_mouse_coordinates_cover_negative_monitor_and_pixel_centres() {
        for (origin, extent) in [(-1920, 3840), (0, 1920), (-1080, 2160), (0, 1)] {
            for offset in [0, extent / 2, extent - 1] {
                let unit = absolute_axis(origin + offset, origin, extent).unwrap();
                assert_eq!(
                    i64::from(unit) * i64::from(extent) / 65536,
                    i64::from(offset)
                );
            }
        }
        assert!(absolute_axis(-1, 0, 1920).is_err());
        assert!(absolute_axis(1920, 0, 1920).is_err());
        assert!(absolute_axis(0, 0, 0).is_err());
    }
    #[test]
    fn ordinary_system_keys_are_supported_and_unknown_input_is_nonfatal() {
        for key in [0x14, 0x5b, 0x5c, 0x5d, 0x90, 0x91, 0x2c, 0x7c, 0x87] {
            assert!(allowed_key(key));
        }
        let mut sink = InputSink::default();
        assert!(matches!(
            sink.apply(
                Input::Key {
                    vk: 0,
                    down: true,
                    repeat: false
                },
                None,
                0
            ),
            Err(Error::UnsupportedInput)
        ));
        assert!(sink.pressed.is_empty());
    }
    #[test]
    fn rotated_rectangular_corners_and_stride() {
        let w = 3;
        let h = 2;
        for rotation in 1..=4 {
            let (rw, rh) = if rotation == 2 || rotation == 4 {
                (h, w)
            } else {
                (w, h)
            };
            let mut seen = HashSet::new();
            for y in 0..rh {
                for x in 0..rw {
                    let (sx, sy) = rotated_point(x, y, w, h, rotation);
                    assert!(sx < w && sy < h);
                    assert!(seen.insert((sx, sy)));
                }
            }
            assert_eq!(seen.len(), w * h);
        }
        assert_eq!(rotated_point(0, 0, w, h, 2), (0, 1));
        assert_eq!(rotated_point(0, 0, w, h, 3), (2, 1));
        assert_eq!(rotated_point(0, 0, w, h, 4), (2, 0));
        let mut padded = vec![255u8; 2 * 16];
        for y in 0..2 {
            for x in 0..3 {
                padded[y * 16 + x * 4 + 2] = (y * 3 + x + 1) as u8;
            }
        }
        let channels = |rotation, w, h| {
            convert_bgra(&padded, 3, 2, 16, rotation, w, h)
                .unwrap()
                .chunks_exact(3)
                .map(|p| p[0])
                .collect::<Vec<_>>()
        };
        assert_eq!(channels(1, 3, 2), vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(channels(2, 2, 3), vec![4, 1, 5, 2, 6, 3]);
        assert_eq!(channels(3, 3, 2), vec![6, 5, 4, 3, 2, 1]);
        assert_eq!(channels(4, 2, 3), vec![3, 6, 2, 5, 1, 4]);
    }
    #[test]
    fn wheel_min_is_rejected_before_injection() {
        let mut sink = InputSink::default();
        assert!(
            sink.apply(
                Input::Wheel {
                    display: 1,
                    layout: 1,
                    x: 0.0,
                    y: 0.0,
                    vertical: i32::MIN,
                    horizontal: 0
                },
                None,
                1
            )
            .is_err()
        );
    }
    #[test]
    fn stale_button_layout_is_rejected_before_system_input() {
        let display = Display {
            id: 1,
            name: "fixture".into(),
            x: -100,
            y: 0,
            width: 100,
            height: 100,
            dpi: 96,
            rotation: 1,
        };
        let mut sink = InputSink::default();
        assert!(matches!(
            sink.apply(
                Input::Button {
                    display: 1,
                    layout: 7,
                    x: 0.5,
                    y: 0.5,
                    button: Button::Left,
                    down: true
                },
                Some(&display),
                8
            ),
            Err(Error::Geometry)
        ));
        assert!(!sink.left);
    }
    #[test]
    fn extended_navigation_and_right_modifiers() {
        for vk in [0x25, 0x2d, 0x2e, 0xa3, 0xa5] {
            assert!(unsafe { key(vk, true).Anonymous.ki.dwFlags }.contains(KEYEVENTF_EXTENDEDKEY));
        }
        assert!(!unsafe { key(0x41, true).Anonymous.ki.dwFlags }.contains(KEYEVENTF_EXTENDEDKEY));
    }
    #[test]
    #[ignore = "requires an interactive Windows desktop; no screenshot is written"]
    fn real_dxgi_frame_stays_in_memory() {
        ensure_dpi_awareness().unwrap();
        let list = displays().unwrap();
        assert!(!list.is_empty());
        for display in &list {
            assert!(
                Capture::new(display.id).is_ok(),
                "DXGI unavailable for display {}",
                display.id
            );
        }
        let mut capture = Capture::new(list[0].id).unwrap();
        let start = std::time::Instant::now();
        loop {
            if let Some(frame) = capture.next_frame(1, 1).unwrap() {
                let wire = frame.to_wire().unwrap();
                let decoded = JpegFrame::from_wire(&wire).unwrap();
                assert_eq!(
                    decoded.decode_rgb().unwrap().len(),
                    (decoded.meta.width * decoded.meta.height * 3) as usize
                );
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(5),
                "DXGI produced no frame"
            );
        }
    }
    #[test]
    #[ignore = "injects harmless text only into a verified foreground window created by this test"]
    fn real_input_into_own_foreground_window() {
        ensure_dpi_awareness().unwrap();
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, SetFocus,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GWLP_WNDPROC, GetForegroundWindow, GetWindowInfo, GetWindowLongPtrW, GetWindowTextW,
            GetWindowThreadProcessId, HTCAPTION, HWND_TOPMOST, IsWindowVisible, MSG, PM_REMOVE,
            PeekMessageW, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow,
            SetWindowLongPtrW, SetWindowPos, TranslateMessage, WINDOWINFO, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WNDPROC, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE, WindowFromPoint,
        };
        use windows::core::w;
        #[derive(Clone, Copy, Default, Debug)]
        struct ClickCounts {
            client_down: u32,
            client_up: u32,
            caption_down: u32,
            caption_up: u32,
        }
        struct WindowProbe {
            original: WNDPROC,
            clicks: ClickCounts,
        }
        thread_local! {
            static PROBES: std::cell::RefCell<std::collections::HashMap<usize, WindowProbe>>
                = std::cell::RefCell::new(std::collections::HashMap::new());
        }
        unsafe extern "system" fn probe_window(
            hwnd: windows::Win32::Foundation::HWND,
            message: u32,
            wparam: windows::Win32::Foundation::WPARAM,
            lparam: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            let original = PROBES.with(|probes| {
                let mut probes = probes.borrow_mut();
                let probe = probes.get_mut(&(hwnd.0 as usize))?;
                match message {
                    WM_LBUTTONDOWN => probe.clicks.client_down += 1,
                    WM_LBUTTONUP => probe.clicks.client_up += 1,
                    WM_NCLBUTTONDOWN if wparam.0 == HTCAPTION as usize => {
                        probe.clicks.caption_down += 1
                    }
                    WM_NCLBUTTONUP if wparam.0 == HTCAPTION as usize => {
                        probe.clicks.caption_up += 1
                    }
                    _ => {}
                }
                Some(probe.original)
            });
            // Count a real non-client click without entering the system title-bar
            // move modal loop. Client dragging retains the actual EDIT procedure.
            if matches!(message, WM_NCLBUTTONDOWN | WM_NCLBUTTONUP)
                && wparam.0 == HTCAPTION as usize
            {
                return windows::Win32::Foundation::LRESULT(0);
            }
            match original {
                Some(original) => unsafe {
                    CallWindowProcW(original, hwnd, message, wparam, lparam)
                },
                None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
            }
        }
        fn click_counts(hwnd: windows::Win32::Foundation::HWND) -> ClickCounts {
            PROBES.with(|probes| probes.borrow().get(&(hwnd.0 as usize)).unwrap().clicks)
        }
        let display = displays().unwrap().remove(0);
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                display.x + 100,
                display.y + 100,
                400,
                200,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        struct TestWindow(windows::Win32::Foundation::HWND);
        impl TestWindow {
            fn new(hwnd: windows::Win32::Foundation::HWND) -> Self {
                let window = Self(hwnd);
                let original = unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) };
                assert_ne!(original, 0, "owned window has no procedure to subclass");
                PROBES.with(|probes| {
                    probes.borrow_mut().insert(
                        hwnd.0 as usize,
                        WindowProbe {
                            original: unsafe { std::mem::transmute::<isize, WNDPROC>(original) },
                            clicks: ClickCounts::default(),
                        },
                    );
                });
                assert_eq!(
                    unsafe {
                        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, probe_window as *const () as isize)
                    },
                    original
                );
                window
            }
        }
        impl Drop for TestWindow {
            fn drop(&mut self) {
                if let Some(probe) =
                    PROBES.with(|probes| probes.borrow_mut().remove(&(self.0.0 as usize)))
                {
                    let original = probe.original.unwrap() as *const () as isize;
                    let _ = unsafe { SetWindowLongPtrW(self.0, GWLP_WNDPROC, original) };
                }
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let _window = TestWindow::new(hwnd);
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            )
        };
        let _ = unsafe { SetForegroundWindow(hwnd) };
        let _ = unsafe { SetFocus(Some(hwnd)) };
        if unsafe { GetForegroundWindow() } != hwnd {
            let foreign = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
            let own = unsafe { GetCurrentThreadId() };
            if foreign != 0
                && foreign != own
                && unsafe { AttachThreadInput(own, foreign, true) }.as_bool()
            {
                let _ = unsafe { SetForegroundWindow(hwnd) };
                let _ = unsafe { SetFocus(Some(hwnd)) };
                let _ = unsafe { AttachThreadInput(own, foreign, false) };
            }
        }
        for _ in 0..20 {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg)
                };
            }
            if unsafe { GetForegroundWindow() } == hwnd {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        assert_eq!(
            unsafe { GetForegroundWindow() },
            hwnd,
            "foreground is not the controlled test window; no input injected"
        );
        assert_eq!(
            pid,
            std::process::id(),
            "foreground belongs to another process; no input injected"
        );
        // Declare before the sink: its panic/drop cleanup must still refuse an
        // external foreground even after a worker returns held state to main.
        let _main_owned_guard = test_injection::guard_owned(
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            [hwnd.0 as usize, hwnd.0 as usize],
        );
        let mut sink = InputSink::default();
        sink.apply(Input::Text { text: "FS".into() }, None, 0)
            .unwrap();
        for _ in 0..20 {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg)
                };
            }
            let mut buffer = [0u16; 32];
            let n = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
            if String::from_utf16_lossy(&buffer[..n]) == "FS" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut buffer = [0u16; 32];
        let n = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
        assert_eq!(String::from_utf16_lossy(&buffer[..n]), "FS");
        assert_eq!(
            unsafe { GetForegroundWindow() },
            hwnd,
            "foreground changed before keyboard combo; no combo injected"
        );
        for (vk, down) in [(0xa0, true), (0x24, true), (0x24, false), (0xa0, false)] {
            sink.apply(
                Input::Key {
                    vk,
                    down,
                    repeat: false,
                },
                None,
                0,
            )
            .unwrap();
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg)
                };
            }
            if vk == 0xa0 && down {
                assert_ne!(
                    unsafe { GetAsyncKeyState(0x10) } as u16 & 0x8000,
                    0,
                    "injected Shift key was not held"
                );
            }
        }
        sink.apply(Input::Text { text: "Q".into() }, None, 0)
            .unwrap();
        for _ in 0..20 {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg)
                };
            }
            let mut buffer = [0u16; 32];
            let n = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
            if String::from_utf16_lossy(&buffer[..n]) == "Q" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut buffer = [0u16; 32];
        let n = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
        assert_eq!(String::from_utf16_lossy(&buffer[..n]), "Q");
        assert_eq!(
            unsafe { GetForegroundWindow() },
            hwnd,
            "foreground changed before mouse input; no click injected"
        );
        let x = (150.0 / display.width as f64).min(0.99);
        let y = (150.0 / display.height as f64).min(0.99);
        // Move only inside the verified test window, on a production-shaped GUI-less worker.
        let worker_display = display.clone();
        let worker_point = map_point(&display, x, y).unwrap();
        std::thread::spawn(move || {
            use windows::Win32::UI::WindowsAndMessaging::{
                GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
            };
            let mut owner = 0;
            unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut owner)) };
            assert_eq!(
                owner,
                std::process::id(),
                "foreground changed; no worker input injected"
            );
            let mut worker = InputSink::default();
            worker
                .apply(
                    Input::Move {
                        display: worker_display.id,
                        layout: 7,
                        x,
                        y,
                    },
                    Some(&worker_display),
                    7,
                )
                .unwrap();
            let mut actual = windows::Win32::Foundation::POINT::default();
            unsafe { GetCursorPos(&mut actual) }.unwrap();
            assert!(
                (actual.x - worker_point.0).abs() <= 1 && (actual.y - worker_point.1).abs() <= 1
            );
        })
        .join()
        .unwrap();
        sink.apply(
            Input::Button {
                display: display.id,
                layout: 7,
                x,
                y,
                button: Button::Left,
                down: true,
            },
            Some(&display),
            7,
        )
        .unwrap();
        sink.apply(
            Input::Button {
                display: display.id,
                layout: 7,
                x,
                y,
                button: Button::Left,
                down: false,
            },
            Some(&display),
            7,
        )
        .unwrap();
        sink.release_all().unwrap();
        // Continue controlling after repeated clicks, including UP after a new layout.
        for n in 0..20 {
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
            assert_eq!(
                pid,
                std::process::id(),
                "foreground changed; no click injected"
            );
            assert_eq!(unsafe { GetForegroundWindow() }, hwnd);
            sink.apply(
                Input::Button {
                    display: display.id,
                    layout: 7,
                    x,
                    y,
                    button: Button::Left,
                    down: true,
                },
                Some(&display),
                7,
            )
            .unwrap();
            sink.apply(
                Input::Button {
                    display: display.id,
                    layout: 7,
                    x,
                    y,
                    button: Button::Left,
                    down: false,
                },
                if n % 2 == 0 { None } else { Some(&display) },
                if n % 2 == 0 { 8 } else { 7 },
            )
            .unwrap();
            for _ in 0..4 {
                let mut msg = MSG::default();
                while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(!sink.left);
            assert_eq!(
                unsafe { GetAsyncKeyState(1) } & i16::MIN,
                0,
                "injected mouse button remained down"
            );
        }
        let second = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                display.x + 260,
                display.y + 100,
                300,
                200,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        let _second_window = TestWindow::new(second);
        test_injection::update_owned_windows([hwnd.0 as usize, second.0 as usize]);
        struct CaptureCleanup;
        impl Drop for CaptureCleanup {
            fn drop(&mut self) {
                let _ = unsafe { ReleaseCapture() };
            }
        }
        let _capture_cleanup = CaptureCleanup;
        assert!(
            display.width >= 600 && display.height >= 350,
            "display lacks room for owned overlapping windows; no further input injected"
        );
        // CreateWindow may have activated the new window. Establish the initial
        // front window once, then only injected clicks may switch foreground.
        let _ = unsafe {
            SetWindowPos(
                second,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            )
        };
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            )
        };
        let _ = unsafe { SetForegroundWindow(hwnd) };
        let _ = unsafe { SetFocus(Some(hwnd)) };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        fn pump_until(deadline: std::time::Instant, ready: impl FnMut() -> bool, failure: &str) {
            assert!(pump_until_check(deadline, ready), "{failure}");
        }
        fn pump_until_check(deadline: std::time::Instant, mut ready: impl FnMut() -> bool) -> bool {
            loop {
                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    if std::time::Instant::now() >= deadline {
                        return false;
                    }
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                if ready() {
                    return true;
                }
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        fn owned_pointer(
            deadline: std::time::Instant,
            sink: &mut InputSink,
            display: &Display,
            expected: windows::Win32::Foundation::HWND,
            x: f64,
            y: f64,
            input: Input,
        ) {
            owned_pointer_batch(deadline, sink, display, expected, x, y, vec![input]);
        }
        fn owned_pointer_batch(
            deadline: std::time::Instant,
            sink: &mut InputSink,
            display: &Display,
            expected: windows::Win32::Foundation::HWND,
            x: f64,
            y: f64,
            inputs: Vec<Input>,
        ) {
            let expected = expected.0 as usize;
            let display = display.clone();
            let point = map_point(&display, x, y).unwrap();
            struct CancelWorker(std::sync::Arc<std::sync::atomic::AtomicBool>);
            impl Drop for CancelWorker {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let _cancel_worker = CancelWorker(cancelled.clone());
            let allowed = [expected, unsafe { GetForegroundWindow() }.0 as usize];
            // The injection worker owns no GUI window, like the host input
            // task. Check hit-test ownership immediately before every event.
            let (reply, response) = std::sync::mpsc::sync_channel(1);
            let worker_sink = std::mem::take(sink);
            let worker = std::thread::spawn(move || {
                let _owned_guard = test_injection::guard_owned(cancelled, allowed);
                // Declare the sink after the guard so panic/drop cleanup still
                // passes through its cancellation and owned-foreground checks.
                let mut worker_sink = worker_sink;
                let hit = unsafe {
                    WindowFromPoint(windows::Win32::Foundation::POINT {
                        x: point.0,
                        y: point.1,
                    })
                };
                assert_eq!(
                    hit.0 as usize, expected,
                    "pointer no longer hits the visible owned target; no input injected"
                );
                assert!(
                    unsafe { IsWindowVisible(hit) }.as_bool(),
                    "target is no longer visible; no input injected"
                );
                let mut owner = 0;
                unsafe { GetWindowThreadProcessId(hit, Some(&mut owner)) };
                assert_eq!(
                    owner,
                    std::process::id(),
                    "hit window belongs to another process; no input injected"
                );
                owner = 0;
                unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut owner)) };
                assert_eq!(
                    owner,
                    std::process::id(),
                    "foreground belongs to another process; no input injected"
                );
                let result = inputs
                    .into_iter()
                    .try_for_each(|input| worker_sink.apply(input, Some(&display), 7));
                let _ = reply.send((worker_sink, result));
            });
            let mut completed = None;
            // Cross-thread foreground activation requires the target GUI thread
            // to process messages. Do not join the worker before pumping it.
            pump_until(
                deadline,
                || match response.try_recv() {
                    Ok(result) => {
                        completed = Some(result);
                        true
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => false,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        panic!("owned pointer worker failed before replying")
                    }
                },
                "owned pointer worker exceeded the bounded message-pump deadline",
            );
            let (returned_sink, result) = completed.unwrap();
            *sink = returned_sink;
            worker.join().unwrap();
            result.unwrap();
        }
        let ny = 150.0 / (display.height - 1) as f64;
        pump_until(
            deadline,
            || unsafe { GetForegroundWindow() } == hwnd,
            "could not establish owned initial foreground; no click injected",
        );
        // The first window spans x=100..500 and the second x=260..560.
        // x=530 exposes the rear second window; x=150 exposes the rear first.
        let mut info = WINDOWINFO {
            cbSize: size_of::<WINDOWINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowInfo(hwnd, &mut info) }.unwrap();
        assert!(info.rcClient.top > info.rcWindow.top);
        // Use the middle of the non-client top area, away from the icon and
        // window buttons, to test the rear window's exposed title bar safely.
        let caption_y = ((f64::from(info.rcWindow.top) + f64::from(info.rcClient.top)) / 2.0
            - f64::from(display.y))
            / f64::from(display.height - 1);
        for (index, (target, px, y)) in [
            (second, 530.0, ny),
            (hwnd, 150.0, ny),
            (second, 530.0, ny),
            (hwnd, 150.0, ny),
            (second, 530.0, ny),
            (hwnd, 220.0, caption_y),
        ]
        .into_iter()
        .enumerate()
        {
            assert_ne!(
                unsafe { GetForegroundWindow() },
                target,
                "test target must begin behind the other owned window"
            );
            if index == 0 {
                // Simulate a game capturing the mouse independently of this
                // session's button state. Only explicit click activation may
                // switch away; a move alone must not steal foreground.
                assert!(!sink.left && !sink.right);
                let _ = unsafe { SetCapture(hwnd) };
                assert_eq!(unsafe { GetCapture() }, hwnd);
                let nx = 530.0 / (display.width - 1) as f64;
                owned_pointer(
                    deadline,
                    &mut sink,
                    &display,
                    second,
                    nx,
                    ny,
                    Input::Move {
                        display: display.id,
                        layout: 7,
                        x: nx,
                        y: ny,
                    },
                );
                assert_eq!(unsafe { GetForegroundWindow() }, hwnd);
                assert_eq!(unsafe { GetCapture() }, hwnd);
            }
            let nx = px / (display.width - 1) as f64;
            let before = click_counts(target);
            owned_pointer_batch(
                deadline,
                &mut sink,
                &display,
                target,
                nx,
                y,
                vec![
                    Input::Button {
                        display: display.id,
                        layout: 7,
                        x: nx,
                        y,
                        button: Button::Left,
                        down: true,
                    },
                    Input::Button {
                        display: display.id,
                        layout: 7,
                        x: nx,
                        y,
                        button: Button::Left,
                        down: false,
                    },
                ],
            );
            let clicked = pump_until_check(deadline, || {
                let after = click_counts(target);
                let delivered = if index == 5 {
                    after.caption_down == before.caption_down + 1
                        && after.caption_up == before.caption_up + 1
                } else {
                    after.client_down == before.client_down + 1
                        && after.client_up == before.client_up + 1
                };
                delivered
                    && unsafe { GetForegroundWindow() } == target
                    && unsafe { GetCapture() }.0.is_null()
            });
            assert!(
                clicked,
                "rear click {index} failed: capture_simulation={}, caption={}, target={target:?}, foreground={:?}, capture={:?}, hit={:?}, point={:?}, before={before:?}, after={:?}, first={:?}, second={:?}",
                index == 0,
                index == 5,
                unsafe { GetForegroundWindow() },
                unsafe { GetCapture() },
                unsafe {
                    WindowFromPoint(windows::Win32::Foundation::POINT {
                        x: map_point(&display, nx, y).unwrap().0,
                        y: map_point(&display, nx, y).unwrap().1,
                    })
                },
                map_point(&display, nx, y).unwrap(),
                click_counts(target),
                click_counts(hwnd),
                click_counts(second)
            );
            assert_eq!(unsafe { GetForegroundWindow() }, target);
            assert!(!sink.left);
            assert_eq!(unsafe { GetAsyncKeyState(1) } & i16::MIN, 0);
            sink.apply(
                Input::Key {
                    vk: 0x41,
                    down: true,
                    repeat: false,
                },
                None,
                0,
            )
            .unwrap();
            sink.apply(
                Input::Key {
                    vk: 0x41,
                    down: false,
                    repeat: false,
                },
                None,
                0,
            )
            .unwrap();
            sink.release_all().unwrap();
        }
        // Begin a selection in the front window, move onto the other owned
        // window, and release there. The original window's mouse capture must
        // end so a subsequent click can activate the rear window again.
        let left = 150.0 / (display.width - 1) as f64;
        let right = 530.0 / (display.width - 1) as f64;
        owned_pointer(
            deadline,
            &mut sink,
            &display,
            hwnd,
            left,
            ny,
            Input::Button {
                display: display.id,
                layout: 7,
                x: left,
                y: ny,
                button: Button::Left,
                down: true,
            },
        );
        pump_until(
            deadline,
            || unsafe { GetCapture() } == hwnd,
            "owned edit window did not begin the controlled drag",
        );
        owned_pointer(
            deadline,
            &mut sink,
            &display,
            second,
            right,
            ny,
            Input::Move {
                display: display.id,
                layout: 7,
                x: right,
                y: ny,
            },
        );
        owned_pointer(
            deadline,
            &mut sink,
            &display,
            second,
            right,
            ny,
            Input::Button {
                display: display.id,
                layout: 7,
                x: right,
                y: ny,
                button: Button::Left,
                down: false,
            },
        );
        pump_until(
            deadline,
            || unsafe { GetCapture() }.0.is_null(),
            "dragging out left mouse capture active after release",
        );
        assert!(!sink.left);
        assert_eq!(unsafe { GetAsyncKeyState(1) } & i16::MIN, 0);
        assert_eq!(unsafe { GetForegroundWindow() }, hwnd);
        for down in [true, false] {
            owned_pointer(
                deadline,
                &mut sink,
                &display,
                second,
                right,
                ny,
                Input::Button {
                    display: display.id,
                    layout: 7,
                    x: right,
                    y: ny,
                    button: Button::Left,
                    down,
                },
            );
        }
        pump_until(
            deadline,
            || unsafe { GetForegroundWindow() } == second,
            "rear window could not activate after controlled drag release",
        );
        assert!(!sink.left);
        assert_eq!(unsafe { GetAsyncKeyState(1) } & i16::MIN, 0);
        sink.release_all().unwrap();
    }
}
