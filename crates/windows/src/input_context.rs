//! Input desktop checks and conservative activation for a fresh pointer click.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForegroundWindow {
    pub window: usize,
    pub process_id: u32,
}

#[cfg(any(windows, test))]
pub(crate) mod click_activation {
    pub(super) const CONFIRM_BUDGET_MS: u32 = 100;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Target {
        pub hit: usize,
        pub root: usize,
        pub process_id: u32,
        pub thread_id: u32,
        pub foreground: usize,
        pub foreground_process_id: u32,
        pub visible: bool,
        pub enabled: bool,
        pub no_activate: bool,
        pub input_desktop: bool,
        pub own_integrity: Option<u32>,
        pub target_integrity: Option<u32>,
    }

    impl Target {
        pub(super) fn eligible(self) -> bool {
            self.hit != 0
                && self.root != 0
                && self.process_id != 0
                && self.thread_id != 0
                && self.foreground != 0
                && self.foreground_process_id != 0
                && self.root != self.foreground
                && self.visible
                && self.enabled
                && !self.no_activate
                && self.input_desktop
                && matches!((self.target_integrity, self.own_integrity),
                    (Some(target), Some(own)) if target <= own)
        }

        fn same_destination(self, current: Self) -> bool {
            // Becoming foreground is the intended change. All hit, identity,
            // style, desktop and integrity facts must still match the witness.
            let expected = Self {
                foreground: current.foreground,
                foreground_process_id: current.foreground_process_id,
                ..self
            };
            current == expected
                && current.foreground == self.root
                && current.foreground_process_id == self.process_id
        }
    }

    pub fn fresh_down(down: bool, left_held: bool, right_held: bool) -> bool {
        down && !left_held && !right_held
    }

    /// Re-read the hit target and security context immediately before attempting
    /// activation. A refused attempt never replaces ordinary SendInput diagnostics.
    pub fn attempt(
        mut inspect: impl FnMut() -> Option<Target>,
        mut activate: impl FnMut(Target),
    ) -> bool {
        let Some(first) = inspect().filter(|target| target.eligible()) else {
            return false;
        };
        if inspect() != Some(first) {
            return false;
        }
        activate(first);
        true
    }

