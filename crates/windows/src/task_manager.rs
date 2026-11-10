//! Fixed system-tool action. Runs in the interactive main process, never the SYSTEM broker.
#[cfg(not(windows))]
pub fn open() -> Result<(), String> {
    Err("此被控端不支持打开 Windows 任务管理器".into())
}

#[cfg(windows)]
pub fn open() -> Result<(), String> {
    use std::{
        os::windows::ffi::OsStringExt,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::*,
        System::{
            RemoteDesktop::ProcessIdToSessionId, SystemInformation::GetSystemDirectoryW,
            Threading::*,
        },
        UI::WindowsAndMessaging::*,
    };
    struct Search {
        path: String,
        session: u32,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> i32 {
        unsafe {
            let search = &mut *(param as *mut Search);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut session = 0;
            if ProcessIdToSessionId(pid, &mut session) == 0 || session != search.session {
                return 1;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return 1;
            }
            let mut buffer = vec![0u16; 32768];
            let mut len = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut len);
            CloseHandle(process);
            if ok != 0
                && String::from_utf16_lossy(&buffer[..len as usize])
                    .eq_ignore_ascii_case(&search.path)
            {
                search.found = hwnd;
                return 0;
            }
            1
        }
    }
    unsafe {
        let mut buffer = vec![0u16; 32768];
        let len = GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32);
        if len == 0 || len as usize >= buffer.len() {
            return Err("无法定位 Windows 系统目录".into());
        }
        let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len as usize]))
            .join("Taskmgr.exe");
        let mut session = 0;
        if ProcessIdToSessionId(GetCurrentProcessId(), &mut session) == 0 || session == 0 {
            return Err("任务管理器必须在已登录用户的交互会话中打开".into());
        }
        let mut search = Search {
            path: path.to_string_lossy().into_owned(),
            session,
            found: std::ptr::null_mut(),
        };
        EnumWindows(Some(visit), &mut search as *mut _ as LPARAM);
        let mut child = if search.found.is_null() {
            Some(
                std::process::Command::new(path)
                    .spawn()
                    .map_err(|e| format!("启动任务管理器失败：{e}"))?,
            )
        } else {
            None
        };
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            EnumWindows(Some(visit), &mut search as *mut _ as LPARAM);
            if !search.found.is_null() {
                if IsIconic(search.found) != 0 {
                    ShowWindowAsync(search.found, SW_RESTORE);
                }
                SetForegroundWindow(search.found);
                return Ok(());
            }
            if Instant::now() >= until {
                return Err("任务管理器未出现可见窗口，请检查被控端系统策略或授权提示".into());
            }
            // A launcher may exit after forwarding to an existing instance; still wait for its window.
            if let Some(process) = child.as_mut() {
                let _ = process.try_wait();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "opens a real system window; disposable interactive Windows runner only"]
    fn real_task_manager_window() {
        assert_eq!(
            std::env::var("FARSAIL_TEST_TASK_MANAGER").as_deref(),
            Ok("1")
        );
        super::open().expect("a visible system Taskmgr.exe window in the caller session");
        // Reusing an existing window must succeed as well.
        super::open().expect("existing Task Manager window");
    }
}
