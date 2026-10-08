**English** | [简体中文](P2P-REFRESH-HANDOFF.zh-CN.md)

# P2P-019: refresh discovery without restarting clients

## Authority and workspace

- User confirms the server QAD=true, `[tls].quic_bind_addr=0.0.0.0:7842`, Docker UDP mapping and actual relay socket. Cloud rule had mistakenly been TCP. After correcting UDP and fully restarting both clients, the current connection is direct (user screenshot RTT16ms,4.9FPS). User asks why connecting/remote control does not freshly discover direct connectivity. Implement an appropriate bounded refresh behavior, not a claim that P2P was absent.
- Baseline `392b88d7390c76155057f5d9ab1a297200ca7652` (verified 0.1.8), managed checkout `C:/Users/wangy/.codex/worktrees/desktop-ui-release/远程`; new branch `codex/p2p-refresh-019`. Parent will commit this handoff before dispatch and stop all writes. Continue in this exact checkout, not the saved project's default directory.
- Primary `D:/working/远程` contains stale baseline and paused Cargo/file-transfer WIP. Never modify, restore, commit or merge its contents. This existing managed worktree is attached to the parent task. Do not create a redundant checkout/worktree or overwrite primary.
- User's standing AGENTS authorizes same-project long-task handoff. No subagents unless separately explicitly authorized. No server SSH, production profile mutations, real desktop screenshots, private environment/token dumps or messages to other chats. Existing push/new-installer authorization persists; carry a verified 0.1.9 outcome through exact CI artifacts and anonymous public hash checks.

## Facts vs open questions

- Actual current FarSail direct authenticated frame/renewal/revocation test passed again. Desktop forceRelay default=false, bind=0.0.0.0:0, iroh Minimal+custom relay, public lookup and portmapper disabled. Those do not disable QAD. The frontend path status is read live from QUIC selected path.
- Independent QAD probe in ignored `.local/qad-probe` fixed to Iroh1.2.0 and the repo lock baseline: UDP7842 QAD IPv4 roundtrip=true, global IPv4 discovered=true, HTTPS probe=true. TLS-validated `/ping`200. No-service loopback negative fixture returned all false. No account/credential/persistent identity was used, no addresses/keys printed. Target URL was user-supplied; do not publish it. Probe evidence files and source in `.local/qad-probe`, executable in `target/debug/farsail-qad-diagnostic.exe` (closed on return).
- Existing native main heartbeat every25s republishes endpoint.addr(), but does not force fresh discovery. `NativeClient::connect_transport` currently downloads peer address and connects without refreshing the local discovery first.
- User restart result proves this pair/network can achieve direct. Exact internal stale-cache causal sequence is still inferred, not instrumented. Do not promise all NATs can punch or that periodic QAD was completely absent.
- Windows ActiveStore firewall query was denied by OS permissions; do not elevate/alter rules to sidestep it.

## Version-specific API findings

Read cached Iroh1.2.0 source (registry path on this host) and official docs, do not rely on latest API memory.

- Endpoint::network_change() merely forwards to netwatch network monitoring. A cloud firewall-rule change is not a local interface/route change; this API alone may not guarantee a fresh complete QAD report.
- socket.rs: net reports periodically every20-26s; `net_report.rs` complete report every5min, otherwise incremental. `UpdateReason::RelayMapChange` is classified major.
- Public Endpoint::insert_relay(url,Arc<RelayConfig>) replaces the existing map entry and sends RelayMapChange unconditionally. Re-inserting the current exact config is a candidate to schedule a complete QAD report without recreating the endpoint, changing identity, closing sessions or removing relay. This candidate still needs a controlled executable regression before using it. Do not remove/re-add the relay, copy a fresh probe's unrelated UDP address into the live endpoint, or restart transport after a grant has been approved (stop invalidates grants).
- Endpoint::watch_addr() is stable, usable for prompt changed-address publication. Endpoint::net_report() is gated unstable-net-report, which can be enabled only for development tests if needed; don't casually add unstable diagnostics to release.
- Once a new net report arrives, socket.handle_net_report_report notifies transports and updates direct addresses. Existing QUIC path state can upgrade dynamically.
- iroh-relay::server::{Server,ServerConfig,RelayConfig,TlsConfig,QuicConfig} supports authenticated ordinary TLS relay plus separate QAD server; QuicConfig.server_config accepts rustls::ServerConfig. Test CA must be properly trusted, not accept-all.

## Work item and exact write set

Implement P2P-019, intended release0.1.9: bounded discovery refresh before new source/inbound connections, appropriate relay-only-session retry policy and/or explicit retry command if useful; publish fresh local addresses promptly. Preserve current endpoint identity/credentials, session/grant TTL, 16-session/8-handshake limits, input cancellation, force-relay override, and ability to continue media while discovery is failing. Prefer a single shared refresh lock/cooldown, avoid per-status-poll network storms. Never repeatedly force transport recreation or close a good direct connection.

