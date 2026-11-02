**English** | [简体中文](WI-ANDROID-MEMORY-004.zh-CN.md)

# Remembered Android access — 0.1.4

The control choice existed only in native runtime memory and capture consent ended with the process. The app now persists intent for the authenticated server/device, restores allowed control when accessibility really connects, and requests a new visible system capture authorization when reopening the foreground home page after capture ended. A live share is retained. Nothing replays a capture token or grants system accessibility permission.

App/notification stop and recording cancellation clear sharing intent; sign-out clears both. Control revisions prevent a delayed restore from overriding a newer disable; consumed recording request generations and device/server scope prevent stale consent from restarting a stopped or different-device share. Settings can turn off foreground sharing restoration.

Local Kotlin/APK/lint and owned API 35 acceptance passed for actual process replacement by the same-certificate APK, private preference retention without credentials, restored native control, no automatic background recording restart or consent prompt, a fresh foreground system dialog, cancellation without a loop, explicit stop across another replacement, stale restore/consent, foreign-device denial and clearing on sign-out. Phase 1 is deliberately terminated while a real share is live; its crash marker is the expected handoff, not a passing JUnit test. Normal-process UI and state are verified externally; the separate intent-guard test passes normally.

The debug-only read-only state provider requires shell DUMP permission and an emulator. It is excluded from release variants, and publication rejects it in the release manifest. API 36 emulator System UI ANR and crash/force-stop accessibility binding gaps were observed in separate attempts; they are not counted as successful acceptance or resolved by changing system permissions in production. Physical OEM background policy and force-stop service recovery remain outside the verified scope.
