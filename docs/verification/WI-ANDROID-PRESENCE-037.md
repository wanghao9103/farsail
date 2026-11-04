**English** | [简体中文](WI-ANDROID-PRESENCE-037.zh-CN.md)

# Android presence independent of screen capture

Date: 2026-11-04. Baseline: main 31b9d29. Local candidate: `0.1.9-online-fix`, code 10. No public update feed change.

## Mechanism

The physical PJE110 / Android 16 phone retained its app process while the capture service and wake locks were absent. It was locked and reported offline. The latest recorded sleep reason was power_button, which does not prove the cause of every reported automatic lock. Code inspection confirmed that recurring presence heartbeats only lived in the capture service, so ending recording also stopped renewal of device presence.

An independent, non-exported connected-device foreground service now maintains the authenticated endpoint every 15 seconds. Native presence clears stale hosting capability while leaving capture and automatic approval disabled unless separately authorized. The service has a visible status notification, a persistent opt-out and an identity-scoped sticky-recreation path. Every attempt checks native account/device state; scope mismatch or rejected credentials stops the task. Sign-out clears the pending scope. CPU wake-lock leases renew with a 90-second bound and are released on stop; no screen wake-up or unlocking is performed.

The service declares `connectedDevice`, `FOREGROUND_SERVICE_CONNECTED_DEVICE` and `CHANGE_NETWORK_STATE` for network interaction with peer devices. Capture stays in its separate media-projection foreground service and still requires fresh system consent. See [Android foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types#connected-device).

## Validation

- Rust Android 16 tests and media 7 tests passed, including presence without capture/approval and rejection without device credentials. JVM 7 tests and debug/release builds and lint passed.
- API 36 connection tests passed four cases: 70 seconds of actual screen-off heartbeat renewal beyond the synthetic 60-second lease; opt-out stops renewal and releases the wake lock; recording stop preserves presence but sign-out stops it; saved-scope service recreation rejects another identity; HTTP 403 stops retries. Several assertions belong to the same case.
- The secure-keyguard test passed: actual system revocation ends capture, clears hosting and releases the screen lock while independent heartbeats continue. Existing control/foreground-restoration/setup regression passed four cases. All nine executed device cases passed without skips.
- Initial fixture failures are retained. The startup fixture now waits for the resumed Activity. The secure-lock fixture disables and waits for an old dead accessibility binding before enabling it, preserving its actual service and permission checks.
- Evidence: `.local/android-idle-offline-20261104/rust-tests-final.log`, `.local/android-idle-offline-20261104/connection-tests-final.log`, `.local/android-idle-offline-20261104/secure-lock-test-passed.log`, `.local/android-idle-offline-20261104/access-regression-passed.log`. APK signature, release manifest, non-exported service, absence of debug probes and dual-ABI ELF/ZIP 16 KB alignment are in `.local/android-idle-offline-20261104/apk-verification.json`. Final installation and physical observation are recorded in `.local/android-idle-offline-20261104/installation.json` and `.local/android-idle-offline-20261104/verification.json`.

## Limits

Emulator tests use synthetic coordination. Sticky-recreation coverage checks the saved-scope restart path, not every vendor process-kill policy. Force stop, unavailable credentials, network loss or vendor restrictions can still prevent presence. Keeping the device online does not make a locked screen viewable. The existing eight-hour screen-protection limit is unchanged. Unplugged overnight behavior and all Android 8–15 variants remain outside this validation.
