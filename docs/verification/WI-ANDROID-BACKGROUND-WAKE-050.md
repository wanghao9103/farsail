**English** | [简体中文](WI-ANDROID-BACKGROUND-WAKE-050.zh-CN.md)

# Android background screen wake

2026-11-11. Follow-up to the Android candidate refresh in WI-049.

On the USB-connected OPPO R17 (Android 10, ColorOS V7.1), the capture projection and accessibility service remained active while the physical display was asleep. At 22:44, system logs repeatedly rejected `app.farsail.android.WakeActivity` with `allowStart = false`, `firewall intercept = true`, and `reason = default` while the launcher was the top activity. Earlier foreground attempts were allowed. A connected desktop session consequently received no frames. One successful foreground wake did not establish background reliability.

On Android 8.1–10, an approved controller now requests the existing bounded screen wake directly from `ControlService` before attempting the background activity. Both operations check the live session, epoch and layout authorization. A revoked lease cannot launch a window; the service wake lock expires after 1.5 seconds and is released on cancellation or disconnection. Requests are throttled to once per second. Later Android versions retain the existing activity behavior. Credential locks are never dismissed.

Local validation in an ASCII temporary checkout passed 13 JVM tests, debug lint (zero errors, 49 warnings), and instrumentation-test Kotlin compilation. The four dispatch regression tests cover blocked background activity, rejected authorization, revocation between wake and launch, and bounded retry. Running JVM tests in the original non-ASCII Windows path failed to load every test class; the same sources passed in the temporary path. No instrumentation fixtures ran on a physical phone.

The candidate workflow produces `0.1.17-wake1` using the original installed signing certificate. When all native inputs match commit `789dc7a175e262e6aa8aad840a267981176050a8`, it verifies the exact APK SHA-256 from successful run `43033361498` and reuses its two native libraries. Changed native inputs require the full Rust build and checks. Kotlin tests, release lint and instrumentation compilation still run for every candidate.

Physical acceptance is pending: manually approve screen recording after the update, send FarSail to the background, press the physical power button once, and reconnect through an approved control session. Verify both screen wake and received desktop frames over repeated attempts. ADB wake or unlock commands and synthetic credential-bearing drivers are excluded from this acceptance test. This candidate is not a public updater release.
