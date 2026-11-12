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

Candidate label: `0.1.17-pulse1`, original installed certificate, fresh native libraries from the changed Rust source. The coordinator artifact must come from the same checked source. Publish the verified server upgrade before requesting the live server update, then verify capability detection, uninterrupted heartbeats beyond the five-minute trigger and approved remote access during a real physical unplugged run. Physical acceptance and release provenance are pending; do not report the issue solved yet.
