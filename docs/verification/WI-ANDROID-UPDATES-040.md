**English** | [简体中文](WI-ANDROID-UPDATES-040.zh-CN.md)

# Phone updates without a computer

Date: 2026-11-05. Candidate: Android 0.1.12 / code 13, desktop 0.1.36.

## Behavior

Android already verified signed update metadata, APK hashes, package identity, increasing version codes and the installed signing certificate before opening its system installer. This change exposes a non-modal update reminder on the home, settings and login pages, including download progress and an install action. Remote viewing and system consent screens are not interrupted.

Automatic checking remains enabled by default, at most once per six hours after success. Failed automatic checks wait at least 15 minutes before retrying; an explicit check can retry immediately. Automatic downloads default to enabled on non-metered Wi-Fi. Previously saved choices remain authoritative, and the update settings offer separate automatic-check, automatic-download and Wi-Fi-only switches. Manual downloads are independent of the Wi-Fi preference. Returning to the app on eligible Wi-Fi can start a previously offered download.

A downloaded APK and its original signed metadata are reverified on process restart, allowing the install reminder to recover offline. Installation always reverifies the metadata/package and uses Android's permission page and package installer. Active screen capture prevents installation. Release notes are read from the source-controlled Android release notes file rather than reusing an old feature description.

## Validation

Evidence: `.local/release-tabs-20261105/`.

- Android Rust tests, including signed version monotonicity and authenticated update boundaries, passed (`android-rust.log`). JVM tests, debug/test APK builds and lint passed; the update reminder instrumentation test passed (`mobile-ui.log`, with the separate layout failure retained).
- A source-compatible test baseline (version 0.0.1 / code 1, original installed certificate) downloaded the actual public signed Android 0.1.8 / code 9 update through the app, verified it and opened the normal unknown-source permission page and system installer. `public-updater-prepare.log` and `update-prepared.json` record the result.
- After process termination and both emulator networks were disabled, the home install reminder restored from the verified cache, without an online check (`offline-cache-ui.xml`). The cached package reopened the system installer offline (`offline-installer-ui.xml`).
- The normal system installer completed the update. An instrumentation check against the installed code 9 confirmed that the continuity marker in app preferences survived (`public-updater-verify-retry.log`). The first verification was started while Android still held the package frozen during installation; that failed startup was retained rather than counted as an application failure.
- The tests use the owned `farsail-memory-api35` emulator. No personal device was installed, uninstalled or cleared. The initial baseline's manifest and APK are retained under `update-baseline/`; no production update channel was changed for this test.

## Remaining release acceptance

The published 0.1.12 APK, signature, source provenance, fixed feed and an update to that exact release are checked after publication. Physical vendor background policies, arbitrary captive/metered networks and unattended background downloads are outside this test. This implementation checks when the app starts or returns to the foreground; it does not promise a permanently running background updater or silent package installation.
