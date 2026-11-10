//! Disposable hosted-runner acceptance test; never linked into the application.
use crate::{Capture, Display, Input, InputSink};
use std::{
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};

#[derive(Clone)]
struct Window {
    handle: HWND,
    title: String,
    image: String,
    pid: u32,
}
fn windows() -> Vec<Window> {
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return 1;
            }
            let mut image = vec![0u16; 32768];
            let mut n = image.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut n);
            CloseHandle(process);
            if ok == 0 {
                return 1;
            }
            let mut title = vec![0u16; 1024];
            let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
            (*(data as *mut Vec<Window>)).push(Window {
                handle: hwnd,
                pid,
                title: String::from_utf16_lossy(&title[..len.max(0) as usize]),
                image: String::from_utf16_lossy(&image[..n as usize]).to_lowercase(),
            });
            1
        }
    }
    let mut result = Vec::<Window>::new();
    unsafe {
        EnumWindows(Some(visit), &mut result as *mut _ as LPARAM);
    }
    result
}
fn wait_window(predicate: impl Fn(&Window) -> bool, timeout: u64) -> Result<Window, String> {
    let until = Instant::now() + Duration::from_secs(timeout);
    loop {
        if let Some(window) = windows().into_iter().find(&predicate) {
            return Ok(window);
        }
        if Instant::now() >= until {
            return Err("expected visible window did not appear".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
struct Probe {
    capture: Capture,
    input: InputSink,
    display: Display,
    sequence: u64,
}
impl Probe {
    fn refresh(&mut self) -> Result<(u32, u32), String> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            self.sequence += 1;
            match self.capture.next_frame(1, self.sequence) {
                Ok(Some(frame)) => {
                    if self.capture.desktop_context().is_empty() {
                        return Err("capture bypassed SYSTEM broker".into());
                    }
                    self.input.bind_desktop(self.capture.desktop_context(), 1);
                    return Ok((frame.meta.width, frame.meta.height));
                }
                Err(crate::Error::CaptureReset(_)) => {
                    self.capture =
                        Capture::new_with_profile(self.display.id, 0).map_err(|e| e.to_string())?;
                }
                Err(e) => return Err(e.to_string()),
                _ => (),
            }
            if Instant::now() >= until {
                return Err("no fresh broker frame".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    fn apply(&mut self, input: Input) -> Result<(), String> {
        self.input
            .apply(input, Some(&self.display), 1)
            .map_err(|e| e.to_string())
    }
    fn keys(&mut self, keys: &[(u16, bool)]) -> Result<(), String> {
        for &(vk, down) in keys {
            self.apply(Input::Key {
                vk,
                down,
                repeat: false,
            })?;
        }
        Ok(())
    }
    fn launch(&mut self, command: &str) -> Result<(), String> {
        self.keys(&[(0x5b, true), (0x52, true), (0x52, false), (0x5b, false)])?;
        wait_window(
            |w| w.title == "Run" && w.image.ends_with("\\explorer.exe"),
            5,
        )?;
        self.apply(Input::Text {
            text: command.into(),
        })?;
        self.keys(&[(0x0d, true), (0x0d, false)])
    }
    fn click_title(&mut self, window: &Window) -> Result<(), String> {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(window.handle, &mut rect) == 0 {
                return Err("window rectangle unavailable".into());
            }
            let x = (rect.left + (rect.right - rect.left) / 2).clamp(
                self.display.x,
                self.display.x + self.display.width as i32 - 1,
            );
            let y = (rect.top + 24).clamp(
                self.display.y,
                self.display.y + self.display.height as i32 - 1,
            );
            let px = (x - self.display.x) as f64 / self.display.width as f64;
            let py = (y - self.display.y) as f64 / self.display.height as f64;
            self.apply(Input::Move {
                display: self.display.id,
                layout: 1,
                x: px,
                y: py,
            })?;
            for down in [true, false] {
                self.apply(Input::Button {
                    display: self.display.id,
                    layout: 1,
                    x: px,
                    y: py,
                    button: crate::Button::Left,
                    down,
                })?;
            }
            thread::sleep(Duration::from_millis(200));
            let mut cursor: POINT = std::mem::zeroed();
            if GetCursorPos(&mut cursor) == 0
                || (cursor.x - x).abs() > 2
                || (cursor.y - y).abs() > 2
            {
                return Err("broker mouse did not reach the target title bar".into());
            }
            if GetAncestor(WindowFromPoint(cursor), GA_ROOT) != window.handle {
                return Err("target title bar is covered by another window".into());
            }
            let mut pid = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
            let foreground = GetForegroundWindow();
            if pid != window.pid
                && GetAncestor(foreground, GA_ROOT) != window.handle
                && IsChild(window.handle, foreground) == 0
            {
                return Err(format!(
                    "broker mouse click did not activate target window: expected pid {}, foreground pid {pid}",
                    window.pid
                ));
            }
        }
        Ok(())
    }
}
#[test]
#[ignore = "opens and closes real system tools through installed SYSTEM broker; disposable hosted runner only"]
fn real_system_tools_through_broker() {
    assert_eq!(
        std::env::var("FARSAIL_SYSTEM_TOOLS_TEST").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    assert_eq!(
        std::env::var("RUNNER_ENVIRONMENT").as_deref(),
        Ok("github-hosted")
    );
    crate::ensure_dpi_awareness().unwrap();
    // This disposable runner image starts a Performance property sheet before checkout.
    // Dismiss only that exact system fixture without applying any settings, so opening
    // Advanced System Properties below must create a fresh tool window.
    let mut fixture_reset = Vec::new();
    for w in windows().into_iter().filter(|w| {
        w.image
            .ends_with("\\system32\\systempropertiesperformance.exe")
            && w.title == "System Properties"
    }) {
        fixture_reset.push(w.pid);
        unsafe {
            PostMessageW(w.handle, WM_CLOSE, 0, 0);
        }
        let until = Instant::now() + Duration::from_secs(5);
        while unsafe { IsWindow(w.handle) != 0 && IsWindowVisible(w.handle) != 0 } {
            assert!(
                Instant::now() < until,
                "runner's initial Performance property sheet did not close"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
    println!("Disposed hosted-image initial Performance property sheet pids: {fixture_reset:?}");
    let display = crate::displays().unwrap().remove(0);
    let capture = Capture::new_with_profile(display.id, 0).unwrap();
    let mut probe = Probe {
        capture,
        input: InputSink::default(),
        display,
        sequence: 0,
    };
    probe
        .refresh()
        .expect("authenticated SYSTEM capture and desktop witness");
    let cases = [
        (
            "Task Manager",
            "taskmgr.exe",
            "\\taskmgr.exe",
            "Task Manager",
        ),
        (
            "Device Manager",
            "devmgmt.msc",
            "\\mmc.exe",
            "Device Manager",
        ),
        ("Services", "services.msc", "\\mmc.exe", "Services"),
        (
            "Computer Management",
            "compmgmt.msc",
            "\\mmc.exe",
            "Computer Management",
        ),
        ("Event Viewer", "eventvwr.msc", "\\mmc.exe", "Event Viewer"),
        (
            "Disk Management",
            "diskmgmt.msc",
            "\\mmc.exe",
            "Disk Management",
        ),
        (
            "Registry Editor",
            "regedit.exe",
            "\\regedit.exe",
            "Registry Editor",
        ),
        (
            "System Information",
            "msinfo32.exe",
            "\\msinfo32.exe",
            "System Information",
        ),
        (
            "Control Panel",
            "control.exe",
            "\\explorer.exe",
            "Control Panel",
        ),
        (
            "System Properties",
            "SystemPropertiesAdvanced.exe",
            "\\systempropertiesadvanced.exe",
            "System Properties",
        ),
        (
            "Settings",
            "ms-settings:",
            "\\systemsettings.exe",
            "Settings",
        ),
    ];
    let mut report = Vec::new();
    for (name, command, image, title) in cases {
        let before = windows();
        let matches = |w: &Window| {
            (w.image.ends_with(image)
                || (name == "Settings" && w.image.ends_with("\\applicationframehost.exe"))
                || (name == "System Properties"
                    && (w.image.ends_with("\\rundll32.exe")
                        || w.image.rsplit('\\').next().is_some_and(|n| {
                            n.starts_with("systemproperties") && n.ends_with(".exe")
                        }))))
                && w.title.contains(title)
        };
        let started = Instant::now();
        let result = (|| -> Result<serde_json::Value, String> {
            if before.iter().any(matches) {
                return Err("pre-existing tool window; test refuses to close it".into());
            }
            probe.launch(command)?;
            let window = wait_window(matches, 10)?;
            // UWP's frame can exist before the application has completed activation.
            // Wait for the same target window to become interactive before testing input.
            let until = Instant::now() + Duration::from_secs(10);
            loop {
                let ready = unsafe {
                    let foreground = GetForegroundWindow();
                    let mut pid = 0;
                    GetWindowThreadProcessId(foreground, &mut pid);
                    pid == window.pid
                        || GetAncestor(foreground, GA_ROOT) == window.handle
                        || IsChild(window.handle, foreground) != 0
                };
                if ready {
                    break;
                }
                if Instant::now() >= until {
                    return Err(
                        "tool window appeared but did not finish foreground activation".into(),
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
            let dimensions = probe.refresh()?;
            probe.click_title(&window)?;
            probe.keys(&[(0xa4, true), (0x73, true), (0x73, false), (0xa4, false)])?;
            let until = Instant::now() + Duration::from_secs(5);
            while unsafe { IsWindow(window.handle) != 0 && IsWindowVisible(window.handle) != 0 } {
                if Instant::now() >= until {
                    return Err("remote Alt+F4 did not close target window".into());
                }
                thread::sleep(Duration::from_millis(100));
            }
            Ok(
                serde_json::json!({"image":window.image,"pid":window.pid,"frame_width":dimensions.0,"frame_height":dimensions.1,"launch":"passed","service_capture":"passed","service_mouse_activation":"passed","service_keyboard_close":"passed"}),
            )
        })();
        let diagnostics = if result.is_err() {
            windows().into_iter().map(|w|serde_json::json!({"title":w.title,"image":w.image,"pid":w.pid,"hwnd":w.handle as usize})).collect::<Vec<_>>()
        } else {
            vec![]
        };
        let _ = probe.input.release_all();
        // Close only newly created matching windows if an assertion failed; never alter tool contents.
        for w in windows().into_iter().filter(matches) {
            if !before.iter().any(|old| old.handle == w.handle) {
                unsafe {
                    PostMessageW(w.handle, WM_CLOSE, 0, 0);
                }
            }
        }
        println!("SYSTEM TOOL {name}: {result:?}");
        report.push(serde_json::json!({"name":name,"elapsed_ms":started.elapsed().as_millis(),"result":result,"failure_windows":diagnostics,"hosted_image_fixture_reset_pids":fixture_reset}));
    }
    std::fs::write(
        std::env::var("FARSAIL_SYSTEM_TOOLS_REPORT").unwrap(),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        report.iter().all(|r| r["result"].get("Ok").is_some()),
        "some system tools failed; inspect per-tool report"
    );
}
