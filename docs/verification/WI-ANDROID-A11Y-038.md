**English** | [简体中文](WI-ANDROID-A11Y-038.zh-CN.md)

# Accessibility screenshot sharing

Date: 2026-11-04. Baseline: `b427d00`. Ordinary-install candidate: `0.1.9-a11y-preview2`, code 10. No public channel release.

## Mechanism and choice

On Android 11+, the user may select `AccessibilityService.takeScreenshot`, with `canTakeScreenshot` declared in service metadata. System projection remains the default. The user explicitly chooses screenshots after reading capture, battery and protected-page information, then grants system accessibility access if needed. Capture targets a 350 ms cadence including capture/copy/encoding, adapting down on system rate limits; hosting is advertised only after a successful frame. An independently stoppable `connectedDevice` foreground task provides notification controls without creating a `mediaProjection` session. Reference: [accessibility screenshot API](<https://developer.android.com/reference/android/accessibilityservice/AccessibilityService#takeScreenshot(int,%20java.util.concurrent.Executor,%20android.accessibilityservice.AccessibilityService.TakeScreenshotCallback)>).

The selected method and sharing choice are scoped to the server/device. Accessibility alone neither starts sharing nor approves connections; direct control requires its separate choice. Sign-out, explicit Stop and identity changes cannot restore old intent. Permission revocation stops capture and invalidates local capability immediately; foreground return after permission recovery may retry. Screenshot restoration has no projection-consent dialog.

Hardware screenshots are copied to software RGBA, bounded to a 2560-pixel longest edge and passed through native full-resolution JPEG 4:4:4 encoding. Capture epoch and geometry are checked atomically at publication, so late frames cannot resize a new capture or publish into a new identity. Hardware buffers are always closed; late callbacks and stopped frames are rejected. The existing system-projection path is retained.

Screen wake uses ordinary window APIs through a non-exported entry that rechecks screenshot sharing, accessibility and the current native control session’s capture/layout epochs. The first action wakes without replaying coordinates; the window exits automatically without requesting dismissal of a password lock. View-only permission cannot enter this path.

The `onInterrupt` callback interrupts accessibility feedback rather than revoking permission. It clears gestures without suspending valid control/capture; actual permission loss and unbinding still revoke capability. Reference: [accessibility service lifecycle](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService#onInterrupt()).

## Verification evidence

- A raw API probe used an owned API 36 emulator and a synthetic solid-color Activity, without a projection session. Screenshots succeeded unlocked, asleep with a secure lock, awake while still locked, and unlocked again; the `FLAG_SECURE` page was black. This probe is not remote-input or physical-vendor-device acceptance.
- Native tests cover rejection of old-epoch geometry changes and publication. Build, sharing lifecycle and regression results are recorded in `.local/android-lock-followup-20261104/verification.json`; the raw probe log is `.local/android-lock-followup-20261104/accessibility-probe-test-final.log`.
- Initial probe-unlock fixture and standalone debug-service lint failures are retained. The corrected fixture uses the declared control service and a visible synthetic Activity. It clears only credentials created by that test, without changing physical-phone lock, account or permission settings.

- Rust: 17 Android tests and 7 media tests, plus Clippy and formatting, passed. JVM: 7 tests passed. Debug/release builds, lint, original certificate, both ABI symbols and 16 KB alignment were checked; debug probes are absent from the release APK.
- API 36 regressions passed: raw capture/protection probe; screenshot lifecycle including rotation, lock, actual revocation, foreground restoration, explicit Stop and identity scope; legacy projection and control state. The latest feedback-interruption/control run passed all 4 cases.
- A separate authenticated QUIC controller running inside the owned emulator decoded native frames, woke the sleeping device while its secure lock remained, then delivered text/click input to an owned fixture after the fixture locally unlocked its own test credential. This does not cover host UDP/NAT: API 36 emulator forwarding failed, so the probe ran on the guest. Readiness handshake replaced a flaky fixed-delay focus fixture.
- The physical PJE110 user confirmed foreground picture and control before preview2. Preview2 was installed without uninstalling or clearing data. At 15:25 the actual screenshot backend published native 1080×2376 frames: capture 23 ms, native encode 51 ms, interval 350 ms. These are processing samples, not measured end-to-end latency.
- Physical OEM battery management independently selected smart background optimization despite standard battery exemption. Through normal Settings UI, Full allowance for background behavior was selected and confirmed. Accessibility settings state that enabling this service grants auto-start independently. Temporary USB stay-awake was restored to its original off value before idle observation; no PIN or timeout was changed. At 15:26 the physical USB device disappeared and the last heartbeat had capture=false; its cause is unresolved. Sustained idle and physical lock recovery remain pending.

## Boundaries

Screenshot capability does not bypass device passwords or protected pages. Vendor app stops and network restrictions may still interrupt service; adding this backend does not turn the earlier physical-device failure after power exemption into a pass. Android 8–10 retain system projection. Physical vendor compatibility, prolonged unplugged operation, remote input on the lock screen and production release are separate checks, without a pass claim before verification.
