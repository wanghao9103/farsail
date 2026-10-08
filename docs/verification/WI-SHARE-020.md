# SHARE-020 — remember local sharing and remote watch

## Authority, baseline and exclusive write set

- User selected only the local sharing and remote-watch switches. Persisting advanced relay/bind/force-relay settings, autostart/services, server changes and production profiles is excluded.
- P2P-019 (`01a0f186-dc53-7a00-8207-2fa062873a49`, local) completed and became idle before any write in this task. Its 0.1.9 delivery records seven successful exact-source CI runs and anonymous byte/hash checks. Baseline `b6a7ad76c58faacadf6306fda07567d32ea65682` equals freshly fetched `origin/main`; checkout was clean.
- Existing managed checkout `C:/Users/wangy/.codex/worktrees/desktop-ui-release/远程`, branch `codex/share-preferences-020`. Old `D:/working/远程` and paused transfer WIP are excluded. No subagents or messages to other chats.
- Exclusive write set: `crates/client/src/lib.rs` and narrow new preferences module/tests; `apps/desktop/src-tauri/src/lib.rs` and narrow startup helper/native test probe; `apps/desktop/src/main.tsx`; targeted synthetic browser/native/HTTP and disposable installer verification scripts; installer version `apps/desktop/src-tauri/tauri.conf.json` (0.1.10); README, WINDOWS_INSTALL, IMPLEMENTATION, CLIENT/REMOTE policy and this record. No dependencies or lockfile changes planned.
- Runtime/cache boundary: own `.local/share020-*` fixtures and processes; shared checkout Cargo `target` and Node `node_modules` only after predecessor finished. Never read real app credentials or install over a production profile.

## Knowledge bootstrap

Read three relevant inbox captures: media/input cancellation (partial), device-bound transport and short lease (passed), NSIS installation/artifact consistency. Applicable: fail closed on identity/generation/permission changes, reject and compensate late activation/approval, release injected input on stop, use isolated profiles and CI originals. Revalidate current startup/transport scheduling and native IPC. Historical process-lifetime-only remote-watch policy is superseded by the user's explicit request; authentication, grant TTL and same-account approval are unchanged.

## Implementation and acceptance plan

1. Versioned, strictly validated DPAPI record scoped to canonical server, validated account, bound device and login session; fresh/corrupt/unknown/mismatched records start off. Keep remembered intent separate from runtime availability.
2. One bounded native startup attempt, without device-page interaction: validate resumed login/identity, heartbeat, default transport, interactive display/capture, capability publication, then remote watch. Pending/failure exposed honestly. No endless retry. Advanced transport parameters remain process-local.
3. Explicit sharing/watch opt-out saves off and invalidates old activation before network cleanup. Logout/identity change forgets permission. Operational close/capture/network/transport stop retains remembered preference while cancelling activity. Existing host/watch epochs, capability serialization and delayed approval compensation remain.
4. Meaningful isolated tests: recreate clients/store, same identity restoration and explicit-off persistence; invalid schema and owner/server/device/session mismatch; delayed restore vs sharing/watch off/logout; failure before advertisement; operational stop preservation; real temporary Windows DPAPI. Browser tests cover truthful pending/failure/off and runtime transport reuse; actual native WebView uses own profile and synthetic server, no desktop pixels or real input.
5. Commands: `cargo fmt --all -- --check`; unified `cargo test --locked -p farsail-client -p farsail-desktop -p farsail-transport -p farsail-media -p farsail-windows --lib`; matching Clippy `--all-targets -- -D warnings`; `npm run typecheck`, `npm run build`; existing browser/native suites plus focused fixtures. Disposable installer CI verifies embedded frontend, persistence/failure/off behavior and reinstall/uninstall retention with debug probes absent.
6. Scoped source commit; fresh fetch and non-force push to main under existing release authorization; trigger `windows-release.yml` at exact source. Gate every relevant CI run including tag-triggered runs. Publish four CI originals, anonymously download and compare sizes/SHA256; final evidence docs push. Capture only verified reusable knowledge, stop own processes and leave clean checkout.

## Results

Implemented separately scoped version-1 DPAPI intent, native one-shot startup restoration, runtime transport reuse and truthful pending/failure/cancellation UI. Scope includes the login session in addition to server/account/device. Unknown, damaged, oversized, missing or foreign records are ignored. Watch restoration follows successful host capability publication and re-reads current watch intent under the mutation lock. Advanced transport parameters are not persisted; startup uses the existing default direct endpoint (`0.0.0.0:0`, no configured relay). A deployment requiring a custom relay must reapply it during that run.

Explicit sharing/watch off synchronously cancels the relevant intent before remote cleanup. A failed opt-out replacement attempts to delete the old opt-in as a fail-closed fallback. Ordinary close cancels even a startup task that has not begun, while keeping the record. Invalid authentication, logout, local device replacement/removal and server changes forget permission. Unbinding another device retains remembered intent while stopping the current operational runtime.

