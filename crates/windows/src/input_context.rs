//! Read-only checks for retrying input after a foreground change.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForegroundWindow {
    pub window: usize,
    pub process_id: u32,
}

#[cfg(windows)]
mod platform {
    use super::ForegroundWindow;
    use std::mem::{size_of, size_of_val};
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            Security::{
                GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid,
                TOKEN_ELEVATION, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenElevation,
                TokenIntegrityLevel,
            },
            System::{
                StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_IO},
                Threading::{
                    GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken,
                    PROCESS_QUERY_LIMITED_INFORMATION,
                },
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
        core::BOOL,
    };

    // Only real handles returned by OpenProcess/OpenProcessToken are owned here.
    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    /// Reports the current process token's elevation without changing privileges.
    pub fn administrator_mode() -> Option<bool> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
        let token = OwnedHandle(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        unsafe {
            GetTokenInformation(
                token.0,
                TokenElevation,
                Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            )
        }
        .ok()?;
        (returned as usize == size_of::<TOKEN_ELEVATION>())
            .then_some(elevation.TokenIsElevated != 0)
    }

    pub fn foreground_window() -> Option<ForegroundWindow> {
        let window = unsafe { GetForegroundWindow() };
        if window.0.is_null() {
            return None;
        }
        let mut process_id = 0;
        if unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) } == 0
            || process_id == 0
            || unsafe { GetForegroundWindow() } != window
        {
            return None;
        }
        Some(ForegroundWindow {
            window: window.0 as usize,
            process_id,
        })
    }

    fn thread_desktop_receives_input() -> bool {
        // GetThreadDesktop returns a borrowed handle; it must not be closed.
        let Ok(desktop) = (unsafe { GetThreadDesktop(GetCurrentThreadId()) }) else {
            return false;
        };
        let mut receives_input = BOOL::default();
        unsafe {
            GetUserObjectInformationW(
                HANDLE(desktop.0),
                UOI_IO,
                Some((&mut receives_input as *mut BOOL).cast()),
                size_of::<BOOL>() as u32,
                None,
            )
        }
        .is_ok()
            && receives_input.as_bool()
    }

    fn integrity_level(process: HANDLE) -> Option<u32> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
        let token = OwnedHandle(token);
        // usize storage supplies TOKEN_MANDATORY_LABEL alignment. A token label
        // and the largest Windows SID fit well within this bounded buffer.
        let mut buffer = [0usize; 64];
        let mut returned = 0;
        unsafe {
            GetTokenInformation(
                token.0,
                TokenIntegrityLevel,
                Some(buffer.as_mut_ptr().cast()),
                size_of_val(&buffer) as u32,
                &mut returned,
            )
        }
        .ok()?;
        if (returned as usize) < size_of::<TOKEN_MANDATORY_LABEL>()
            || (returned as usize) > size_of_val(&buffer)
        {
            return None;
        }
        let label = unsafe { buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>().read() };
        let start = buffer.as_ptr() as usize;
        let end = start.checked_add(returned as usize)?;
        let sid = label.Label.Sid;
        let sid_start = sid.0 as usize;
        if sid_start < start.checked_add(size_of::<TOKEN_MANDATORY_LABEL>())?
            || sid_start.checked_add(8)? > end
        {
            return None;
        }
        let count = unsafe { *GetSidSubAuthorityCount(sid) };
        if count == 0 || count > 15 || sid_start.checked_add(8 + usize::from(count) * 4)? > end {
            return None;
        }
        if !unsafe { IsValidSid(sid) }.as_bool() {
            return None;
        }
        Some(unsafe { GetSidSubAuthority(sid, u32::from(count - 1)).read_unaligned() })
    }

    /// A conservative preflight, not proof that the next SendInput will succeed.
    /// Unknown state and a changing foreground fail closed without any injection.
    pub fn input_context_ready(expected: ForegroundWindow) -> bool {
        if expected.window == 0
            || expected.process_id == 0
            || foreground_window() != Some(expected)
            || !thread_desktop_receives_input()
        {
            return false;
        }
        let Ok(process) = (unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                expected.process_id,
            )
        }) else {
            return false;
        };
        let process = OwnedHandle(process);
        let (Some(target), Some(own)) = (
            integrity_level(process.0),
            integrity_level(unsafe { GetCurrentProcess() }),
        ) else {
            return false;
        };
        target <= own && thread_desktop_receives_input() && foreground_window() == Some(expected)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn own_process_integrity_can_be_queried_without_input() {
            assert!(integrity_level(unsafe { GetCurrentProcess() }).is_some());
        }

        #[test]
        fn own_process_elevation_can_be_queried_without_input() {
            assert!(administrator_mode().is_some());
        }

        #[test]
        fn null_and_mismatched_foreground_witnesses_are_not_ready() {
            assert!(!input_context_ready(ForegroundWindow {
                window: 0,
                process_id: 0,
            }));
            assert!(!input_context_ready(ForegroundWindow {
                window: usize::MAX,
                process_id: u32::MAX,
            }));
        }
    }
}

#[cfg(windows)]
pub use platform::{administrator_mode, foreground_window, input_context_ready};

#[cfg(not(windows))]
pub fn administrator_mode() -> Option<bool> {
    None
}

#[cfg(not(windows))]
pub fn foreground_window() -> Option<ForegroundWindow> {
    None
}

#[cfg(not(windows))]
pub fn input_context_ready(_expected: ForegroundWindow) -> bool {
    false
}
