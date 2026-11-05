**English** | [简体中文](WI-ANDROID-BACKGROUND-042.zh-CN.md)

# Android background and lock-screen settings shortcuts

Date: 2026-11-05. Unreleased change on top of Android 0.1.12 / code 13. The already-installed public 0.1.12 does not contain this new guide.

## Behavior

Settings, sharing settings and the bound-device home screen expose “Lock screen and background settings”. The home link stays available even when the standard battery exemption is already enabled, because that does not establish vendor background permission. The page identifies the actual device/Android version and distinguishes presence from usable screen capture and input.

Battery exemption uses the existing standard Android consent screen, with live exemption status on return. App background settings opens this app’s system details. OPPO/OPlus/OnePlus/realme devices also receive qualified ColorOS menu guidance and a Phone Manager launcher shortcut. Only the two vendor launcher packages are queried; private vendor Activity names are not used. If the launcher is absent or protected, navigation falls back to app details, then general settings. If every destination is unavailable, the app shows manual instructions instead of crashing. No navigation enables a permission or changes an existing user preference.

Android 8–10 instructions explicitly retain system screen-recording consent and do not offer the Android 11+ accessibility screenshot backend. Locking may restrict the picture or interrupt projection; reconnecting an online device does not prove its picture/control remain usable. The existing sharing power option can prevent automatic display sleep while sharing, with its existing eight-hour bound; it does not bypass a manually secured screen. Android 11+ guidance points to the optional, user-authorized screenshot mode and retains protected-content/vendor limitations. Sharing explanations no longer apply newer Android projection revocation behavior categorically to Android 10.

## Verification

Evidence: `.local/android-background-settings-20261105/`. Debug app/test APKs, JVM tests and Android lint passed. The initial API 35 owned-emulator run passed all four background/navigation tests, including real system consent and app details navigation, version-dependent copy, return navigation, unchanged battery/control/online choices, and simulated absent/protected destinations. Final-source runs on owned API 29 (Android 10) and API 35 emulators each passed all four tests (`api29-final-tests.log`, `api35-accepted-tests.log`); all nine JVM tests and final build/lint passed. The first API 29 run incorrectly assumed a scrollable container before checking it; its failure is retained in `api29-tests.log`. The test now waits for the settings content and scrolls only when a container exists. No production change was needed for that test failure.

OPPO R17 is the user’s tentative model identification, not a tested device. No R17/ColorOS test or overnight physical retention acceptance is claimed. No new public package was published, and the personal phone’s existing 0.1.12 installation was not replaced by this debug build.

References: [Android Settings intents](https://developer.android.com/reference/android/provider/Settings), [accessibility screenshots introduced in API 30](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService), and [Doze exemptions and limits](https://developer.android.com/training/monitoring-device-state/doze-standby).
