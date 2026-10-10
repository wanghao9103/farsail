**English** | [简体中文](WI-WINDOW-032.zh-CN.md)

# WINDOW-032 — Overlapping windows and pointer release

Date: 2026-10-10. Baseline `dab641d`, desktop candidate 0.1.21. This is candidate-source verification, not a public-release or actual-game acceptance claim.

## Report and findings

The user controls Windows from Ubuntu. Two windows of the same game overlap; cursor positioning and the front window work, but clicking the rear window's exposed content or title bar does not bring it forward. The exact game and its input messages have not been inspected. Game capture, activation behavior and injected-input handling remain candidates rather than a proven unique cause.

The viewer forwards coordinates from one desktop image; it does not filter Windows foreground/background targets. Ordinary host input does not preflight against foreground integrity. That read-only witness is used for recovery after an injection failure. Existing owned-window tests activated every target before clicking, leaving rear-window activation untested.

A separate release defect was reproduced with real browser events and delayed synthetic IPC. An outside pointerup queues the positioned UP, then lostpointercapture and pointerleave occur. The old leave handler advances the input generation, discarding that queued UP. The final release-all safely clears buttons but loses the drop location. The correction preserves only this normal capture-release leave, delivering UP before ordered release-all. Blur, abnormal capture loss and ordinary hover leave retain cancellation; a button held before entering the image cannot create a new remote gesture.

## Changes being checked

- A new authorized button gesture can request normal activation of the actual visible enabled top-level window under the verified pointer. Already-held drags, movement and UP do not request activation. Input-desktop and known integrity checks remain conservative. Windows can deny the request; ordinary SendInput behavior and failure reporting remain in place. Production input does not use AttachThreadInput, forced Z order, restoration or elevation.
- **More actions → Switch remote window (Alt+Tab)** sends release-all, left Alt DOWN, Tab DOWN/UP and Alt UP through the existing authorized serial queue. View-only, paused, ended and stale-generation input cannot use it. Local operating-system shortcut interception does not replace this explicit remote action.
- The owned native regression uses overlapping test windows and exposed hit points without pre-activating each target. It checks activation, actual target client/non-client DOWN and UP receipt, repeated clicks, capture and release. One GUI-less worker queues each complete click while the GUI pumps messages; the caption probe counts messages without entering the system move loop, while client drags retain the real EDIT procedure. HWND/PID and hit-test checks constrain injection to test-created windows, including cancellation and cleanup on an owned foreground. No desktop pixels or actual games are captured.

## Verification

The delayed-IPC outside-release regression failed before correction. Complete Chromium and WebKit synthetic IPC regressions pass afterward: edge-positioned x=1 UP after delayed DOWN, one ordered release-all, rejected external held entry, cancellation on actual active-capture loss and blur, the exact five-event remote Alt+Tab sequence, and discarded stale shortcuts after pause. Existing quality, layout, recovery, termination and FPS checks also pass. TypeScript, the Vite production build and formatting pass; logs are `.local/viewer-input-final-{chromium,webkit,build}.log`.

The Windows module's six portable policy tests on Linux, Rustfmt and all-target Clippy pass. This does not prove cfg(windows) API compilation or native behavior; later Windows CI must provide compilation and owned overlapping-window evidence.

The Windows workflow explicitly selects only `real_input_into_own_foreground_window` with `--ignored --test-threads=1` and a job timeout. It does not enable the separate real desktop-capture test. Ordinary recorded-input unit tests must not invoke real activation.

## Limits and references

Actual Ubuntu→Windows use with the user's game remains pending. A normal EDIT fixture or a modeled capture case cannot establish that a game accepts injected input. Owned windows and the injection worker also share a process, so this cannot prove that a separate game satisfies Windows foreground policy. Excluding WS_EX_NOACTIVATE cannot preserve every application's custom MA_NOACTIVATE response; requesting activation before an explicit click is a behavior change. This does not bypass game filtering, elevated windows, secure desktops, modal disable or Windows foreground restrictions. Existing control grants, sharing/session checks, geometry and input cancellation stay authoritative.

Evidence and implementation links: [native input](../../crates/windows/src/native.rs), [input context](../../crates/windows/src/input_context.rs), [viewer regression](../../scripts/test-remote-viewer.cjs), [PR #2](https://github.com/wanghao9103/farsail/pull/2). Primary API references: [WM_MOUSEACTIVATE](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mouseactivate), [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow), [WindowFromPoint](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-windowfrompoint).