Write set: `crates/transport/src/lib.rs`, `crates/transport/Cargo.toml` if needed for dev-only net report test feature; `crates/client/src/lib.rs` plus narrowly needed native IPC/viewer/frontend/CSS/browser tests if an explicit retry status/action is implemented; `apps/desktop/src-tauri/tauri.conf.json` version; relevant test scripts, docs/verification/WI-P2P-019.md, docs/TRANSPORT.md and release docs README/WINDOWS_INSTALL/IMPLEMENTATION. Expand only after recording the concrete reason here. No server/schema/codec or unrelated file-transfer scope.

## Required verification

1. Controlled real QAD failure→server becomes responsive→refresh on same endpoint→QAD success, without restarting endpoint/client. Use loopback random ports (test-only relay map QUIC config override) and trusted temporary CA. Keep a real authenticated live session sending data across refresh to verify no teardown. Avoid merely asserting that an API was called. Server can initially hold/drop UDP on a reserved random socket, then start QAD on that same test port.
2. Forced-relay and no-relay refresh skip; cooldown/single-flight, closed/lifecycle cancellation, old address generation constraints and pending grants retained. Existing revocation/TTL and denied input remain required.
3. Build/fmt/appropriate Rust tests+Clippy -Dwarnings. Actual native viewer isolation and browser suites if touching UI. Tests and helper runtime only own loopback/isolated processes; no real app interruption.
4. User-facing progress should distinguish trying direct, currently relay fallback, discovery/refresh failed, forced relay. Do not fabricate NAT diagnostics from a single QAD server or expose endpoint/IP/token diagnostics in public UI/logs.
5. Current working source prior to this task has no modifications; only this handoff is new. Keep test caches in target/node_modules/.local. Stop own processes after tests.

## Existing build/release workflow

- Bundled Playwright NODE_PATH: `C:/Users/wangy/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules`.
- Vite explicit command from apps/desktop: `node ../../node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1420 --strictPort` (npm arg forwarding in PowerShell previously stripped --host).
- Browser `node scripts/test-desktop-ui.cjs`, `node scripts/test-remote-viewer.cjs`; native `cargo build --locked -p farsail-desktop`, `./scripts/test-native-viewer.ps1`. Native smoke uses isolated profile and owns its process.
- Ordinary targeted Rust: include client/transport/desktop/media/windows in one feature-unified command where useful to avoid repeated full rebuilds. Input/DXGI ignored tests only in verified owned HWND/PID/memory, never user's desktop app.
- `git push origin HEAD:main` (fresh fetch/no force/scoped stage), then `gh workflow run windows-release.yml --ref main`; wait every triggered relevant run at exact source SHA. No production installation locally. Disposable Windows CI validates real install/settings restart/reinstall/uninstall/debug-probe absence.
- Previous `.local/publish-018.ps1` validates exact source runs/version/hash and publishes four CI originals. Make019 helper with actual commit/runIDs/tag/version, do not publish a local debug build. `.local/release-notes-018.md` is previous notes template.
- Anonymous curl downloads of four assets use normal verified TLS, HTTP1.1, finite45s deadlines. Compare sizes and SHA256 with CI originals. Retry only failures and resume known partial installer. Write proof table, update README/WINDOWS_INSTALL/IMPLEMENTATION after successful publication, docs commit separately.
- Latest released0.1.8 tag windows-preview-0.1.8-0f43f10, source0f43f106f31e6065e6c2c45569d27f64930ab695; main docs baseline392b88d.
- Bootstrap relevant notes before implementation (narrow retrieval; skill already applied in parent). Capture only verified reusable results under Obsidian inbox, update existing project mechanism note where appropriate, no secrets/private IP. Relevant notes: FarSail device/transport short-lease boundary and offline-image/shared-network/QAD operations. Their physical cross-NAT gaps are historical; user now confirms current pair direct, do not erase other boundaries.

## Handoff status

No product changes made yet for P2P-019; candidate refresh API not yet verified. Parent stops writing after this handoff commit. All prior diagnostic processes ended; only local probe source/results and build caches remain. Take exclusive responsibility for this write set and carry to verified delivery; do not ask routine permission already authorized above.

## Completed successor delivery (2026-09-30)

P2P-019 source `5c261c5578635bbe6174b209ff6bfbb0b0d588f6` implements bounded original-endpoint refresh, relay-session maintenance, prompt watched-address publication and public scheduling status. Local regression, exact-source CI/install and four anonymous public asset hash checks passed; [0.1.9 release](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.9-5c261c5). The final evidence, precise write set and physical cross-NAT migration limitation are authoritative in [WI-P2P-019](verification/WI-P2P-019.md). All owned test processes are stopped. Primary WIP/server/production profiles/credentials were not modified; no messages were sent to other chats. Continue in this existing managed checkout, not the old primary.
