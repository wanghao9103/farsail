**English** | [简体中文](WI-ANDROID-UPDATE-049.zh-CN.md)

# Android candidate refresh

2026-11-11. Base: main `25a5ac9`, Android `0.1.17`, version code `18`. Retain its independent heartbeats, static-frame bootstrap and approved-control wake behavior. Reapply the previously verified registered-device-name UI and temporary lease-failure handling from `45794f9` without replacing newer main behavior.

Temporary transport authority failures retry within the last verified lease. Explicit rejection and independent expiry still close the session; failures never extend authorization. Large gateway error pages retain their HTTP status. Each viewer continues to use its own session and proof context. The local name is looked up by this phone's registered ID.

The `Android candidate package` workflow checks client, transport and Android scenarios, Clippy, Kotlin tests, lint and instrumentation compilation, then builds `0.1.17-lease1` with the existing installed certificate. ARM64 and x86_64 remain required. Local delivery must independently verify the artifact source/digest, APK signature, package name and version. Preparing a candidate does not publish a feed or install it on a phone; physical multi-phone acceptance remains separate.
