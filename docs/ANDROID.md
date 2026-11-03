**English** | [简体中文](ANDROID.zh-CN.md)

<a id="android-host"></a>

# Android app preview

The native Android app can control a computer or another Android phone, and can share its own screen with a FarSail computer or phone controller. Mobile previews are distributed through the repository Android release channel. It reuses the existing coordinator, Ed25519 device identity, explicit approvals, encrypted iroh transport, JPEG frames and expiring authorization leases. No coordinator schema change is required.

## Install and connect

Upgrade the coordinator from this source before using the phone app: earlier coordinators recognize Android device identities but restrict live screen hosting to Windows and reject Android sharing with HTTP 409. This change permits Windows and Android hosts while retaining generation, live-device and approval checks; no new database migration is needed. This work does not deploy the production server automatically.

Install the APK from an Android release or a local build. The minimum supported system is Android 8; the APK contains ARM64 and x86_64 libraries. Older 32-bit phones are not included.

- Save the desktop HTTPS service address under Settings → Server settings, then return to sign in. Login, registration and email verification use independent pages; verification contains only email and code. Add this phone after signing in.
- For viewing, tap “开启屏幕共享” and confirm the Android system screen capture dialog. Full screen capture is requested so that remote touch coordinates match the physical screen.
- For control, tap “允许同账号设备直接控制”. If authorization is missing, the app attempts FarSail details, or locates “FarSail 远程控制” when the system restricts direct access. Confirm the system service switch and return to complete activation. Enabling accessibility alone grants no connection.
- With control enabled, same-account computer/phone view/control requests are automatically approved; other accounts still require phone confirmation. Control off, capture stop or accessibility disconnection cancels automatic approval immediately. Both ends show a connection verification code.
- Mouse clicks map to taps; dragging maps to a bounded swipe after release; the wheel scrolls. Right click and Escape go back. The updated desktop toolbar exposes Back, Home and Recents. Unicode text can be sent to the focused ordinary editable field.
- Different desktop viewer windows can connect to different phones or computers concurrently. Each has independent frame, input, recovery and close state. The existing native client limit is sixteen transport sessions; a phone accepts one viewer at a time.

Use “停止共享” in the app or notification to stop capture and invalidate queued input immediately. A gesture already accepted by Android can finish within its bounded duration of 1.5 seconds. Password fields, arbitrary desktop shortcuts, multitouch, audio and mobile file transfer are outside this preview.

