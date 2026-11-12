**English** | [简体中文](WI-ANDROID-SERVER-PULSE-053.zh-CN.md)

# Server-driven Android standby heartbeats

2026-11-12. WI-051 captured a 125-second heartbeat interruption when ColorOS removed CPU locks; WI-052 confirmed that local wake alarms are also coalesced to approximately five minutes. Both exceed the unchanged 60-second device-online lease. Prepare a paired coordinator and Android update so a real network response can drive the next authenticated heartbeat without waiting for the phone's suspend-paused looper timers.

## Protocol and authorization

`POST /v1/devices/heartbeat` accepts optional `wait_for_next: true` only with an established `generation`. Existing requests remain immediate and keep their original response shape. Opt-in requests require the same device credential, bound/enabled device, enabled account and unrevoked login session. They renew the device lease on request arrival, release the database connection, wait 15 seconds, then recheck credentials, device generation and remaining lease before replying with `standby_wait_ms: 15000`. Delivering the response never renews the device lease. The returned `expires_in` subtracts the server wait. At most 512 delayed requests wait concurrently; no database connection is retained during the wait. No migration or change to remote grants is required.

The independent native presence lane retains its cached identity revision, scope checks, sign-out cancellation and post-response identity validation. Only the opt-in request gets a 35-second HTTP timeout; other calls retain 15 seconds. Response-size and HTTP-error classification limits remain. The JNI operation bypasses account/UI state locks and does not change transport frame delivery, recording permission or remote approval.

## Android worker

An enabled foreground connection service sends the next request directly from its dedicated worker when the delayed response arrives. Heartbeat checkpoints and gap counters update on that worker; UI refresh and endpoint maintenance cannot block it. The saved scope, service generation and keep-online preference are checked before each request. Denial and identity replacement terminate the existing task; shutdown stops subsequent requests, with an already-issued network request remaining bounded by its timeout.

Old servers ignore the optional request field and return no delay marker. The client then falls back to ordinary timed heartbeats and displays the need to upgrade the server. A claimed delay with an implausibly immediate response also cannot create a busy request loop. Reopening the app retries capability detection. The bounded system-alarm path remains only a recovery fallback. No fake media, sensor traffic, system whitelist edits or alarm-clock UI is used.

## Verification status

Local Android checks passed 22 JVM tests, debug lint (zero errors, 52 warnings) and instrumentation-test Kotlin compilation. Native standby tests passed 9 cases, including account-lock isolation, late identity replacement, revocation/sign-out and a real 16-second response to verify the dedicated timeout. PostgreSQL integration cases cover legacy response compatibility, missing generation rejection, a real delayed response without extending the accepted lease, and device disablement during an outstanding wait. Their database execution is required in CI before coordinator publication; local compilation and Clippy are also required.

Candidate label: `0.1.17-pulse1`, original installed certificate, fresh native libraries from the changed Rust source. Verify live server upgrade and capability detection, uninterrupted heartbeats beyond the five-minute trigger and approved remote access during a real physical unplugged run. Physical acceptance remains pending; do not report the issue solved yet.

## Verified packages

Source `e2a6bde0266837bbc74111d0616038fd21d1f5bf` passed coordinator PostgreSQL tests, updater regressions and actual container replacement in [run 43236269761](https://github.com/wanghao9103/farsail/actions/runs/43236269761). The extracted package metadata, all internal checksums and archive digest were independently verified. [Coordinator prerelease](https://github.com/wanghao9103/farsail/releases/tag/coordinator-idle-e2a6bde) contains the verified Linux amd64 package and sidecar; published asset digests match the downloaded CI artifact. Package SHA-256: `131b02e1c88c0e2f2db59d6dce11f865163a8b2c35a4aa122eccc9d535c81f66`. This coordinator-only tag is accepted by the existing updater; it does not publish a desktop or Android update feed.

[Android run 43236269492](https://github.com/wanghao9103/farsail/actions/runs/43236269492) passed the full native checks and rebuilt both ABIs from the same source. Artifact 13954231744 was verified against its API digest. The APK is `app.farsail.android`, version `0.1.17-pulse1`, code 18, signed with the original installed certificate `fc352f906ce7b42d12fe83d8b8f7fc4030e3297f19fc8faea40193426fd4f613`. APK SHA-256: `63231905c2a3f0b7afbf8d954cfffaff850084973e451d532cdc2b5b682fb5d7`. Native SHA-256 values: ARM64 `645bc1c5c507e54d0e00b95b75335386d9e536ffca2ac3e07f10dbcae393b202`; x86_64 `e6bcaf0440e2c5c4e8cdc9292b4788fb243b333669494b0a9a8bfbcc2d57e724`. Both differ from the old base libraries, as required by the changed Rust source. Live server upgrade and phone installation/physical acceptance remain separate steps.

At 22:41 on November 12, the inspected phone was updated in place with `adb install -r`. Version `0.1.17-pulse1` was confirmed; UID 10191 and original installation time `2026-11-04 22:20:21` were unchanged. No uninstall or data clear was performed. At installation, manual recording consent, live server upgrade confirmation and physical unplugged acceptance were pending. No other phone has received this candidate yet.

## Live controlled regression, November 13

The user confirmed the server upgrade and restored recording consent. The live phone then reported `serverPulse=true`, real delayed responses and active sharing. At 05:39–05:47 (Asia/Shanghai), a controlled run retained physical USB for logs, temporarily reported battery-unplugged and sent FarSail to the background with the display asleep. No forced Doze or ADB wake/unlock was used. Nineteen samples established 432 seconds of continuous screen-off, the same PID 23899 and active sharing throughout. Responses increased from 3 to 33, with a maximum interval of 15,151 ms. System CPU-lock entries disappeared after approximately five minutes; delayed responses continued beyond that point. System wake-alarm deliveries remained at one, so the repeated response cadence was independent of alarm deliveries.

The test restored real battery reporting in `finally`; a subsequent read confirmed `UpdatesStopped: false`, USB powered, `mForceIdle=false` and active idle states. Physical USB remained connected in this regression, so it does not establish availability under actual CPU suspension on battery.

At 05:48 the user physically unplugged USB, confirmed by an empty ADB inventory. The saved baseline has PID 23899, 35 delayed responses and a 15,151 ms maximum gap. The one-hour physical test is in progress. Before reconnecting USB or opening the phone app, verify the desktop device remains online and use an ordinary approved remote-control connection to test wake and continuous video. Then reconnect USB for heartbeat-counter, maximum-gap, process and power-history readback. If the device becomes offline early, preserve the phone state for readback. Do not infer remote wake from a USB-triggered display wake.
