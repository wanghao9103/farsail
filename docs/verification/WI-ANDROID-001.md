**English** | [简体中文](WI-ANDROID-001.zh-CN.md)

# WI-ANDROID-001: native Android host and concurrent desktop viewers

Date: 2026-11-02. Status: partial; native emulator transport accepted and initial preview installed on a physical phone. The in-app upgrade passed on an emulator; new-version physical installation and production coordination remain unaccepted.

## Implemented

- Native Kotlin Android app, ARM64/x86_64 JNI library, Android Keystore AES-GCM credential storage and explicit account/device binding.
- Full-screen MediaProjection foreground service, latest JPEG frames over existing authenticated transport, ordered accessibility input and local consent/approval gates.
- Background operation independent of the Activity. No automatic capture restoration after process death. Android 13 and later do not receive an app-initiated notification permission prompt; the platform still exposes the active foreground service in Task Manager. Earlier systems use a silent notification.
- Desktop navigation actions for Android. Different viewer bindings keep their session scope, recovery and cancellation independent; one phone admits one viewer, while the desktop retains the existing sixteen-session limit.

- Independent account pages: login, registration, email verification, server settings, app settings and updates. Async completion is scoped to the originating page and navigation generation.
- Signed Android updates: a fixed independent feed, Ed25519 metadata, APK size/digest/package/version/installed-certificate checks, automatic checks by default and normal system installation confirmation.

## Verification

- Rust host tests: 8 passed, including two real encrypted QUIC phone sessions, independent input and shutdown, frame ACKs, denied control on view-only sessions, reordered/replayed inputs, stale geometry and cancelled activation.
- Android host Clippy with warnings denied: passed.
- Desktop Rust: 50 passed, 1 ignored; the ignored test requires an owned interactive Windows environment. Includes concurrent-window access scope and recovery isolation.
- Frontend TypeScript/Vite: passed. Pure viewer/notice/feedback state tests: 21 passed. Real browser viewer regression: passed, including Android navigation key pairs and removal in view-only mode. These browser tests use synthetic IPC.
- ARM64/x86_64 native release builds: passed. ELF load segments use 16 KB alignment. Android debug APK and instrumentation APK build: passed. Android lint: passed with no errors; dependency, launcher-icon and compatible-platform warnings remain.
- Android 15 / API 35 x86_64 emulator: passed real Keystore encryption/tamper rejection, JNI binding, visible system capture consent, Home/background operation, Activity task removal, authenticated QUIC JPEG reception and full pixel decode at 576×1280, text input and mouse click into owned fixtures, local stop and notification cleanup. Notification permission remained ungranted. The device test uses synthetic coordinator responses.
- The emulator UDP redirect completed handshake/input but timed out on media with segmented UDP sends; disabling Android UDP segmentation restored real media reception. Desktop transport settings are unchanged. Physical-phone network behavior remains unverified.
- Real PostgreSQL coordinator suite: 5 passed, including Android live capability enable, stale-generation rejection, pending approval, wrong-device approval rejection, separate Files capability and grant invalidation on screen-sharing stop. Earlier coordinator builds must be upgraded because their capability API only allows Windows screen hosts. No database migration was added.
- APK Signature Scheme v2 verification and 16 KB ZIP alignment: passed. Android 0.1.1 is publicly released as a non-debuggable APK, retaining the original installed certificate. Separate build/publish jobs completed successfully in Actions run 41389326495.

Local logs and screenshots are under `.local/android-verification`. The reproducible emulator driver is `scripts/test-android-emulator.py`; it refuses physical devices, uses synthetic coordination responses, forwards only an emulator UDP port, and verifies the real JNI/Keystore/projection/accessibility/QUIC media path against an owned input fixture. Production account approval policy continues to reside in the existing coordinator.

- Modular-page UIAutomator test: passed. Login/registration exclude server/code fields, verification excludes passwords, server settings contain only connection settings; Back retains the email and updates have their own page. Actual 1080×2376 screenshots were inspected.
- Rust update-trust tests cover valid signatures, tampering, wrong keys, version and download-origin boundaries. Debug/release builds and lint passed.
- OPPO / Android 16 / ARM64 initial versionCode 1 installation and launch were confirmed. This does not accept physical-phone media or new-version upgrade behavior.

- Public Android 0.1.1 updates: Ed25519 metadata, canonical/versioned feed equality and all 4 API asset digests passed. The dedicated emulator bootstrap code 1 used production update code to download public code 2, verify the APK, enable the normal source setting and confirm system UPDATE. Same-certificate instrumentation verified installed code 2, version 0.1.1 and retained test preferences. The initial download failed and a retry succeeded; the initial failure was not attributed to APK corruption.
- Update instrumentation requires explicit `-e publicUpdate true` and `app.farsail.android.UpdateFlowTest#publicDownloadAndSystemInstaller`. Build the bootstrap with Gradle `-PfarsailVersionCode=1 -PfarsailVersionName=0.1.0`; run `-e phase verify` after installation to check preserved data. Dedicated emulator only; never downgrade or clear a physical phone.

## Remaining boundaries

Physical Android manufacturers, long-running battery behavior, Android 8–14 and 16 KB device runtime, production HTTPS relay, cross-NAT, mobile file transfer, audio, multitouch and arbitrary desktop shortcuts have not been accepted. Password inputs are excluded. An already-dispatched Android gesture can finish within 1.5 seconds after cancellation; pending actions are invalidated immediately.

See [Android usage and build instructions](../ANDROID.md).
