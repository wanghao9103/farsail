**English** | [简体中文](WI-ANDROID-STATE-036.zh-CN.md)

# WI-ANDROID-STATE-036: effective permissions and automatic foreground sharing

Date: 2026-11-04. Scope: Android Kotlin only; no server or desktop changes. Local candidate `0.1.9-state-fix` / code 10, not a public release.

## Problem and implementation

The physical PJE110 / API 36 device retained a true control choice while system accessibility was disabled and ControlService was unbound. The old switch displayed remembered intent while automatic approval was inactive. The switch now requires remembered intent, the system-enabled service, a bound service instance and runtime control. Permission observers, unbind callbacks and refresh-time revocation keep state aligned. Permission restoration reapplies intent; explicit opt-out wins. Incoming control requests without live permission expose recovery instead of an unusable approval action. Automatic approval is only described as active during valid capture and control.

Remembered sharing previously only started from Home or Sharing in the off/interrupted phase, consuming the attempt before its task ran. It now starts from Home, Settings, Sharing and Updates, including a failed attempt on the next foreground visit. Busy operations and navigation do not consume an attempt before consent starts. Returning from consent is not another visit. Existing capture, explicit Stop, cancellation, sign-out and identity fencing retain their behavior. Fresh Android consent remains required; projection authorization data is never persisted or replayed.

## Evidence and limits

- Builds, JVM tests and debug/release lint: `.local/android-oppo-sleep/verified-build-resumed.log`; five JVM tests passed, covering permission combinations and existing image geometry.
- The initial permission/restoration device checks passed two tests. Failure-loop and authorization-navigation coverage was added for the final run in `accepted-device-tests.log`. These run only on an owned API 35 emulator, using synthetic coordination and real JNI, Settings and recording consent.
- Restart regression: `memory-final.log` records the initial real-process replacement, cancellation, Stop and identity guards. The final-source run is in `memory-accepted.log`.
- The release candidate matches the installed and repository-pinned certificate. Package/version, non-debug status, absence of a debug provider, both native ABIs and ELF/ZIP 16 KB alignment were checked in `apk-verification.json`. Installation and the final summary are in `installation.json` and `verification.json`.

Physical installation does not prove production-account cross-device approval after the user grants accessibility, unplugged long-term background behavior, why vendor authorization was revoked, or silent recording restoration after secure locking. The system recording-termination boundary on lock is unchanged.

Final results: five JVM tests, four device tests, real-process restart and intent guards, debug/release builds and lint passed. Same-certificate install -r succeeded on the physical phone; installed APK and deliverable hashes match, first-install time was retained and MainActivity started. The physical phone still had keyguard showing=true; no unlocking or recording/accessibility confirmation was performed. Listed validated checks passed; overall verification remains partial. Earlier startup failures and tool conflicts are retained separately. The final fixture waits for unlock, drains serialized refreshes and verifies the current fixture server before locating Home.
