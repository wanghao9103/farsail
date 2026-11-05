**English** | [简体中文](WI-VIEWER-TABS-039.zh-CN.md)

# Concurrent viewers, session tabs and retained window geometry

Date: 2026-11-05. Baseline: main `e5bddf0`. Initial validation used desktop 0.1.35 / Android 0.1.10 before publication; release results are below.

## Changes

- New outgoing viewers join one native window as tabs. Each retains its own native session binding, cancellation, recovery instance, display selection and input generation. The application menu can open subsequent connections in separate windows.
- Click a tab, use arrow keys on the tab strip, Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+PageUp / PageDown or Ctrl+1…9 to switch. Ctrl+W / the tab close button ends only that connection. The native window close button ends all its tabs.
- Inactive tabs stay connected and retain their latest picture, but pause frontend frame polling/decoding. Native transport continues media acknowledgement. Switching away invalidates queued input and releases held input in order; selecting another phone cannot receive the previous phone's keys.
- Automatic phone sizing is owned by the native window lifetime. Fresh session IDs after recovery, tab changes and remote orientation changes cannot overwrite the window's chosen geometry. Local phone display rotation survives recovery.
- Status polling permits one outstanding request per viewer, preventing overlapping work under load. The baseline already contains independent transport lease validation, fresh generation-bound proof caching, decoded-image presentation and removal of the coordinator's remote request quota; these fixes were retained and exercised rather than replaced with relaxed authorization.
- Android keeps its existing immersive canvas, floating controls and remote orientation following. The first frame hides tools after 700 ms. Added enlarge/reduce actions and an explicit fill-screen option; cropping, pinch/pan and touch inversion share the same geometry. Empty viewer intents do not start a connection.

## Verification

Evidence directory: `.local/viewer-tabs-20261105/`.

- TypeScript / Vite production build passed (`build-final.log`). Display and viewer-notice policy tests: 10 and 7 passed.
- Desktop Rust library: 52 passed, one unrelated interactive test ignored (`rust-desktop-final.log`). Client library: 54 passed, one platform keyring test ignored (`rust-client.log`). Desktop all-target Clippy with denied warnings passed (`clippy-final.log`); workspace formatting and diff checks passed.
- Isolated PostgreSQL and real coordinator / authenticated transport: one complete integration test passed, no skips (`coordinator-flow.log`). Two independent targets exchange media/ACKs through lease rotations and one peer's restart/reconnect.
- Complete Chromium viewer suite passed (`ui-complete.log`), including three tabs, active-session key routing, keyboard switching, independent tab closure and the existing input/rotation/decoded-frame/recovery checks. Additional narrow-window checks passed (`tab-probe.log`); screenshots are `desktop-tabs.png`, `phone-tabs.png` and `.local/ui-verification/viewer-three-tabs.png`.
- Native Linux Xvfb/Openbox probe passed (`native-retry/report.json`): 23 prohibited command paths remain denied; maximize, fullscreen, cascade, minimize and main-window isolation passed. After the window was explicitly changed to 960×600, an automatic portrait-layout request retained 960×600. The first probe hit frontend startup timing and is retained; the probe now waits for the actual titlebar.
- Android debug/APK/test APK, JVM tests and lint passed (`android-final-build.log`). Four geometry tests cover fitted, rotated and filled/cropped touch mapping, invalid geometry and zoom/pan.
- Owned API 35 emulator: one immersive layout instrumentation test passed (`android-immersive-verified.log`). Canvas bounds remain equal to the root when tools open/close; zoom, fill and reset preserve those bounds. This is a synthetic bitmap layout test, not a remote-phone connection. Initial failures captured System UI ANR and the system's first-use fullscreen education overlay; the test now waits for/acknowledges that specific education overlay. A failed concurrent Gradle intermediate-file attempt was followed by a successful sequential clean build; failure logs remain available.

## Limits

The reported overnight versions and logs were unavailable. The two-target integration is synthetic and the native window probe runs on Linux. Actual Windows multi-phone traffic, a physical phone controller, long overnight use, vendor background restrictions and wide-area network behavior have not been accepted in this run. No production coordinator was modified, no package was published, and no personal-device install was performed.

## Published releases

Desktop [0.1.36](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.36) uses source `cf0b0fb0fe2ef8b66cc8e56e4c580de67384889e`; [release workflow](https://github.com/wanghao9103/farsail/actions/runs/41958143325) succeeded. All 12 public assets, three architecture signatures, source metadata, 11 checksums, native tray reports and the ordinary fixed feed were verified; the publication was reconstructed byte for byte. Only failed publication was retried after the fixed URL matched; original package jobs and version asset identities were retained. Evidence: `.local/client-public-0.1.36/verification.json`.

Android [0.1.12](https://github.com/wanghao9103/farsail/releases/tag/android-v0.1.12), code 13, uses source `82066f9fc8687ee0beb33af0b2f77361148b2228`; [release workflow](https://github.com/wanghao9103/farsail/actions/runs/41959560575) succeeded. All 4 public assets, signed metadata, original certificate, both native ABIs and fixed feed passed verification. A final owned-emulator update from code 1 to code 13 used automatic in-app download and the system installer; the installed APK matched the public hash and data/identity continuity passed. See [phone update acceptance](WI-ANDROID-UPDATES-040.md).

Additional simulation found and fixed shared status-request overlap and an Android input-release request sent before connection readiness. The final three-phone stress ran 60 switches and 6 isolated recoveries with at most one status request in flight per session. The mobile controller test deliberately delayed peer lookup by 450 ms and passed real JNI/QUIC, static-frame control, rotated input, view-only denial and cancellation. An installation-requirement reminder fix was published as Android 0.1.12; the earlier 0.1.11 candidate was cancelled before publication, without moving its tag. Four pre-existing Markdown formatting issues were corrected in a separate documentation commit. Production coordinator deployment and physical overnight tests remain outside this run.
