**English** | [简体中文](WI-ANDROID-LOCK-003.zh-CN.md)

# Android idle protection and lock recovery — 0.1.3

The projection service previously held only a partial CPU wake lock. That does not prevent automatic display timeout. A separate screen-dim wake lock now covers an explicitly started share across apps; a visible home switch defaults to enabled and can release protection immediately. Both locks are bounded to eight hours and released on stop/failure. The implementation never wakes or unlocks the phone.

Android 15 QPR1 and newer revoke MediaProjection at lock. The onStop handler now saves an interrupted state and lock-specific guidance instead of replacing it with generic off during service destruction. Recovery obtains a fresh system token, and the remote-control preference is retained. Unlocking does not automatically restart sharing or open a consent dialog.

Local evidence: Kotlin/APK build and lint; API 35/36 emulator projection, background/task removal and rejected-coordinator failure-state tests; idle beyond a short system timeout in another app, live wake-lock inspection, disabling protection, explicit sleep and cleanup. The lock/recovery test uses an Android 16 emulator with a temporary known credential, restores its original settings and clears only that fixture credential. A blank emulator without a secure lock may keep capture after screen off, so it cannot reproduce a secured physical phone's lock path. Native framework callback logs record sharing → interrupted → fresh system consent → sharing.

Real account data and physical-phone settings are excluded from instrumentation. Physical device compatibility, vendor battery controls and eight-hour endurance are not claimed by these tests. Publication and installed version verification are recorded separately after CI.

## Publication and physical installation

Source `dbe049ce9fe70281eac2a5f035930c15e0724efc`; [CI 41417163764](https://github.com/wanghao9103/farsail/actions/runs/41417163764) passed after a publication-only retry, without rebuilding or changing the four immutable version assets. All four assets, including the full APK, were downloaded anonymously and matched API digests. The signed canonical feed equals the versioned metadata; Ed25519 signature, package/version, original APK certificate, non-debuggable flag and 16 KB ZIP alignment passed.

[Android 0.1.3](https://github.com/wanghao9103/farsail/releases/tag/android-v0.1.3), versionCode 4. APK SHA-256: `2bcf7cf5bb362d0abc54f2c36c962bf0dee829f3920e45b67368ffd09efc0328`. Replacement installation on the connected Android 16 phone returned Success, installed APK hash matched, and original first-install time was retained; no data was cleared. The phone was locked at final inspection, so physical projection consent and lock recovery were not exercised.

Evidence: `.local/android-public-0.1.3/final-verification.json`. The retained control choice was tested across service lock/stop within the same process; persistence across process restart or installation is not claimed. An additional Android 16 full media probe could not establish its emulator UDP redirect because the console returned 0xff; it did not start the desktop probe. Existing older media logs are not counted as this run's success.