    pub fn confirm(
        target: Target,
        mut request: impl FnMut(usize) -> bool,
        mut barrier: impl FnMut(usize, u32) -> bool,
        mut inspect: impl FnMut() -> Option<Target>,
    ) -> bool {
        target.eligible()
            && request(target.root)
            && barrier(target.root, CONFIRM_BUDGET_MS)
            && inspect().is_some_and(|current| target.same_destination(current))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn target() -> Target {
            Target {
                hit: 1,
                root: 2,
                process_id: 3,
                thread_id: 4,
                foreground: 5,
                foreground_process_id: 6,
                visible: true,
                enabled: true,
                no_activate: false,
                input_desktop: true,
                own_integrity: Some(0x2000),
                target_integrity: Some(0x2000),
            }
        }

        #[test]
        fn only_a_stable_eligible_hit_attempts_activation_once() {
            let mut reads = 0;
            let mut attempts = Vec::new();
            assert!(attempt(
                || {
                    reads += 1;
                    Some(target())
                },
                |target| attempts.push(target.root)
            ));
            assert_eq!(reads, 2);
            assert_eq!(attempts, [2]);
        }

        #[test]
        fn unknown_disabled_noactivate_and_higher_integrity_targets_are_skipped() {
            let base = target();
            let mut cases = Vec::new();
            for invalid in [
                Target { hit: 0, ..base },
                Target { root: 0, ..base },
                Target {
                    process_id: 0,
                    ..base
                },
                Target {
                    thread_id: 0,
                    ..base
                },
                Target {
                    foreground: 0,
                    ..base
                },
                Target {
                    foreground_process_id: 0,
                    ..base
                },
                Target {
                    foreground: base.root,
                    ..base
                },
                Target {
                    visible: false,
                    ..base
                },
                Target {
                    enabled: false,
                    ..base
                },
                Target {
                    no_activate: true,
                    ..base
                },
                Target {
                    input_desktop: false,
                    ..base
                },
                Target {
                    own_integrity: None,
                    ..base
                },
                Target {
                    target_integrity: None,
                    ..base
                },
                Target {
                    target_integrity: Some(0x3000),
                    ..base
                },
            ] {
                cases.push(Some(invalid));
            }
            cases.push(None);
            for candidate in cases {
                let mut attempted = false;
                assert!(!attempt(|| candidate, |_| attempted = true));
                assert!(!attempted);
            }
        }

        #[test]
        fn changed_hit_process_or_context_does_not_activate() {
            let base = target();
            for changed in [
                Some(Target { hit: 6, ..base }),
                Some(Target { root: 6, ..base }),
                Some(Target {
                    process_id: 6,
                    ..base
                }),
                Some(Target {
                    thread_id: 6,
                    ..base
                }),
                Some(Target {
                    foreground: 6,
                    ..base
                }),
                Some(Target {
                    foreground_process_id: 7,
                    ..base
                }),
                Some(Target {
                    input_desktop: false,
                    ..base
                }),
                Some(Target {
                    target_integrity: None,
                    ..base
                }),
                None,
            ] {
                let mut candidates = [Some(base), changed].into_iter();
                let mut attempted = false;
                assert!(!attempt(
                    || candidates.next().unwrap(),
                    |_| attempted = true
                ));
                assert!(!attempted);
            }
        }

        #[test]
        fn release_duplicate_and_drag_button_changes_never_activate() {
            assert!(fresh_down(true, false, false));
            for held in [(true, false), (false, true), (true, true)] {
                assert!(!fresh_down(true, held.0, held.1));
            }
            for held in [(false, false), (true, false), (false, true), (true, true)] {
                assert!(!fresh_down(false, held.0, held.1));
            }
        }

        #[test]
        fn accepted_activation_confirms_with_one_bounded_barrier_and_updated_foreground() {
            let base = target();
            let current = Target {
                foreground: base.root,
                foreground_process_id: base.process_id,
                ..base
            };
            let events = std::cell::RefCell::new(Vec::new());
            assert!(confirm(
                base,
                |root| {
                    events.borrow_mut().push(("request", root, 0));
                    true
                },
                |root, budget| {
                    events.borrow_mut().push(("barrier", root, budget));
                    true
                },
                || {
                    events.borrow_mut().push(("inspect", 0, 0));
                    Some(current)
                },
            ));
            assert_eq!(
                *events.borrow(),
                [
                    ("request", base.root, 0),
                    ("barrier", base.root, 100),
                    ("inspect", 0, 0)
                ]
            );
        }

        #[test]
        fn refused_or_timed_out_activation_does_not_retry_or_claim_confirmation() {
            for (accepted, responsive, expected_barriers) in [(false, true, 0), (true, false, 1)] {
                let mut requests = 0;
                let mut barriers = 0;
                assert!(!confirm(
                    target(),
                    |_| {
                        requests += 1;
                        accepted
                    },
                    |_, budget| {
                        barriers += 1;
                        assert_eq!(budget, 100);
                        responsive
                    },
                    || panic!("refused/timed out target must not count as confirmed"),
                ));
                assert_eq!(requests, 1);
                assert_eq!(barriers, expected_barriers);
            }
        }

        #[test]
        fn confirmation_rechecks_hit_identity_desktop_and_integrity() {
            let base = target();
            let current = Target {
                foreground: base.root,
                foreground_process_id: base.process_id,
                ..base
            };
            for changed in [
                Some(Target { hit: 99, ..current }),
                Some(Target {
                    process_id: 99,
                    ..current
                }),
                Some(Target {
                    thread_id: 99,
                    ..current
                }),
                Some(Target {
                    input_desktop: false,
                    ..current
                }),
                Some(Target {
                    target_integrity: Some(0x3000),
                    ..current
                }),
                Some(Target {
                    own_integrity: None,
                    ..current
                }),
                Some(Target {
                    no_activate: true,
                    ..current
                }),
                Some(base),
                None,
            ] {
                assert!(!confirm(base, |_| true, |_, _| true, || changed));
            }
            let mut requested = false;
            assert!(!confirm(
                Target {
                    enabled: false,
                    ..base
                },
                |_| {
                    requested = true;
                    true
                },
                |_, _| true,
                || Some(current)
            ));
            assert!(!requested);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{ForegroundWindow, click_activation};
    use std::mem::{size_of, size_of_val};
    use windows::{
        Win32::{
            Foundation::{
                CloseHandle, GetLastError, HANDLE, HWND, LPARAM, POINT, SetLastError, WIN32_ERROR,
                WPARAM,
            },
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
            UI::{
                Input::KeyboardAndMouse::IsWindowEnabled,
                WindowsAndMessaging::{
                    GA_ROOT, GWL_EXSTYLE, GetAncestor, GetForegroundWindow, GetWindowLongPtrW,
                    GetWindowThreadProcessId, IsWindowVisible, SMTO_ABORTIFHUNG, SMTO_BLOCK,
                    SendMessageTimeoutW, SetForegroundWindow, WM_NULL, WS_EX_NOACTIVATE,
                    WindowFromPoint,
                },
            },
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

    fn desktop_receives_input(thread_id: u32) -> bool {
        // GetThreadDesktop returns a borrowed handle; it must not be closed.
        let Ok(desktop) = (unsafe { GetThreadDesktop(thread_id) }) else {
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

    fn thread_desktop_receives_input() -> bool {
        desktop_receives_input(unsafe { GetCurrentThreadId() })
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

    fn click_target(x: i32, y: i32) -> Option<click_activation::Target> {
        let hit = unsafe { WindowFromPoint(POINT { x, y }) };
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        if hit.0.is_null() || root.0.is_null() {
            return None;
        }
        let foreground = foreground_window()?;
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut process_id)) };
        if process_id == 0 || thread_id == 0 {
            return None;
        }
        unsafe { SetLastError(WIN32_ERROR(0)) };
        let style = unsafe { GetWindowLongPtrW(root, GWL_EXSTYLE) };
        if style == 0 && unsafe { GetLastError() }.0 != 0 {
            return None;
        }
        let process = OwnedHandle(
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?,
        );
        Some(click_activation::Target {
            hit: hit.0 as usize,
            root: root.0 as usize,
            process_id,
            thread_id,
            foreground: foreground.window,
            foreground_process_id: foreground.process_id,
            visible: unsafe { IsWindowVisible(hit).as_bool() && IsWindowVisible(root).as_bool() },
            enabled: unsafe { IsWindowEnabled(hit).as_bool() && IsWindowEnabled(root).as_bool() },
            no_activate: style as u32 & WS_EX_NOACTIVATE.0 != 0,
            input_desktop: thread_desktop_receives_input() && desktop_receives_input(thread_id),
            own_integrity: integrity_level(unsafe { GetCurrentProcess() }),
            target_integrity: integrity_level(process.0),
        })
    }

    /// Best effort only: Windows foreground policy and application input handling
    /// still apply. Never attach input queues, change z-order, or elevate privileges.
    pub(crate) fn try_activate_at(x: i32, y: i32) {
        let attempted = click_activation::attempt(
            || {
                let target = click_target(x, y);
                #[cfg(test)]
                eprintln!(
                    "owned click activation candidate: {target:?}, eligible={}",
                    target.is_some_and(|candidate| candidate.eligible())
                );
                target
            },
            |target| {
                let confirmed = click_activation::confirm(
                    target,
                    |root| {
                        let accepted =
                            unsafe { SetForegroundWindow(HWND(root as *mut std::ffi::c_void)) }
                                .as_bool();
                        #[cfg(test)]
                        eprintln!(
                            "owned click activation request: root={root}, accepted={accepted}, foreground={:?}",
                            foreground_window()
                        );
                        accepted
                    },
                    |root, budget| {
                        // On the caller's GUI thread activation is synchronous.
                        // SendMessageTimeout would call its procedure directly
                        // and ignore the timeout, so only perform the post-check.
                        if target.thread_id == unsafe { GetCurrentThreadId() } {
                            #[cfg(test)]
                            eprintln!(
                                "owned click activation barrier: root={root}, skipped_same_thread=true"
                            );
                            return true;
                        }
                        // Cross-input-queue activation is asynchronous. WM_NULL
                        // lets that queue process the activation before DOWN is
                        // injected, with a short budget and no input-queue attach.
                        let responsive = unsafe {
                            SendMessageTimeoutW(
                                HWND(root as *mut std::ffi::c_void),
                                WM_NULL,
                                WPARAM(0),
                                LPARAM(0),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                budget,
                                None,
                            )
                        }
                        .0 != 0;
                        #[cfg(test)]
                        eprintln!(
                            "owned click activation barrier: root={root}, budget_ms={budget}, responsive={responsive}, foreground={:?}",
                            foreground_window()
                        );
                        responsive
                    },
                    || {
                        let current = click_target(x, y);
                        #[cfg(test)]
                        eprintln!("owned click activation post-barrier candidate: {current:?}");
                        current
                    },
                );
                #[cfg(test)]
                eprintln!("owned click activation confirmation: confirmed={confirmed}");
                // Failure never replaces or suppresses the ordinary SendInput
                // path: it retains its own counts and error diagnostics.
                let _ = confirmed;
            },
        );
        #[cfg(test)]
        eprintln!(
            "owned click activation finished: attempted={attempted}, foreground={:?}",
            foreground_window()
        );
        let _ = attempted;
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

#[cfg(windows)]
pub(crate) use platform::try_activate_at;

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
