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

## Physical-device correction

After the emulator cases passed, the first candidate still reproduced offline presence on the locked physical phone. This prevents an end-to-end pass claim. The process and connection foreground service remained alive and unfrozen; accessibility remained enabled, while projection had ended. The phone was powered and the standard Doze controller was still ACTIVE, but the system released its connection wake lock and listed it under Restored Wake Locks. That section is outside the currently effective set; the first physical sampler's tag-only held boolean is unreliable.

The second candidate `0.1.9-online-fix2` adds standard battery-optimization exemption consent, a one-time setup entry on Home and actual system exemption status in Settings. It does not automatically request, approve or repeatedly prompt. The physical user allowed the system dialog; no shell whitelist change was used. Standard service diagnostics expose only successful-heartbeat age, callback age and worker phase, without account information, so process survival is not mistaken for presence. The API 36 system-dialog opening, unapproved state and dismissal-preservation case passed; the second build, lint and package checks passed.

The second physical observation samples first, then stops app Binder sampling for at least 130 seconds before rechecking, to avoid diagnostic calls disturbing sleep. Final results are in `.local/android-idle-offline-20261104/verification.json`. The user requires ordinary installation and declined ADB activation; confirmation-free remote control after secure locking remains unimplemented, and this work is not evidence of that goal being complete. Desktop source now gives Android-specific sharing/interruption guidance; no desktop installer was published.

The quiet-window recheck failed: after about 147 seconds without app Binder sampling, the physical phone remained Dozing with keyguard showing=true, running=true and batteryExempt=true, but heartbeatAgeMs=95984 and no connection lock in the effective wake-lock set. Java cpuLeaseHeld=true does not prove the system still honors the lock; system exemption is not an OPlus background-compatibility pass. Sustained physical-device presence remains unresolved; vendor background activity / auto-start restrictions and an unplugged overnight run remain uncovered.

## Later capture-backend follow-up

The limitation above describes the projection backend and the evidence available at that point. A later optional Android 11+ accessibility screenshot backend supports ordinary installation; see [accessibility screenshot sharing](WI-ANDROID-A11Y-038.md). Its emulator results do not replace the failed physical presence observations above. OEM full background allowance has since been selected through normal settings, with sustained physical idle/lock recovery still pending.