The delayed-capability/logout regression identified an existing compensation gap: using current credentials after logout cannot withdraw the old request. Capability requests now retain a private original server/device/generation snapshot and compensate against that identity; activation also verifies the entire current snapshot. This does not expose credentials to the UI or change server schema/grant TTL.

Local verification: actual isolated native WebView performed six ordinary close/restart cycles (enable both, restore both, watch off, restore sharing only, sharing off, restart off), with real DPAPI and the production desktop/Capture constructor. No desktop frame was read or saved, and no input was sent. Existing native viewer/window/IPC isolation passed. Browser suites passed, including pending/failure truthfulness, cancellation and reuse of an already-restored endpoint. Full final Rust/Clippy/installer/release evidence is recorded below when complete.

Unified local Rust suite: 52 passed (client 19, desktop 6, transport 12, media 5, Windows 10); two existing real frame/input tests remained intentionally ignored. Includes the new failure-save/delete fallback and cancellation before startup scheduling tests. After tightening optional-store failure cleanup, client/desktop 25 tests passed again. TypeScript, production Vite build, formatting, Clippy and script syntax checks passed. New native six-cycle evidence is in the ignored `.local/share020-native-verification.json`; existing native isolation and both browser suites also passed. CI and publication remain pending here until the exact-source release is verified.

First installer run `36702053298` at `9c0e300` failed during the added synthetic binding confirmation. The devices page and overview form both have an “添加这台电脑” button; the test now waits for the form's named input before submitting and confirms the resulting synthetic device. Failure diagnostics print only that owned fixture window's accessibility names. No failed installer is published. Failed password/bind/login-switch requests now cancel operational activity but preserve the same identity's remembered intent; successful binding/password change and explicit logout still forget it. A dedicated HTTP regression verifies this distinction.

After that correction, client 20 and desktop 6 tests, formatting and Clippy passed. Combined with unchanged transport/media/Windows suites, final coverage is 53 passing Rust regressions and two intentionally ignored interactive tests. Initial-source Windows client `36702051245`, transport `36702051235` and backend `36702051233` succeeded; a new exact-source installer and regression cycle is required for the correction.

Second installer run `36703091696` at `da00d7e` reached successful native startup sharing (the production accessibility tree showed 本机屏幕共享中) but then looked for the watch button while still on the overview page. The test now explicitly navigates to My Devices on each signed-in launch. Windows client `36703089852`, transport `36703089855` and backend `36703089830` all passed. The existing delayed-logout regression now switches the server before releasing the old capability response, verifying compensation still targets the original identity. Failed installer artifacts remain unpublished.

## Verified delivery — 2026-10-08

The interrupted task was resumed by root with a clean checkout and no active successor writers. Final source is `be659f287386f5b6ef65d690158da23157904d15`. [Installer 36704066596](https://github.com/wanghao9103/farsail/actions/runs/36704066596), [Windows client 36704063594](https://github.com/wanghao9103/farsail/actions/runs/36704063594), [transport 36704063420](https://github.com/wanghao9103/farsail/actions/runs/36704063420), [backend 36704063396](https://github.com/wanghao9103/farsail/actions/runs/36704063396) all succeeded at this exact source.

[Release 0.1.10](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.10-be659f2) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.10-be659f2/FarSail_0.1.10_x64-setup.exe). CI's installed production binary passed native settings restart, sharing/watch preference restart, actual desktop preflight, reinstall/uninstall retention and absence of debug probes. Publication used only those tested CI originals. Four public attachments were downloaded anonymously with normal TLS and matched originals by size/SHA-256.

| Attachment | Bytes | SHA-256 |
| --- | ---: | --- |
| FarSail_0.1.10_x64-setup.exe | 7,910,524 | `cbe56dfde6b18a9024fcac130b7e6c8c986d83cc9cbeec94d76e008e3ef4166f` |
| SHA256SUMS.txt | 96 | `eb4ed95e6b9f03f156802821048d7afa74e69c2cf5f315d5dc63c70e0663f26b` |
| release-metadata.json | 653 | `51ceeb3f08b017a80ac4f147dc1aacc1664b2989c3aa980a7938e27314104fc9` |
| installation-verification.json | 508 | `f4f6c5db683c9c269e731a87a4a7ad24daaa8a21fb2cd20eb0c97815f1fe5878` |

No production installation/profile/server changes. Old primary WIP remains excluded. The record preserves the process-local advanced-connection limitation and fail-closed identity/desktop requirements; it does not claim cross-NAT unattended recovery with an unconfigured relay. Source tag triggered three additional regressions, whose completion is checked before the next source write set starts.

Tag-triggered backend `37719071423`, Windows client `37719071452` and transport `37719071451` all succeeded at the same source; all seven relevant CI runs are successful. Source delivery and publication are complete. Root now owns the subsequent INPUT-021 write set; the interrupted successor tasks remain inactive to avoid concurrent edits.
