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

Implementation and delivery pending. No release is claimed by this plan.
