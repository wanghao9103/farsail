**English** | [简体中文](WI-ANDROID-LOCK-003.zh-CN.md)

# Android idle protection and lock recovery — 0.1.3

The projection service previously held only a partial CPU wake lock. That does not prevent automatic display timeout. A separate screen-dim wake lock now covers an explicitly started share across apps; a visible home switch defaults to enabled and can release protection immediately. Both locks are bounded to eight hours and released on stop/failure. The implementation never wakes or unlocks the phone.

Android 15 QPR1 and newer revoke MediaProjection at lock. The onStop handler now saves an interrupted state and lock-specific guidance instead of replacing it with generic off during service destruction. Recovery obtains a fresh system token, and the remote-control preference is retained. Unlocking does not automatically restart sharing or open a consent dialog.

Local evidence: Kotlin/APK build and lint; API 35/36 emulator projection, background/task removal and rejected-coordinator failure-state tests; idle beyond a short system timeout in another app, live wake-lock inspection, disabling protection, explicit sleep and cleanup. The lock/recovery test uses an Android 16 emulator with a temporary known credential, restores its original settings and clears only that fixture credential. A blank emulator without a secure lock may keep capture after screen off, so it cannot reproduce a secured physical phone's lock path. Native framework callback logs record sharing → interrupted → fresh system consent → sharing.

Real account data and physical-phone settings are excluded from instrumentation. Physical device compatibility, vendor battery controls and eight-hour endurance are not claimed by these tests. Publication and installed version verification are recorded separately after CI.