Home distinguishes off, waiting for system consent, starting, sharing and failed. Sharing is shown only after server acceptance. Failure details survive service cleanup callbacks; Android HTTP 409 from old servers requires the [Android coordinator upgrade](https://github.com/wanghao9103/farsail/releases/tag/coordinator-android-026491f).

## Control another device from your phone

After sign-in and binding, choose “打开我的设备” on Home. Online devices with screen sharing enabled offer Control and View only. The phone acting as controller needs neither screen capture consent nor accessibility permission; those belong to the controlled device. Existing same-account approval choices and per-connection permissions remain in force. This controller feature requires no additional coordinator change.

The remote view fits the source without stretching. Touch to click or drag; enable scrolling mode to scroll, use right click or Keyboard for text, Enter, Backspace, Tab, Escape and common shortcuts. Android peers additionally expose Back, Home and Recents. Rotate view changes only the controller's displayed image and inverse touch coordinates, not the controlled device's orientation. Multiple source displays can be selected when advertised.

A phone controller owns one foreground connection. Exit, background, cancellation and sign-out close it and invalidate queued input; a stopped connection requires an explicit reconnect. View-only grants never send input. Frames use a latest-value slot; input is bounded, ordered, checked against the displayed layout, active lease and current input generation. Local screen sharing and an outgoing controller share the authenticated endpoint without sharing capture authorization. Stopping local sharing does not end the outgoing controller.

## Background operation

Switching apps, pressing Home, or removing the Activity task keeps the explicit media projection foreground service running. The service owns capture, transport polling and a partial CPU wake lock. On Android 13 or later the app does not request notification permission: without permission the foreground service appears in the system Task Manager, while its notification is absent from the notification drawer. Enabling notifications in system settings is optional. Its system notification is silent, has no sound or vibration, and disappears when sharing ends. Android requires a foreground service notification for sustained background screen capture; the app does not hide it or restart capture from boot.

Some manufacturers restrict background operation. “后台运行设置” opens the app settings so the user can allow background activity. Force stop, revoked screen capture, process death and system lock behavior can end sharing. Consent is never replayed after process death; reopen the app and confirm a new capture session. Accessibility disconnection closes control sessions while viewing can remain available.

Official platform requirements: [media projection](https://developer.android.com/media/grow/media-projection), [foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types), [notification permission](https://developer.android.com/develop/ui/compose/notifications/notification-permission), [accessibility service](https://developer.android.com/guide/topics/ui/accessibility/service).

## Build and verify

Use Rust 1.93, JDK 17 or newer, Android SDK 35, NDK 27.2.12479018 and the checked-in Gradle wrapper. Set the SDK/JDK environment variables for your machine:

```sh
export JAVA_HOME=/path/to/jdk
export ANDROID_HOME=/path/to/android-sdk
rustup target add aarch64-linux-android x86_64-linux-android
scripts/build-android.sh
```

The output is `apps/android/app/build/outputs/apk/debug/app-debug.apk`, signed with the local Android debug key. This is an installable testing package. Public previews are built by `android-release.yml`, signed with the original installed certificate and have debugging disabled. Preserve that certificate for in-place upgrades. The script links native libraries with 16 KB ELF alignment and runs Android lint and JVM tests. Gradle refuses to package missing native ABIs. See [Android page size requirements](https://developer.android.com/guide/practices/page-sizes).

```sh
cargo test -p farsail-android
cargo clippy -p farsail-android --all-targets -- -D warnings
cd apps/android
./gradlew connectedDebugAndroidTest
```

For actual process-restart acceptance, build the debug APK and test APK, then run `ANDROID_SERIAL=emulator-5554 python3 scripts/test-android-memory.py` on an owned emulator. It uses a local synthetic coordinator and same-certificate APK replacement, verifies a new process, fresh visible consent, retained control, cancellation, explicit stop and stale/different-device denial. Its shell-permission-protected state probe exists only in debug builds and refuses physical phones. The publication helper rejects a release containing that provider.

Instrumentation tests require a dedicated emulator and skip physical phones to preserve real account state. For the full emulator media/input probe, start an API 35 emulator and build both debug APKs, then run from the repository root:

```sh
cargo build -p farsail-android --example device_probe
(cd apps/android && ./gradlew :app:assembleDebugAndroidTest)
python3 scripts/test-android-emulator.py
```

The driver clears the dedicated emulator app's synthetic test data, confirms the system capture dialog visibly, and checks both complete JPEG decoding and real mouse/text input. Do not use it on a physical phone.

The host tests use real local encrypted QUIC connections to verify two independent phone sessions, frame ACKs, ordered input, view-only denial, stale geometry and cancelled capture activation. Instrumentation uses synthetic local coordinator responses and the real Keystore/JNI/projection foreground service. Optional `realPeer` instrumentation connects a host probe through the emulator UDP redirect and exercises text input into an owned test field. These tests do not prove physical phone compatibility, production account coordination or cross-network relay behavior. Verification results and remaining gaps are recorded in [the implementation evidence](verification/WI-ANDROID-001.md).

## Automatic screen timeout and manual locking

During an explicitly started share, “Prevent automatic screen timeout while sharing” defaults to on. A screen-dim wake lock is scoped to the projection foreground service so it also applies while another app is visible. It can be turned off immediately; stopping or failing capture releases it. It does not wake or unlock the phone. Screen and CPU wake locks have an eight-hour upper bound. Keeping the display on increases battery use.

Manual locking is different from switching apps. On Android 15 QPR1 and newer, the system ends MediaProjection on lock; the revoked token cannot be reused or restored silently. The app remembers control and sharing intent for the current signed-in server/device. A live background share remains active when returning to the app. If capture has ended, opening the home page in the foreground can request fresh system consent directly, once per visit; no capture token is stored. Settings → Restore previous sharing when opening the app can disable this prompt. Merely unlocking into another app does not start capture.

The control choice is restored only after authenticated device recovery and an actual accessibility-service connection. A saved switch is not proof of a live system permission. The app does not enable accessibility settings itself. App/notification Stop, cancellation of a recording request and sign-out cancel the applicable saved intent; system capture termination preserves the choice for a later foreground confirmation. Delayed recording results and queued control restores are fenced by intent generations and server/device identity. On older releases, screen-off behavior depends on the system; a live projection is not stopped solely because the screen goes off.

Since 0.1.8, enabling control saves the choice before opening system accessibility settings, including the setup-page action. If the process is replaced during authorization, the choice remains enabled and waits for the real service connection. Sharing intent is saved before requesting recording consent. A successful result waits in Activity memory for authenticated account state to finish restoring, and is still rejected after Stop or an identity change. The home page distinguishes remembered sharing awaiting restoration from an active recording. Older choices that were never saved need one explicit enable after upgrading.

References: [Android MediaProjection lifecycle and lock auto-stop](https://developer.android.com/media/grow/media-projection), [PowerManager screen-dim wake lock](https://developer.android.com/reference/android/os/PowerManager#SCREEN_DIM_WAKE_LOCK). The latter is deprecated in favor of a visible window flag for ordinary activities; this service needs to cover background sharing across apps and owns its explicit cleanup.

## Software updates

Settings → Software update separately manages checks, downloads and installation. Automatic checks default to enabled, throttled to once per six hours; automatic downloads default to disabled. Installing a downloaded update requires a tap and confirmation in the Android package installer. Stop sharing before installation. Android 8 and later require the normal per-app permission to install applications.

The update feed is separate from the account server and fixed to repository `android-updates/latest.json`. Rust verifies its embedded Ed25519 public-key signature and increasing version code. Download verification checks size, SHA-256, package identity, version and the installed APK certificate. Keys remain in Actions secrets, never in committed files. Version APKs belong to immutable `android-v<version>` releases. Separate build and publish jobs allow publication retries to reuse the signed artifact without rebuilding a published version.

For a local signed build set `ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS`, then run `scripts/build-android.sh :app:assembleRelease :app:lintRelease`. Before publication verify `apps/android/version.json`, certificate and source tag. `scripts/configure-android-updates.mjs` checks key consistency by default; `--apply` configures repository secrets. Keep the original certificate so users can upgrade without uninstalling and losing data.

If the Actions token receives HTTP 403 when creating release entries, preserve the signed artifact, create the version/channel entries with existing release permissions and rerun only failed publication. Do not rebuild or overwrite version assets or copy a personal token into repository secrets to work around this.

## Code boundaries

`apps/android` contains the native Kotlin UI, Android Keystore AES-GCM storage, media projection foreground service and accessibility input adapter. Credentials stay in native storage protected by a non-exportable Android Keystore key; backup is disabled. The service is not exported, and access to the accessibility service requires the system binding permission.

`crates/android` provides the JNI bridge and host runtime using existing FarSail client/transport/media crates. Capture generations, layout versions and live session leases are checked again before Android applies a queued action. Frames use a latest-value slot, at most two in-flight frames, ACK deadlines and a payload budget. Stopping locally wins over a delayed server response. The authenticated peer message identifies Android navigation support; older desktop clients can still receive baseline frames and input without the new toolbar buttons.
