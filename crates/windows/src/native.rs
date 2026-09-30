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
}
impl Capture {
    pub fn new(id: u32) -> Result<Self> {
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
        let factor = (logical_w as f64 / 1280.0)
            .max(logical_h as f64 / 720.0)
            .max(1.0);
        let out_w = (logical_w as f64 / factor).floor().max(1.0) as usize;
        let out_h = (logical_h as f64 / factor).floor().max(1.0) as usize;
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
        self.last_rgb = Some(rgb.clone());
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
        Ok(Some(JpegFrame::encode_rgb(meta, &rgb, 55)?))
    }
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
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let inserted = SendInput(inputs, size_of::<INPUT>() as i32);
        (inserted, GetLastError().0)
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
                move_to(display, layout, id, expected, x, y)?;
                let (slot, down_flag, up_flag) = match button {
                    Button::Left => (&mut self.left, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                    Button::Right => (&mut self.right, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                };
                if *slot == down {
                    return Ok(());
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
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SetFocus};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow, GetWindowTextW,
            GetWindowThreadProcessId, HWND_TOPMOST, MSG, PM_REMOVE, PeekMessageW, SWP_NOMOVE,
            SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos, TranslateMessage,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };
        use windows::core::w;
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
        impl Drop for TestWindow {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let _window = TestWindow(hwnd);
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
    }
}
