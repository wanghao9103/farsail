**English** | [简体中文](IMPLEMENTATION.zh-CN.md)

<a id="farsail-实施与接力记录"></a>

# FarSail implementation and handoff record

<a id="input-021--0111-鼠标释放与持续控制2026-10-08"></a>

## INPUT-021 — 0.1.11 mouse release and continuous control (2026-10-08)

After confirming that all handoff tasks were interrupted and no other writers were active, the root task first closed out 0.1.10, then completed this work item serially on the clean 365f2d4 baseline. Tests reproduced old-layout UP returning Geometry and leaving the button held. Release now applies only to buttons owned by this session; stale/missing layouts do not block release, normal positions still reposition the pointer, and tracking and suspension are retained when the OS refuses input. The viewer tracks Pointer Events/buttons masks, uses its own capture for drags outside and combined buttons, and cancels old queues and releases on focus/capture loss. Moving inside with a button held outside is not treated as a new click.

Source 70d9393342d07d0989fed6a178edb0e3ab057117 was pushed and [0.1.11](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.11-70d9393) released. 56 ordinary Rust tests, frontend/fmt/Clippy, two browser groups, 20 clicks in an actual self-created window/focus switching between two windows, native IPC isolation and sharing-preference restart passed. Exact-source installer 37722171352, client 37722169148, transport 37722169167 and backend 37722172138 succeeded; actual production installation/preference restoration/desktop checks/reinstallation-uninstallation retention passed. Complete anonymous validation of the four public attachments matched. Installer SHA256: f3afc4894f351b079390bfd97cde2ed27061274a1c71085ae7d441e3584b7ef4. Evidence and the next entry point: [INPUT-021](verification/WI-INPUT-021.md). The old main-directory WIP and real configuration/server were untouched, runtime stopped, and the knowledge note updated. The fix addresses a reproduced mechanism; establishing the original incident's sole root cause still requires upgrading and retesting both ends.

<a id="share-020--0110-本机共享与远程值守偏好2026-09-30"></a>

## SHARE-020 — 0.1.10 local sharing and remote standby preferences (2026-09-30)

Delivery was closed out on 2026-10-08: final source `be659f287386f5b6ef65d690158da23157904d15`, all four source CI checks succeeded, and the installation report confirmed sharing-preference restoration, desktop checks and reinstallation/uninstallation retention in the actual production version. [0.1.10](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.10-be659f2) was released, and anonymous download validation of all four attachments matched. Installer SHA256: `cbe56dfde6b18a9024fcac130b7e6c8c986d83cc9cbeec94d76e008e3ef4166f`. The first two failed attempts were used only to correct acceptance procedures and were not released. INPUT-021 followed serially; the old main directory and its file-transfer WIP were retained.

Implementation was serial on the clean `b6a7ad7` baseline after full P2P-019 delivery, remembering only the two switches specified by the user. Versioned DPAPI records bind the service, account, device and login session. Native startup revalidates login, heartbeat, transport and ordinary desktop once, then restores sharing followed by standby. Manual disablement, logout and identity changes discard delayed restoration. Ordinary window closure, capture failure or network failure stops only effective activity while retaining valid choices. The interface distinguishes remembered intent from actual sharing, supports cancellation and explicit retry after failure, and keeps advanced connection parameters valid for the current run only.

A delayed capability update after logout must compensate using the original request identity; otherwise, cleared current credentials cannot retract the old update. New isolated HTTP/actual QUIC endpoint and temporary DPAPI tests verify this boundary. Six ordinary close/start cycles of a real native window verified actual sharing/standby restoration and manual disablement of both choices, without reading desktop pixels or sending input. Installation CI also checks preference reading, failure closure, IPC closure and reinstallation/uninstallation retention in the production embedded frontend. See [SHARE-020](verification/WI-SHARE-020.md); that record closes out exact-source, CI and original release-artifact evidence.

<a id="p2p-019--019-保留端点的有界地址刷新2026-09-30"></a>

## P2P-019 — 0.1.9 bounded address refresh retaining endpoints (2026-09-30)

Source `5c261c5578635bbe6174b209ff6bfbb0b0d588f6` was pushed and [0.1.9](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.9-5c261c5) released. Full discovery is requested before new connections and during inbound handshakes; native relayed-session maintenance retries periodically. Endpoints share a 30-second cooldown, not triggered by status polling. The address watcher publishes promptly; uploads verify lifecycle/heartbeat generations and complete serially. Identity, sessions, screen, approvals and existing authorization boundaries are retained; forced relay/no relay skip refresh. Upgrading both ends is recommended, with compatibility for the existing server.

Real trusted-CA/random-loopback QAD blackhole → recovery, 80 authenticated relayed frames without interruption/with renewal, direct connections between endpoints from the same source, and refresh retaining direct connections passed. 43 local Rust tests, Clippy/fmt/build, two browser groups and independent native IPC isolation passed. Installer 36695612068, Windows client 36695611364, transport 36695611377 and backend 36695611280 all succeeded. All four original public attachments were downloaded completely and anonymously; sizes/digests matched CI. Installer size: 7,892,785 bytes; SHA256: `38254c65406ce25cd6b98745779eac7fcbc3aeb0478f5babbab7f019f92bd71c`. For complete evidence and boundaries, see [P2P-019](verification/WI-P2P-019.md). The user supplied additional evidence of direct connection after restart; no acceptance claim was made for the specific old-cache root cause or physical cross-NAT upgrading of existing sessions after network repair. The current managed workspace can continue to be used. Old main-directory WIP, server/production profile/credentials were not operated on. Owned test runtime was stopped and lessons captured in the knowledge-base inbox.

<a id="viewer-018--018-默认铺满远程画面2026-09-30"></a>

## VIEWER-018 — 0.1.8 remote screen fills by default (2026-09-30)

The user reported non-remote-image areas on the left/right of a wide window and above/below a restored window. The root cause was contain letterboxing with differing aspect ratios, not missing source imagery. Source `0f43f106f31e6065e6c2c45569d27f64930ab695` was pushed and [0.1.8](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.8-0f43f10) released: the complete screen fills by default, while “More actions” offers remembered aspect-ratio preservation. Mouse/button/scroll mapping stays synchronized; switching cancels and releases old input. Source resolution, remote system display mode and the server are unchanged; only the viewer needs updating.

Actual screenshot corner-color checks verified no borders or cropping in wide, tall, fullscreen and restored states. Both coordinate modes, blank-area rejection, preference retention and existing regressions passed. Frontend build/formatting, native windows and nine IPC-isolation checks passed. Windows client 36687041306 and installer 36687086372 succeeded; installation/reinstallation/uninstallation retention and anonymous validation of all four public attachments passed. Installer size: 7,871,373 bytes; SHA256: `6d1608d7e68f044e72a3cc5dd0c2f814c0707d82aab544e411f2048e09f3b8c7`. For complete evidence and the next entry point, see [VIEWER-018](verification/WI-VIEWER-018.md). The current managed desktop-ui-release workspace was reused. Main-directory file-transfer WIP, production configuration and server were retained, runtime stopped, and the knowledge base updated. Filling stretches the image when aspect ratios differ; preserving aspect ratio intentionally leaves borders. No physical two-computer performance measurements were added.

<a id="viewer-017--017-统一标题栏自动适配与-2k4k2026-09-30"></a>

## VIEWER-017 — 0.1.7 unified title bar, automatic fitting and 2K/4K (2026-09-30)

Current delivered source `d7753f2806827dbf60f003b7095075bf7c7c2a33` was pushed and [0.1.7](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.7-d7753f2) released. The remote window uses the same dark custom title bar as the client and hides it automatically in fullscreen. Quality defaults to fitting window physical pixels and source capability, with 720p/1080p/2K/4K support; remote display mode is not changed and source pixels are not upscaled. Static desktops send periodic keepalives. Frame rate uses actual receive intervals over four seconds; capture waits were shortened without promising actual high frame rates between two computers. Older endpoints retain 720p/1080p and 1 MB-per-frame compatibility; higher presets extend bounded media/budgets/deadlines.

31 ordinary local Rust tests, real input into a self-created window, DXGI in-memory encoding/decoding, native custom title bar/real fullscreen button/separate closure, two browser-regression groups and fmt/Clippy/build passed. Installer 36680856936, Windows client 36680825793, transport 36680825771 and backend 36680825765 all succeeded; installation/reinstallation/uninstallation retention and anonymous download validation of all four public attachments passed. Installer size: 7,873,131 bytes; SHA256: `1ee9c1deab63567a03d4a9647bee702e688712c2365f91d045cde35ce8ca5ca5`. For evidence and the next entry point, see [VIEWER-017](verification/WI-VIEWER-017.md). Future work reuses the current managed desktop-ui-release workspace; file-transfer WIP in the original shared directory, production configuration and server were unchanged, runtime stopped, and the knowledge base updated. Both ends upgraded to 0.1.7 can use the full HD presets; physical two-computer results and actual 4K monitor behavior still need on-site acceptance testing.

This file is the authoritative entry point for development status. `PROJECT_DESIGN.md` defines product goals and cannot serve as evidence that features are implemented. On 2026-09-28, the user authorized implementation according to the design and authorized committing completed code to the public GitHub repository.

<a id="起点与协作边界"></a>

## Starting point and collaboration boundaries

- Initial commit: `e74298567ce3e2fe819b0e1c05fdbba40d7cc15b`, containing only design, research and brand assets.
- Remote: `https://github.com/wanghao9103/farsail`, public, MIT, main branch `main`.
- Under the user's global long-task rules, each independently verifiable result uses one task in the same project; shared-workspace writes are serial. While a writing task runs, the coordinator performs read-only checks and updates the next work item only after that task finishes.
- Before each task starts, verify the recorded baseline HEAD and workspace. Do not overwrite other uncommitted changes. Modify files only in the task's Write Set; record the reason and specific paths before a necessary expansion.
- Local runtime state belongs in the ignored `.local/`; Rust cache `target/` and Node cache `node_modules/` are not committed. Docker operates only on `farsail-dev` by default and isolated test projects explicitly added by work items; local ports bind only to loopback. Do not stop other user services. WI-008A separately defines `farsail-deploy-test`; the user's server runs parameterized `farsail-prod`.
- Each delivery includes implementation, appropriate tests, accurate documentation and an independent commit. Before pushing, review staged scope, credentials and private paths; use the existing GitHub login and project noreply identity. Do not upload test accounts, tokens, actual email, screen imagery or running databases.
- The user prepared a public Ubuntu 24.04.2/amd64 server with Docker 29.8.1 and Compose 5.5.1 installed; about 1.6 GiB memory and 35 GB free disk. Initial testing uses a loopback-only inbox. The project has not yet been deployed to the server; the user currently runs commands in an external terminal, and the coordinator has no SSH session or key it can take over. Gaps in public-network, cross-NAT and mobile validation must be recorded accurately.

<a id="环境基线"></a>

## Environment baseline

- Current host: Windows; Rust 1.93.0, Cargo 1.93.0; Node 23.11.1, npm 10.9.2; Docker Engine 29.1.3 available.
- An Android SDK directory exists; specific platforms/NDK/emulators and physical devices still need checking.
- Only a local Windows execution environment is currently available. iOS requires Mac/Xcode or usable CI and physical devices; a web preview cannot replace verification.
- Dependencies use stable versions actually available in the registry and compatible with the toolchain, with lockfiles committed. Versions from previous online research in the documentation must be reconfirmed against current builds.

<a id="工作项与顺序"></a>

## Work items and order

| Work Item | Verifiable result                                                                                               | Status                                                                                                               |
| --------- | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| WI-001    | Rust workspace, PostgreSQL account/device/authorization service, isolated local execution and permission tests  | Complete: local acceptance passed, code pushed                                                                       |
| WI-002    | Tauri client login, device registration/list, secure credential storage, user/administrator interface           | Complete: local and Windows CI passed, code pushed                                                                   |
| WI-003    | iroh end-to-end connection, application authorization, direct/relay configuration and revocation                | Complete: focused local verification and remote CI passed                                                            |
| WI-004    | Windows screen capture, user confirmation, mouse/keyboard input and usable remote-control baseline              | Local and three remote CI workflows passed                                                                           |
| WI-008A   | Public-IP deployment bundle, prebuilt Linux artifacts and integration preparation                               | Complete: pinned six-image artifact, anonymous downloads, cross-Docker-storage import and two Linux CI rounds passed |
| WI-008B   | Windows x64 preview installer, production build and public-artifact acceptance                                  | Complete: NSIS, native installation lifecycle, CI and complete anonymous downloads passed                            |
| WI-008C   | Deployment download HTTP/1.1, reliable resume and cache reuse                                                   | ready: prioritize the actual deployment blocker                                                                      |
| WI-005    | Chunked bidirectional file transfer, lossless compression, verification/resume, authorization and rate limiting | In progress: temporarily yielded write access to WI-008C; unverified WIP retained                                    |
| WI-006    | Actual hardware encoding/decoding, multiple monitors and adaptive rates, capability negotiation and 4:4:4 path  | planned                                                                                                              |
| WI-007    | Android/iOS mobile control and file interfaces, executable platform builds and validation                       | planned                                                                                                              |
| WI-008    | Full-chain regression, deployment/packaging, public repository and real-environment validation                  | planned                                                                                                              |

Any work item may be split into smaller consecutive items based on measured dependencies, but unimplemented scope must not be marked complete. Mobile hosting, unattended access and other later extensions remain future routes under the original design.

<a id="wi-001rust-账号与设备服务"></a>

## WI-001: Rust account and device service

<a id="基线与-write-set"></a>

### Baseline and Write Set

At startup, HEAD was verified as `1a48cdbd268335a29406724cc54677d5aa00a9f4` and the workspace was clean. All code paths were within the Write Set below; design drafts and other work items were not modified. Implementation commit `0e6be111e03478053a9d10e252d46a9b392a4367` was pushed and checked with `git ls-remote`; the Linux GitHub Actions backend workflow passed. See `docs/verification/WI-001.md`.

Allowed changes: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/core/**`, `services/coordinator/**`, `deploy/local/**`, `scripts/test-coordinator.ps1`, `.github/workflows/backend.yml`, `.gitignore`, `README.md`, `docs/IMPLEMENTATION.md`, `docs/API.md`, `docs/verification/WI-001.md`.

<a id="必须交付"></a>

### Required delivery

- Modular Rust/Axum service and PostgreSQL migrations, with core protocol/DTOs separated from the service. Configuration comes from environment variables or ignored local files; listening is local-only by default, and production configuration must not silently enable development backdoors.
- Email registration, verification, login and password change/recovery; Argon2id password hashing; short access sessions, refresh rotation/revocation, logout/session removal. Email uses a replaceable mailer, with an in-memory mailer for tests and Mailpit available locally; this work item sends no real email.
- Device public-key possession proof, one-time challenges, unique ownership, complete paginated same-account device list, capability/status, renaming and unbinding. Dedicated device credentials are separate from user-session permissions. Heartbeats have generations/deadlines; old connections must not pollute new connection state.
- Temporary invitations, remote-control/file permission requests, target-device approval/denial, short authorization leases, renewal and revocation APIs. This work item tests the state machine and authorization; real transport is integrated in WI-003.
- Administrator user enable/disable, registration policy, device revocation and session/audit APIs. Administrators cannot automatically obtain users' remote-control permissions; bootstrap admin uses explicit local configuration/commands rather than first-come registration.
- Reproducible local PostgreSQL/Mailpit environment, run documentation and API contract; isolated development/test data and clear explanations of external dependencies or unfinished features.

<a id="验收"></a>

### Acceptance

- `cargo fmt --all -- --check`
- `cargo test -p farsail-core`
- `cargo test -p farsail-coordinator` (includes real PostgreSQL integration tests using an independent test database/isolated schema)
- `cargo clippy -p farsail-core -p farsail-coordinator --all-targets -- -D warnings`
- Core negative tests: cross-account enumeration/access, device-challenge replay and concurrent binding, device-token privilege violations, refresh reuse/revocation, old heartbeat generations, grants before approval/after expiry/after revocation, read-only permissions and administrator boundaries.
- Actually start the local service and verify health checks; record HTTP smoke checks, migrations, ports, commands and shutdown methods. Mock-only tests must not be used to claim that database authorization behavior passed.
- Update status, evidence, the next item's contract and unverified environments; commit and push, then verify the remote commit. Continue fixing any remaining core failures rather than closing the item as a “completed skeleton”.

<a id="记录"></a>

### Record

The Rust workspace, shared protocol enums, Axum coordinator and PostgreSQL migrations are implemented. Accounts support email verification, login/recovery, Argon2id, access/refresh rotation and login-session revocation. Devices support Ed25519 possession proof, unique binding, login-session-associated dedicated credentials, a complete paginated list, heartbeat generations and unbinding. Authorization supports one-time invitations, target-device approval/denial, 30-second grants, renewal and revocation. Administrators support user/device enable/disable, registration policy, email-scoped signup invitations and metadata audit. Tests use real PostgreSQL isolated schemas and an in-memory mailer or local Mailpit. For complete commands, results, ports, runtime state and still-unverified items, see the [WI-001 verification record](verification/WI-001.md); for the request contract, see the [API documentation](API.md).

WI-002 integration begins with `/v1/me`. Account access tokens and device tokens are stored separately, and device keys stay in native secure storage. `GET /v1/devices` returns all bound devices in the same account, including offline and administrator-disabled devices. `GET /v1/remote/{id}` reports only pending/approved/denied/revoked/expired state and does not transmit grant tokens. Only the target's `decide`/`renew` receives grant tokens; secure delivery to the requester and iroh handshake validation belong to WI-003. Public-network operation, domain/TLS proxies, real email, a second Windows computer, P2P/relay revocation and mobile clients are currently unverified.

Coordinator acceptance: implementation and handoff were closed out, remote main is `9f222515c0482cf83e318c1788dcbf34f9d61be9`; real local database/HTTP and Linux CI passed, workspace clean, and no services for this work item are running. Approved proceeding to WI-002.

<a id="wi-002tauri-客户端与账号设备界面"></a>

## WI-002: Tauri client and account/device interface

<a id="基线与-write-set-1"></a>

### Baseline and Write Set

Code baseline `9f222515c0482cf83e318c1788dcbf34f9d61be9`; after committing this section's plan separately, use that plan commit as startup HEAD.

Allowed changes: `apps/desktop/**`, `crates/client/**`, `packages/ui/**`, `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `.gitignore`, `scripts/start-local.ps1`, `scripts/test-desktop.ps1`, `.github/workflows/client.yml`, `README.md`, `docs/IMPLEMENTATION.md`, `docs/CLIENT.md`, `docs/verification/WI-002.md`. If an existing backend contract blocks actual integration, first record the exact necessary fix scope in `services/coordinator/**` / `docs/API.md`, then fix it and run focused backend regression tests. Arbitrary rewriting of historical migrations is prohibited.

<a id="必须交付-1"></a>

### Required delivery

- Tauri 2 + TypeScript client that compiles and runs on Windows (React/Vite preferred), using the selected FarSail v2 icon. The interface includes login/registration/email verification/recovery, my devices, connection requests and sessions, account security, settings and administrator pages; administrator pages may initially be integrated into the same client.
- All application pages call the real coordinator API. The native Rust layer handles HTTP, login/refresh serialization, device signatures and heartbeats. The interface cannot obtain long-lived tokens or private keys and does not store credentials in WebView localStorage. Do not construct a fake device list as a runtime result.
- Configurable service address: explicit loopback HTTP for local development, validated HTTPS enforced publicly. An HTTPS public IP without a domain is a valid configuration. Do not introduce an option to disable certificate validation.
- Windows credentials use system secure storage or DPAPI protection; keys, account login and device credentials are managed separately. Logout/account switching clears UI state and stops old heartbeat/poll tasks. After login revocation, old device credentials cannot reconnect. Later mobile clients adapt native secure storage through interfaces.
- Confirm device binding after login, read all same-account devices through complete pagination (including offline/disabled), search and filter by capability. Support renaming/unbinding and revoking other login sessions. Device capabilities must correspond to features actually implemented; without capture/files integrated, do not declare the local device a working host.
- Real remote-control requests/temporary invitations, target-device approval/denial, status polling and cancellation. This work item explicitly connects only authorization UI; screen/input and file channels are implemented in later items. Grant tokens remain in native Rust and do not leak through pages or logs.
- Administrator pages actually integrate user enable/disable, device revocation/restoration, registration policy/invitations, sessions and audit. Server authorization remains authoritative; frontend role checks control display only.
- Rust client logic goes into a reusable crate. Frontend layout adapts to narrow screens in preparation for mobile interfaces. Do not install/run an iOS environment or start a new task in parallel in this item.

<a id="验收与运行边界"></a>

### Acceptance and runtime boundaries

- Commit npm/Cargo lockfiles. Run frontend type checking, production build and necessary behavior tests; `cargo fmt --all -- --check`; client-crate tests; Windows Tauri `cargo check/build`; and applicable Clippy. Record actual commands in the evidence document.
- Use isolated farsail-dev PostgreSQL/Mailpit to actually test registration/verification/login, native-client binding/signatures, device listing, refresh/revocation and administrator paths. Fake HTTP responses cannot replace this integration validation.
- Use a browser or available client to test actual interface layout, empty/error states and interaction. Explain the separate scope of browser previews and Tauri native-capability verification; do not present previews as native end-to-end testing.
- Test processes and local accounts/credentials must not enter Git. At completion, record all started processes, ports and retained state, and clean up owned services no longer needed. Tests may use only this project's isolated instances.
- When complete, update records, commit independently and push, verify CI and remote state, then hand off to WI-003. Core functionality cannot be replaced by TODO/stubs.

<a id="记录-1"></a>

### Record

Implemented Windows Tauri 2 + React/Vite client, reusable native Rust client crate, DPAPI user-scoped credentials, Ed25519 device identity, login/refresh/heartbeat, complete device enumeration and filtering, authorization request/approval/denial/revocation, account security and administrator pages. Browser checks cover responsive layout only; `state` and login commands in actual Windows WebView2 passed IPC smoke tests. Over real HTTP against isolated PostgreSQL, the client completed registration, verification, binding, authorization, administrator and logout flows. For implementation details, commands and risks, see the [WI-002 verification record](verification/WI-002.md) and [client documentation](CLIENT.md).

Inputs to WI-003: the native layer provides `NativeClient::take_grant_for_transport` and `renew_grant_for_transport`; only the approving target device obtains a grant. The next item must add authenticated signaling to deliver it securely to the requester, binding the iroh handshake, lease renewal and disconnection revocation to both public keys, nonce, session ID, permission and deadline. `GET /v1/remote/{id}` still returns only state and cannot be used to retrieve a grant. There is currently no validation of a public server, valid HTTPS IP certificate, second Windows computer, P2P/relay or physical mobile devices. The client keeps `can_host=false` and `can_files=false`; authorization UI must not be interpreted as actual remote control.

Coordinator acceptance: WI-002 implementation `e507412` and handoff commit `a1c9cf68a23724612243fb267c24d2c43e40fa2c` were pushed. Real PostgreSQL/HTTP, Windows DPAPI, WebView2 login IPC, local compilation and Windows/Linux CI passed. Development processes stopped and workspace clean. Approved proceeding to WI-003.

<a id="wi-003认证的加密传输与中继"></a>

## WI-003: authenticated encrypted transport and relay

<a id="基线与-write-set-2"></a>

### Baseline and Write Set

Startup HEAD `e0c610678cddc2d3371b0246a52969f19d290d32`, workspace clean; local `main` is one authorized WI-003 plan commit ahead of the remote. All code changes are within this section's Write Set.

Allowed changes: `crates/transport/**`, `crates/core/**`, `crates/client/**`, `services/coordinator/**` (new migrations only; do not modify committed historical migrations), `apps/desktop/src-tauri/**`, `apps/desktop/src/**`, `packages/ui/**`, `Cargo.toml`, `Cargo.lock`, `deploy/relay/**`, `scripts/test-transport.ps1`, `.github/workflows/transport.yml`, necessary compatibility/cache configuration in existing backend/client CI, `README.md`, `docs/IMPLEMENTATION.md`, `docs/API.md`, `docs/CLIENT.md`, `docs/TRANSPORT.md`, `docs/verification/WI-003.md`.

<a id="必须交付-2"></a>

### Required delivery

- Use `iroh` / `iroh-relay` 1.2.x verified compatible with Rust 1.93 and commit lockfiles. Reuse the same Ed25519 identity as device registration. Connection-handshake public keys must match both authorized identities; do not generate a separate anonymous endpoint to bypass device registration.
- Add connection-address registration, updates and authorized queries validated with device credentials and the current online generation. Discovery information is returned only to authorized accounts/session participants; invalid devices cannot publish, and cross-account invitations cannot become an arbitrary device-address directory.
- After target approval, deliver the grant only to the correct requester over an authenticated end-to-end connection. The requester validates the grant with the coordinator; both sides check session, device public keys, nonce, permission and deadline before opening the corresponding data channels. Handshakes have timeouts, concurrency limits and replay handling. Denial, missing approval, incorrect identity and privilege escalation must be rejected.
- Implement encapsulated session data paths (control, media and file types/versions and size boundaries), providing actually usable bounded read/write interfaces for WI-004/005. An echo example alone is insufficient, and screen/file-content processing cannot be claimed complete in advance.
- Renewal, server revocation, logout, unbinding, disconnection and lease invalidation close established P2P/relay data paths. Enforce bounded leases using trusted server remaining TTL and a local monotonic clock, rather than arbitrary expiry supplied by the peer. Logout cancels local sessions first and cannot be blocked for long by slow HTTP requests. Control permission cannot be inferred from view.
- Local direct connections and self-hosted relay are configurable. Use actual/selected-path information to display direct/relay/connecting/closed and RTT. A configured relay URL does not mean relay is actually used.
- Tests disable public-address discovery, explicitly bind to loopback and disable UPnP/PCP/NAT-PMP. The client also does not automatically modify router port mappings by default. Relay binds only to local test ports; production deployment files retain configurable domain/public IP and normal TLS validation. The user's public machine has not yet been provided; do not deploy an external environment.
- Test forced relay separately: use actual 1.2 configuration such as `clear_ip_transports()` to disable IP direct connections and confirm the selected path. Do not manufacture conditions by changing the global system firewall. Relay TLS correctly validates test CA/certificates; accept-all verifiers are prohibited.
- Connect native transport state/requests to the existing Tauri interface while keeping long-lived credentials and grants out of JS. Retain truthful labels for unimplemented media/file capability; only the next item may enable corresponding host capability.

<a id="验收-1"></a>

### Acceptance

- Two real local endpoints: authorization handshake, bidirectional data, rejection of wrong public keys/missing approval/expiry/replay/permission mismatch. Verify that existing streams cannot continue carrying application data after revocation or disconnected lease expiry.
- Complete at least one full transport integration using real coordinator HTTP/PostgreSQL and device binding. Forced local TLS relay must also transfer and validate data, with actual-path checks. Tests simulating authorization callbacks alone are insufficient as complete integration evidence.
- `cargo fmt --all -- --check`, applicable transport/client/backend tests and Clippy, frontend typecheck/build and Windows Tauri compilation. Validate new migrations separately in the existing project database and isolated test database; do not delete previous data to hide migration issues.
- Retain deployment instructions and a pending-acceptance checklist for public NAT and a second device. Do not describe loopback success as completed public traversal-rate verification.
- Update all runtime ports/process state and next-item interfaces; commit/push and check CI/remote state. Capture reusable conclusions according to the skill.

<a id="记录-2"></a>

### Record

Implemented `crates/transport` encrypted iroh 1.2 QUIC reusing device keys, bounded typed data frames, actual selected path/RTT and local monotonic short leases. The coordinator adds independent new migrations and registers device addresses against the registered public key and online generation; only participating devices in an approved session can query them. Relay URLs are restricted by HTTPS/credential format and the server allowlist. The target delivers grants only over authenticated connections. Both sides check session, public keys, nonce, exact permission and valid server TTL; renewal and revocation close existing channels. Old approvals from disconnected sessions cannot be reused after endpoint restart. Tauri pages can start/connect/view paths, but `can_host=false` and `can_files=false`; media handling and file contents remain unimplemented. For detailed interfaces, running instructions and verification evidence, see [authenticated transport](TRANSPORT.md), [API](API.md), [client](CLIENT.md) and the [WI-003 verification record](verification/WI-003.md).

Local two-endpoint direct connections, forced local TLS relay, real PostgreSQL/HTTP authorization integration and in-place migration of the existing development database passed. Implementation commit `e9e119a17e485da97d288913b63e5ff809cc45d9` was pushed; backend, transport and Windows client CI succeeded. The user has not yet provided a public server or second device; public-IP certificates, cross-NAT operation and mobile clients remain unverified. For process cleanup, commands and runtime links, see the verification record. WI-004 can directly use native `Session::send/receive` with its `Media`/`Control` channels, enabling host capability only after actual capture and input confirmation are implemented.

Coordinator acceptance: WI-003 final remote `e4992a86f35a1b0dd84f9ad211229e40abcb1edd`, workspace clean. Direct connections, forced TLS relay, real authorization integration, fresh approval after disconnection and all three CI workflows have evidence; proceeding to WI-004 is allowed. The user reiterated that the public server remains in preparation, so local development continues.

<a id="wi-004windows-屏幕与输入"></a>

## WI-004: Windows screen and input

<a id="基线与-write-set-3"></a>

### Baseline and Write Set

Code baseline `e4992a86f35a1b0dd84f9ad211229e40abcb1edd`; HEAD after this section's plan commit is the startup baseline.

Allowed changes: `crates/windows/**`, `crates/media/**`, `crates/core/**`, `crates/transport/**`, `crates/client/**`, `apps/desktop/**`, `packages/ui/**`, `services/coordinator/**` (only actual integration fixes such as host capability/participant display; add new migrations without changing historical ones), `Cargo.toml`, `Cargo.lock`, `package.json`, `package-lock.json`, `.gitignore`, `scripts/test-remote.ps1`, `.github/workflows/remote.yml`, necessary compatibility and Rust build cache in existing workflows, `README.md`, `docs/IMPLEMENTATION.md`, `docs/API.md`, `docs/CLIENT.md`, `docs/TRANSPORT.md`, `docs/REMOTE.md`, `docs/verification/WI-004.md`.

<a id="必须交付-3"></a>

### Required delivery

- Native Windows monitor enumeration, selection and DXGI capture. Candidate: the Rust `windows-capture` 2.0.1 `dxgi_duplication_api` (without Python/Qt), or an equivalent verifiable implementation. Respect monitor adapter ownership, row pitch, rotation, DPI and negative coordinates; handle timeouts, ACCESS_LOST/monitor changes and resource release. Screenshot data stays in local memory or ignored test directories and does not enter Git.
- The first actual media path uses an explicitly labeled low-frame-rate JPEG baseline through existing authenticated iroh sessions; Tauri receives binary frames and displays real screen data. H.264/H.265 and 4:4:4 await measured integration in WI-006. Options or labels cannot stand in for encoding capability. Frames include monitor/layout version, sequence number, dimensions, time and codec identifier; verify actual encoded-image dimensions and pixel budget.
- Bounded latest-frame queues and basic bandwidth/FPS control, reducing repeated work on unchanged screens. Existing transport generic IO timeout is 10 seconds; media needs appropriate frame deadlines and reset/drop handling so stale JPEG frames cannot block new frames or reliable input. Unbounded queues and fake fixed-FPS metrics are prohibited.
- Actual Windows system adapters for mouse movement, left/right buttons, scrolling, keys/key combinations and text input. Correctly map coordinates between the display area, scaling/letterboxing and remote monitor. View grants cannot inject input; field allowlists and message volumes are bounded. Recheck active session/generation at execution, discard queued input after cancellation and release every key pressed by this session, preventing old key-down events from running after release-all.
- Local host sharing enablement, explicit view/control approval, continuously displayed status and immediate stopping. Window closure/logout/lease invalidation stops capture and input. Stop local resources before waiting for HTTP revocation. The system secure desktop/UAC/logged-out desktop remain unsupported; display the actual reason at this boundary without bypassing system permissions.
- Registration/updates of `can_host` must match the actual Windows backend and local switch, allowing a real same-account client to list and connect to the host. `can_files` still awaits WI-005. Fix necessary API/operation ordering to make ordinary two-client flows usable; manually changing database capability is not an acceptable demonstration.
- Add the actual viewer page, monitor selection, scaling/window fit, view-only/control state, stop button and network RTT/actual frame rate. Approval windows show meaningful requesting account/device information. Both sides display comparable verification codes derived from the connection TLS exporter and session context, never grant tokens. Network RTT must not be presented as end-to-end screen latency.
- Add desktop single-instance enforcement or explicit exclusive configuration-directory locking to prevent two launches sharing a refresh token and revoking each other; test identities use separate temporary storage.

<a id="验收与资源边界"></a>

### Acceptance and resource boundaries

- Capture at least one frame from a real local monitor, encode it, transfer it through an authorized connection and decode/verify it. Prove this is the actual native path while retaining synthetic images for stable protocol, size, coordinate and backpressure tests. Do not upload actual desktop pixels or private-window contents.
- Input tests use harmless data only in self-created controlled test windows/receivers. Confirm the foreground belongs to that window before execution, then release keys and clean up afterward. If foreground cannot be obtained safely, record it as unverified. Do not inject keys into the user's terminal or other apps, and do not claim simulated functions prove successful system injection.
- Cover denial/read-only permission, queued input on expiry/revocation, focus-loss release, invalid coordinates/frame dimensions, stopping streams on disconnection, slow consumers, unavailable monitors/capture loss and single-instance behavior. Tests correspond to implementation risks; browser previews cannot replace native screenshot/injection acceptance.
- Complete real service + client + transport regression, frontend typecheck/build, Rust tests/fmt/Clippy and Windows Tauri build. Where CI lacks an interactive desktop, explicitly distinguish runnable synthetic tests from local native smoke tests. Existing three cold-cache builds are noticeably slow; credential-free Rust dependency/target caching may be added and verified.
- Update running instructions, explicit support scope, process/port cleanup and next-item interfaces; commit and push, check CI/remote state, and capture knowledge. If a public server or second computer remains unavailable, continue local implementation and record the gaps.

<a id="记录-3"></a>

### Record

The native Windows layer captures using the monitor's owning DXGI adapter and handles DPI, negative coordinates, rotation and row pitch. The first media path is an explicitly labeled low-frame-rate JPEG baseline with monitor, layout, sequence, dimension and time metadata. Host and viewer use existing authenticated iroh sessions, latest-frame/ACK handling and payload budgets. Native Windows system input executes under control permission, the local sharing switch, session generation and layout snapshot; injected input is released after stopping. Ordinary clients enable/disable `can_host` through the heartbeat-generation capability API, without manual database changes. `can_files` remains false. The Tauri viewer receives raw binary IPC and displays both sides' TLS exporter verification codes, path RTT, actual FPS, permission and stop controls; the configuration directory uses an exclusive lock. For interfaces and running instructions, see the [remote-control documentation](REMOTE.md); for commands, real capture/safe input evidence and remaining environment gaps, see the [WI-004 verification record](verification/WI-004.md).

Real local PostgreSQL/HTTP/iroh authorization and DXGI → JPEG → decoding passed. Harmless text, key combinations and clicks in a controlled test window passed, as did WebView2 binary IPC and single-instance smoke tests. H.264/H.265/4:4:4 await WI-006; the file channel awaits WI-005. The user has just informed the coordinator of the public deployment target. This item did not connect to external machines; independent server integration and a second Windows/cross-NAT validation are required later.

Implementation commit `78541d08d885b6827c9ed8c22a1aa7787e1f365d` was pushed and checked with `git ls-remote`. The three GitHub Actions workflows were still running at handoff; links and final local port/process state are in the [WI-004 verification record](verification/WI-004.md). The local Write Set was closed out and the workspace clean, allowing the coordinator to serially begin deployment-bundle/server integration while continuing read-only checks of CI completion.

Coordinator check: final commit `a66e43f3b7a42f7c6ee1a565bc96c8bc66511770` matches remote main and the workspace is clean. WI-004's writing task is complete, with no local running processes. The user requested public deployment, so execute WI-008A serially before returning to WI-005/006/007. Unfinished CI does not mean passed CI. If it fails, the current writing task fixes it within explicit scope; the coordinator does not change code in parallel.

<a id="wi-008a公网-ip-部署包与服务器联调准备提前执行"></a>

## WI-008A: public-IP deployment bundle and server-integration preparation (executed early)

<a id="背景与顺序"></a>

### Background and order

The user prepared a public Ubuntu 24.04.2 x86_64 server with about 1.6 GiB memory and no Docker/Swap. The coordinator is guiding Docker installation. Server addresses and login information are stored only in the current conversation/ignored local records; the public repository remains parameterized. Current Codex has no SSH session to take over and no key, so it must not claim connection or remote deployment. First complete an independently verifiable Linux deployment bundle, then have the coordinator help the user run it. WI-005/006/007 continue afterward; this does not mark the entire product complete.

Later preparation update: the user installed Docker 29.8.1 / Compose 5.5.1 and cloned the code. Outbound DockerHub pulls time out on the Alibaba Cloud server, so delivery changed to an offline archive containing all six runtime images. The exact server product and GitHub attachment download availability still need confirmation. The user supplied a nominal 200 Mbps figure, not measured throughput. SSH was not taken over and the external server was not modified.

<a id="基线与-write-set-4"></a>

### Baseline and Write Set

Code baseline `a66e43f3b7a42f7c6ee1a565bc96c8bc66511770` (WI-004 local acceptance passed; CI tracked read-only by the coordinator); HEAD after this section's plan commit is the startup baseline.
Allowed changes: deploy/production/**, deploy/relay/**, Dockerfile or dedicated Dockerfiles under deploy, .dockerignore, scripts/deploy/**, scripts/test-deploy.ps1, .github/workflows/deploy.yml, necessary Cargo.toml/Cargo.lock compatibility, services/coordinator/** (only necessary fixes such as internal relay admission/deployment configuration/SMTP port; new migrations without changing historical ones), crates/client/** or apps/desktop/** (only actual public-address/relay-configuration blockers; record exact scope first), README.md, docs/DEPLOYMENT.md, docs/IMPLEMENTATION.md, docs/API.md, docs/verification/WI-008A.md. Do not implement a file engine or hardware codecs, or modify completed WI-004 media logic.

<a id="必须交付-4"></a>

### Required delivery

- Reproducible Linux amd64 coordinator and pinned iroh-relay 1.2.0 runtime images/release bundle. The server retrieves only prebuilt artifacts and does not compile Rust on the 1.6 GiB host. Build from committed lockfiles, including necessary CA certificates and migrations. GitHub Actions generates downloadable artifacts or public GHCR images with commit identifiers and digests; actually verify existence and anonymous availability before writing commands. Do not publish unverified latest as though it were a pinned version.
- Linux Docker Compose production/integration configuration with an independent farsail-prod project and volumes. The coordinator currently enforces loopback, so container-network reachability must actually be solved using Linux host networking or a shared network namespace; do not show a 0.0.0.0 binding when the binary immediately exits at startup. PostgreSQL, the SMTP test inbox and internal management/admission APIs stay on private networks/loopback. Public defaults: API 443/TCP, relay 8443/TCP, ACME 80/TCP (configurable; check existing ports).
- IP certificates with normal CA validation, Certbot >=5.4 webroot+shortlived/IP support, HTTP-01 bootstrap, automatic certificate renewal and service reload/restart hooks, with visible renewal failure. Existing templates have both relay and API using 443 and must separate them. A cloud host's public IP may be a NAT mapping absent from its network interface. Public URLs use the user's IP, but listeners use actual local 0.0.0.0/interface addresses; binding PUBLIC_IP directly can cause EADDRNOTAVAIL and is prohibited. A domain may also be configurable, but do not require the user to buy one or disable TLS. Without the public machine and authorization details for an interactive account, run local test-CA rehearsals only; real CA issuance is performed by the user with explicit commands.
- Relay must not default to unlimited use by arbitrary devices. Prefer reusing upstream AccessConfig::Http: POST with X-Iroh-NodeId, public key already verified by relay, and a private Bearer token protecting the coordinator's internal admission endpoint. Admit only currently valid registered/enabled devices; reject unknown/disabled/missing-token/service-error cases. The API reverse proxy prevents public access to internal endpoints. Verify binary schema, timeouts and rate limiting, and limit client_rx. The 1.2.0 source explicitly leaves accept_conn_limit/burst unimplemented, so they cannot be treated as effective protection. There is no need to write an entire new relay server. Preserve application authorization and 30-second lease revocation, validating them separately from relay admission.
- Parameters and secrets come from server-side ignored files/mounts. Random passwords are not printed or included in Git/image layers/CI output; certificate private keys stay on the server. Use non-root containers where available, minimal configuration permissions, persistence, backup/restore, pinned-ref upgrades and workable rollback. Do not change global Docker configuration, clean up other containers or expose database ports publicly.
- The user explicitly chose loopback-only Mailpit for initial integration. Account-verification email needs an actually workable flow: support complete TLS SMTP configuration. Without SMTP, provide a clearly labeled test profile and loopback-only Mailpit, accessed through the user's own SSH forwarding. Do not expose the inbox or skip email verification. Administrators still use the existing explicit bootstrap command; first registration must not automatically elevate privileges.
- Client-setting examples accurately distinguish API URL from relay URL and explain that the local default 127.0.0.1 binding cannot be used directly across networks. Only actual selected relay paths count as passing relay tests. If cross-NAT P2P discovery remains unverified, explicitly record the gap instead of claiming traversal merely because server startup succeeds.
- Provide short Ubuntu 24.04/root/amd64 steps. Each copied block is a single executable command or committed reviewable script, avoiding multiline paste concatenation in the user's terminal. Stages have checks and error stop points. Do not run test-script database clearing/development migration resets publicly. Basic checks, dependency installation, cloning a pinned commit, configuration initialization, certificates, startup, healthz, registration/administrator, logs, stopping/upgrading must be followable.

<a id="验收与运行边界-1"></a>

### Acceptance and runtime boundaries

- Actually build and run an isolated deployment in local Docker, confirming database-backed health checks, persistence, internal ports restricted to loopback/private networks, configuration and migrations present, and no credentials in images/public logs. Use the independent `farsail-deploy-test` project, with host-published test ports restricted to loopback (candidate HTTP 58080, API 58443, relay 58444, database 55433; check before actual use), avoiding farsail-dev or other-service conflicts. Do not alter Docker Desktop's global host-network setting for tests.
- Correctly validate the test CA for HTTPS API and relay endpoints, registered-device admission and unknown-identity rejection. Internal endpoints must be unreachable through the reverse proxy; relay downtime/certificate misconfiguration has understandable failures. OpenSSL/curl may check SAN/IP and chain; -k cannot be evidence of success.
- Separately record the verified layers for Linux images, local Compose, public-artifact downloads, production SMTP, real CA, public-network/home-computer operation. Do not claim remote deployment.
- Applicable backend fmt/tests/clippy, deployment-script syntax/Compose configuration, GitHub CI/artifact checks and necessary existing auth regressions. Stop this item's test processes/containers while retaining needed volumes, and record runtime/cache boundaries.
- Update authoritative records and capture knowledge. After committing/pushing, verify remote state and notify the coordinator to continue the user's server steps. If the user adds SSH access, the coordinator assigns subsequent real-deployment actions; this task does not guess passwords or modify the user's server.

<a id="当前实现记录"></a>

### Current implementation record

At startup, HEAD `58db323bdc523132377903e83b853a08c6a981a5` and a clean workspace were verified. Changes stayed within this item's Write Set. With additional coordinator authorization, the final successful results of all three CI workflows were appended to `docs/verification/WI-004.md`. Deployment code is Bash and test probes are Rust, with no custom Python/Qt.

A shared gateway network namespace keeps the coordinator on loopback. Internal relay admission uses an independent route, private Bearer token, valid device/account/login checks and a 2-second database deadline; the loopback admission proxy limits upstream connection/read/write deadlines. Public API blocks /internal, and relay receive limiting uses actual effective 1.2.0 fields. API uses 443 and relay 8443; listening addresses and public URLs are separate. Initial loopback Mailpit still performs real email verification, and administrator bootstrap is explicit. The six-image offline bundle validates pinned tags + image IDs and uses pull never throughout; start recreates all shared-namespace dependencies together. Certificates use Certbot 5.4.0 short-lived IP/webroot, visible-failure systemd renewal and Manual certificate reload/relay restart.

Runtime boundaries: farsail-deploy-test only, loopback 58080/58443/58444/58026/55433, with private test state and image caches retained in .local and this project's Docker volumes. Farsail-dev and other containers are not modified. Docker build, real test CA/SMTP/identity/relay and artifact-release results are continuously recorded in [WI-008A](verification/WI-008A.md).

Runtime images are pinned to `4252e9d901e3022175772f563be45df967c3e9cc`; [release CI 36400484173](https://github.com/wanghao9103/farsail/actions/runs/36400484173) succeeded. The deployment loader is pinned to `51e131f0bf60a3a8fa15cddd869a8b97d3db79a0`; [compatibility CI 36402545193](https://github.com/wanghao9103/farsail/actions/runs/36402545193) succeeded. Complete anonymous download, byte-count/SHA256 validation, Docker 29 cross-store import and actual TLS + relay startup of the six-image public archive all passed. Backend/transport/Windows CI for the previous implementation 9d910e6 succeeded. Host database tests on the internal network were fixed only in test.override; the production database stays private. Test containers and ports were cleaned up, retaining volumes/private state. The coordinator continues user-server CA/deployment/cross-NAT work. Files, HEVC and mobile features are not marked complete; artifact links, digests, commands and boundaries are in the WI-008A verification record.

Cross-storage conclusion: classic and containerd inspect .Id semantics differ. 51e131f checks the cross-storage-stable image config SHA (including rootfs diffIDs) and exactly allowlists the 4252 manifest/archive SHA, allowing initialized users to reuse the old bundle without resetting secrets or redownloading nearly 300 MB. The 51e131f loader is required; the original loading script inside 4252 cannot be used directly. Initialized users wait for old load to exit, then fetch → checkout 51e131f → load-release 4252, skipping init. Only afterward does the coordinator guide real certificate issuance/startup.

Coordinator acceptance: final commit `8b46e19d8f70bda8f1511d95963466724cd08b91` matches remote main and workspace clean. Payload CI 36400484173 and loader CI 36402545193 were independently verified successful. WI-008A contains evidence for complete anonymous attachments, strict Docker 29 import and actual startup. This writing task ended. The user initialized the server and passed preflight and is continuing with the repaired loader steps; real CA/public services are not claimed complete. Begin WI-008B serially to provide a runnable installer for the second Windows computer.

<a id="wi-008bwindows-预览安装包"></a>

## WI-008B: Windows preview installer

<a id="基线与-write-set-5"></a>

### Baseline and Write Set

Code baseline `8b46e19d8f70bda8f1511d95963466724cd08b91`; HEAD after this section's plan commit is the startup baseline. WI-008A's writing task ended, workspace clean, and test services stopped. The coordinator continues guiding verified deployment commands on the user's separate public server; this item neither accesses nor modifies that server.
Allowed changes: `apps/desktop/**` (only necessary production-build, installation, first-run fixes/configuration, without expanding functionality), `scripts/package-windows.ps1`, `scripts/test-windows-package.ps1`, `.github/workflows/windows-release.yml`, necessary compatibility in the existing client workflow, `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `.gitignore`, `README.md`, `docs/IMPLEMENTATION.md`, `docs/CLIENT.md`, `docs/WINDOWS_INSTALL.md`, `docs/verification/WI-008B.md`. If a native crate has an actual release-build/startup blocker, record the exact path and fix before regression testing; unrelated refactoring is prohibited. Do not modify WI-008A's delivered deployment scripts, runtime bundle, user configuration or server.

<a id="必须交付-5"></a>

### Required delivery

- Produce an actually installable x64 NSIS preview from the current Rust/Tauri/React Windows client, with a reproducible packaging script and pinned-commit GitHub Actions. Retain the FarSail v2 icon, existing application identifier and current account/device/remote-control baseline. Users do not need Rust/Node to run it.
- Embed the production frontend in release without relying on a local Vite port. Handle WebView2 dependencies correctly and state internet/installation requirements accurately. Use only official Tauri mechanisms and verified dependencies, without Python/Qt. Do not replace the whole toolchain or change global user development configuration for packaging.
- Debug IPC probes and test-environment shortcuts cannot enter production behavior. Private keys, long-lived tokens, device tokens and grants remain in Rust/DPAPI only. First launch does not automatically register, share, inject input or enable unattended access; upgrade/uninstallation does not silently clear existing user credentials.
- Prefer per-user installation, avoiding default requirements for a system service/background sharing at startup. Do not require the user to buy a signing certificate. Without an existing signature, clearly state the package is unsigned; do not claim a trusted publisher or disable system protection. Prefer disposable Windows CI for installation/uninstallation tests. Locally verify only project processes in explicitly isolated directories without real credentials; overwriting existing installations or cleaning user configuration is prohibited.
- Artifacts include installer, source-commit/version metadata and SHA256, published as an explicit prerelease in the authorized public repository. Finally, actually download the public installer anonymously and compare its digest; cargo build success or green Actions alone cannot establish installer usability. Retain a portable package if needed, without adding unverified formats just to increase variety.
- Provide short second-Windows steps: download/verify/install, configure the user's HTTPS API and relay addresses, test-inbox verification, login/binding, target sharing/approval, verification codes and actual relay path, and stopping sharing. Parameterize documentation without hardcoding the user's public IP, email or credentials.
- Explicitly describe the current low-frame-rate JPEG viewing/control preview, with files/HEVC/mobile implementation later. Do not mark the whole product complete. The coordinator continues actual CA and two-computer acceptance for the public user instance; do not claim completion.

<a id="验收与运行边界-2"></a>

### Acceptance and runtime boundaries

- Lockfile dependency installation, frontend typecheck/build, Rust release/NSIS packaging, necessary formatting/Clippy and related regression tests. Prioritize actual risks; repeated full backend tests cannot cover untested installation.
- Check installer binaries, icons, resources and version. In a disposable Windows environment, verify installation/startup/uninstallation and basic usability of the actual WebView initial page/native commands. Distinguish production-startup evidence from earlier debug IPC evidence. Clearly state unverifiable layers; do not add unsafe production test backdoors.
- Native input, if tested, still uses only self-created controlled windows with foreground checked first. This item usually need not repeat already-passed capture/input tests. Do not save or upload actual desktop pixels, user credentials or private paths.
- Runtime/cache uses only this project's `target/`, `node_modules/`, ignored `.local/windows-package/` and necessary Tauri cache. Do not stop other apps or modify global proxy/certificates/firewall. Clean up processes started by the task and record retained state.
- Commit/push and verify exact remote state, CI, anonymous public files and actual digests; update handoff and reusable knowledge before closing this item. Return afterward to WI-005 files, WI-006 codecs/adaptation/multiple monitors and WI-007 mobile.

<a id="回查结论"></a>

### Knowledge review conclusions

This item narrowly reviewed Tauri credential/WebView boundaries and independent public-artifact acceptance notes. Applicable lessons: native credential isolation, debug/production build distinction, separate native IPC and web-preview verification, and complete anonymous artifact validation. The old notes' “remote control not yet implemented” status was superseded by WI-004 and no longer applies. Official build entry points and WebView2 options are checked against current Tauri documentation and pinned versions: https://v2.tauri.app/distribute/windows-installer/ .

<a id="执行记录"></a>

### Execution record

Startup HEAD `2d96e3e54f3515a48fceb065d3ad65e7e2a940f1`, workspace clean. Two notes on specified mechanisms were narrowly reviewed against existing code. Retained version 0.1.0 and the v2 icon; added the Tauri `custom-protocol` feature and release Windows GUI subsystem, using official NSIS currentUser + embedBootstrapper. First-time WebView2 absence still requires internet access. Tests do not install locally: installation occurs only on a disposable GitHub Windows runner. UI Automation scoped to this test PID/top-level HWND subtree drives the ordinary settings page to verify native IPC/persistence; reinstallation/silent uninstallation checks retention. No production test backdoor or desktop screenshot is added.

Production source/release tag is pinned to `6d79ef8e4c8336cee45840b5c9cd812fe08910dd`, prerelease `windows-preview-0.1.0-6d79ef8`. Installer size: 7720107 bytes; SHA256: `361e15a22db30be3c3009cad2da813caf81e9fe9e01b054ce6bfca848ce5c01e`. Complete [Windows installation CI 36405852502](https://github.com/wanghao9103/farsail/actions/runs/36405852502) succeeded. Real WebView settings save/restart, three native startups, same-version reinstallation and uninstallation data retention, and absence of debug probes passed; 8 client/1 desktop unit tests and Clippy passed. Existing Windows client CI 36404665949 for the native code also succeeded. The first round failed a strict EXE digest assertion because of Tauri's temporary bundle-type patch. Switching to the pinned CLI's official `--no-binary-patching` (no current updater) retained the assertion and passed. All four public attachments were downloaded completely and anonymously and matched each corresponding CI artifact's digest and byte count.

For pinned links, metadata, detailed evidence/boundaries, see [WI-008B](verification/WI-008B.md); for second-Windows steps, see [WINDOWS_INSTALL](WINDOWS_INSTALL.md). Unsigned; absence of WebView2 was not simulated, and cross-version data migration or the user's public two-computer setup was not verified. No local installation/real configuration changes and no started-process or test-port residue; only specified build/download caches were retained, and Docker services/volumes were unchanged. The coordinator continues actual CA/public two-computer acceptance; development returns to WI-005/006/007.

Coordinator acceptance: final commit `246a83dba70129d3d5dc5dfd73ff6c6fc02cecf5` matches remote main and workspace clean. Installation CI was independently verified successful, with complete public-installer and installation-lifecycle evidence; WI-008B ended. The user received the pinned installer link. The public server still awaits image-import feedback; certificates/service startup are not regarded as complete. Continue local WI-005 while the coordinator continues user-server steps.

<a id="wi-005双向文件传输与选择性无损压缩"></a>

## WI-005: bidirectional file transfer and selective lossless compression

<a id="基线与-write-set-6"></a>

### Baseline and Write Set

Code baseline `246a83dba70129d3d5dc5dfd73ff6c6fc02cecf5`; WI-008B's writing task ended and workspace clean. HEAD after this section's plan commit is the startup baseline. The coordinator independently guides user-server deployment; this item neither logs in to nor changes the user's server.
Allowed changes: crates/file-transfer/**, crates/core/**, crates/media/** (shared budget interfaces only, no new codec implementation), crates/transport/**, crates/client/**, apps/desktop/**, packages/ui/**, Cargo.toml, Cargo.lock, package.json, package-lock.json, scripts/test-files.ps1, .github/workflows/files.yml and necessary existing CI compatibility, README.md, docs/IMPLEMENTATION.md, docs/FILES.md, docs/CLIENT.md, docs/TRANSPORT.md, docs/verification/WI-005.md. Modify services/coordinator/** and docs/API.md only when actual file-capability/audit integration requires it, with new migrations rather than changing historical ones. Do not write WI-006 codecs or mobile-platform implementations.

<a id="必须交付-6"></a>

### Required delivery

- Use Rust/Tauri. Custom scripts use PowerShell/Bash/Rust only, without Python/Qt. A reusable native file engine and actual Tauri file panel: start independently from device cards or establish a separate Files authorization session from remote-control pages; multiple upload/download tasks, progress, actual rate, pause/resume, cancellation and failure retry. Same-account requests still require explicit target confirmation.
- Route sessions by authorization type: RemoteRuntime must not start or close Files sessions as screen sessions. File and screen engines own separate session receivers to avoid competing over the same receive.
- Files cannot imply screen/control permissions, and view/control cannot read/write files. The host selects local scope and read/write permissions, with no sharing by default. The remote side sees only opaque resource IDs and permitted relative names and cannot specify arbitrary absolute local paths. Declare can_files only when natively supported and locally enabled. Sharing and file switches are independent and reuse WI-004 operation-generation/cancellation constraints. Delayed enablement responses cannot restore capability after stopping/logout. Closing screen sharing must not silently revoke an independent file session that remains explicitly allowed.
- Enforce scope through authorized directory/file handles. Avoid replacement races from canonicalize+starts_with followed by reopening by path. Implementations such as cap-std or Windows handle-relative access must verify reparse/junction, parent-directory replacement, hard links/existing-file overwrite and ADS boundaries. Receive into newly created temporary files, without silent overwrite or automatic opening/execution.
- Bounded chunk protocol, sequence numbers, version, transfer ID, offsets, original/encoded lengths, none/zstd negotiation and per-chunk/final SHA-256. Stream reads/writes without loading entire files into memory. File metadata, block ordering and commit acknowledgment are bounded and reliable. Actually use existing authenticated iroh channels; do not disguise local copying as transfer.
- Selective Zstd lossless compression runs before encryption and after receiver decryption, passing through when compression gives no benefit. Skip already-compressed content by default and measure compressible text. Each chunk is independent, without cross-message dictionaries; bound original length, compression window, cumulative disk/memory and concurrent tasks. Bounded workers perform compression/hash/file I/O without blocking Tokio input or UI.
- Pause/disconnection preserves verified progress, allowing resume only after fresh authorization. Progress binds the account/both devices/resource/source-content version. Stable source snapshots or appropriately held handles prevent stitching different versions together; recovery verifies stored chunks against the source. Mark complete only after final hash validation, same-volume no-overwrite commit and peer commit ack. Disk-full/corruption/cancellation must not leave false completed files. Recovery state contains no long-lived tokens.
- Configurable file rate limits prioritize input/heartbeat and interactive screen data. Multiple tasks/files and media share a total budget to reduce background traffic; separate QUIC streams alone cannot establish priority. Slow consumers stay bounded. Revocation/logout/termination promptly stops all reads/writes without reviving old queued tasks.
- Preserve compatibility of released account/device/viewing/control flows and stored credentials. New capability updates must not reset the other sharing switch. Give explicit upgrade prompts for new APIs unsupported by old deployment bundles; do not fabricate can_files. Old Windows previews/server images remain separately pinned versions, with no rewriting of existing release attachments. Distinguish source functionality from released versions.
- Save to an explicitly user-selected Windows location. Design handle interfaces for mobile document providers without claiming mobile delivery in this item. Audit only necessary metadata, without contents, sensitive full paths or access credentials.

<a id="验收与运行边界-3"></a>

### Acceptance and runtime boundaries

- Independent Files sessions and independent Files authorization within remote control; actual two-endpoint iroh upload/download, multiple files, empty files, Chinese names and files larger than the memory window. At least one file-content round trip through local direct connections and forced TLS relay.
- Real service/client authorization negatives: reject unauthorized/read-only/write-only/expired/revoked/account-switched operations. Corresponding states for scope escape, reparse points/directory replacement, no-overwrite same-name handling, wrong offset/size/hash/codec/window, compression bombs/truncation and cancellation/insufficient space/lost commit ack.
- Resume after fresh confirmation following disconnection; reject stitching after source changes, and prevent stopped streams/expired tasks from continuing writes. Measure input latency/queue boundaries with rate-limited files and media transferring simultaneously. Record measured numbers and conditions without promising a fixed compression ratio.
- Use synthetic or public project assets as file content; do not read/transfer arbitrary private files. Ignore test directories and task storage. Still use only the farsail-dev local database/email and local TLS relay, without public deployment.
- fmt/tests/clippy, frontend typecheck/build, Windows Tauri build and related existing regressions/CI. Record and close out started processes/ports. Update documentation, capture knowledge, commit/push and verify remote state before handoff.
  <a id="回查与依赖线索"></a>

### Knowledge review and dependency leads

Reviewed this project's short-lease transport and media-cancellation notes: every application operation must recheck lease/generation, wait queues are bounded, logout stops local IO first, and identity switching cannot accept delayed results. Extending these mechanisms to file operations still needs focused verification. cap-std 4.0.3 Dir/from_std_file documentation requires Windows handles without FILE_SHARE_DELETE to avoid races; verify link and no-overwrite commit semantics of the chosen APIs in practice. Zstd Decompressor provides output-capacity/parameter limits, and both original length and compression window must be bounded. References: https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html , https://docs.rs/zstd/latest/zstd/bulk/struct.Decompressor.html . Library versions and Windows behavior follow current builds/tests; API names alone cannot establish safety.

<a id="wi-008c部署下载断点续传修复"></a>

## WI-008C: deployment download-resume fix

<a id="触发基线与写入交接"></a>

### Trigger, baseline and writing handoff

The user downloaded an approximately 284 MiB archive from GitHub on the public machine and encountered curl 92/HTTP2 PROTOCOL_ERROR after more than 40 minutes. The existing downloader writes to .part but overwrites from the beginning on rerun, lacking reliable resume. The coordinator provided an immediate alternative: upload the same public bundle already validated locally with scp, then import it with load-release from an offline directory.
Code baseline `69b2817ff39bb0b9220e45affce7a972561951f0`; HEAD after this section's plan commit is startup HEAD. After atomic editing, WI-005 was explicitly quiescent with no build/test processes. All writing and commits paused, preserving uncommitted WIP: Cargo.toml/Cargo.lock, crates/file-transfer/**, crates/client/src/lib.rs, crates/transport/src/lib.rs, services/coordinator/src/device.rs, apps/desktop/src-tauri/Cargo.toml and src/files.rs/src/remote.rs. These changes have not passed acceptance and must not be staged, committed, rewritten or released. At completion, the coordinator explicitly returns write access to WI-005.
Exact Write Set: scripts/deploy/farsail.sh, new scripts/deploy/download-related Bash/Node tests and helpers, .github/workflows/download.yml, docs/DEPLOYMENT.md, docs/verification/WI-008C.md, docs/IMPLEMENTATION.md. Do not change Cargo/application logic/image contents/old release attachments, rebuild images or log in to the user's server.

<a id="必须修复与验收"></a>

### Required fixes and acceptance

- Production downloads explicitly use HTTP/1.1, normal HTTPS and bounded connection/no-progress timeouts. Every retry issues a fresh Range request from the existing .part length, retaining progress gained before failure/interruption and avoiding curl internal retries rolling back the current attempt's progress. Permanent HTTP/certificate/Range errors fail clearly, without bypassing TLS or looping indefinitely.
- Use known SHA256 to detect an already complete .part first. Even if the previous network finalization failed, a matching digest lets it be adopted without a network request. Unknown/corrupt contents must not become final files. Digest mismatch retains evidence and fails rather than silently deleting and redownloading.
- Handle small SHA256SUMS/manifest files separately from the large archive. Obtain valid verification information first, then resume and validate the large file. Support existing caches and initialized state without init. Retain existing exact legacy425 allowlisting and portable config SHA/rootfs/platform import checks; source_dir offline import continues to make no network access.
- Meaningful local network-failure tests: a test server capable of HTTP2 negotiation confirms the client uses HTTP1.1; after deliberate interruption, the next request's Range offset advances and final bytes/digest match; a complete .part is adopted while offline; corrupt digests are rejected; a server refusing Range does not damage the downloaded portion; bounded retries and TLS rejection. Use Node/standard utilities and a test CA, without Python/Qt or user-file access. Test listeners bind only to loopback and owned processes are cleaned up afterward.
- A small anonymous HTTP1.1/Range check against the original public 425 attachment is sufficient. The complete public bundle is already local, so do not redownload/release the nearly 300 MB archive to verify a downloader. Necessary integration uses existing cache and strict verification; do not run Cargo against File WIP or mix in unfinished code.
- Update operating instructions: an alternative to upload the three verified files directly from the local computer, reuse of old .part files, and failure/progress information. Do not promise that nominal 200 Mbps bandwidth equals actual GitHub download speed.
- Stage only the exact files above and confirm File WIP is excluded. Commit/push the pinned fix version, corresponding CI/evidence and knowledge capture. Provide update commands that retain existing private configuration and caches. After completion, stop writing for this item and notify the coordinator to resume WI-005.

<a id="适用经验"></a>

### Applicable lessons

Known GitHub TLS/EOF experience supports only diagnosis along the actual connection path, command-level options and preserved certificate validation; it cannot attribute every failure to the same network cause. This case has actual curl 92 evidence and downloader source lacking -C/outer resume. The original six-image bundle was completely downloaded anonymously and passed SHA/runtime verification. The issue is delivery reliability and cannot be hidden by disabling checks or asking the user to download again from scratch.

<a id="ui-011--011-客户端交互修复2026-09-29"></a>

## UI-011 — 0.1.1 client interaction fixes (2026-09-29)

User acceptance feedback was consolidated: dark device list/details panel, local sharing and inbound approval, direct remote control/view requests and automatic connection after approval, continuous registration/verification flow, Chinese login-failure dialog and custom window bar. Source `9ad6175`, Windows installer CI `36511486164` and Windows client CI `36511468941` passed. `windows-preview-0.1.1-9ad6175` was released; anonymous download validation of all four public attachments matched. See `docs/verification/WI-UI-011.md`.

This release contains only client UI and interaction fixes. WI-005's uncommitted file-transfer WIP remains retained and absent from the release; WI-008C's download-resume fix still awaits implementation. The public production server does not need redeployment for this client update. Subsequent actual two-computer acceptance cannot be replaced by synthetic IPC tests.

<a id="remote-012012-远控可靠性与独立窗口"></a>

## REMOTE-012: 0.1.2 remote-control reliability and separate window

Implemented in an independent workspace on `024eb05` / plan `01817f0`. Includes lease-read races, out-of-order input, focus/disconnection queue cleanup, bounded media retries, separate native viewing windows and session-scoped IPC, cancellable reconnect with fresh authorization, and locally default-off “Remote standby”. Server routes/migrations are compatible with the 4252 release bundle and require no server upgrade. Real database/HTTP/QUIC passed a 70-second hold test spanning multiple 30-second leases. The sole cause of the user's approximately 30-second on-site disconnection remains unconfirmed; gaps and final release evidence are in [REMOTE-012](verification/WI-REMOTE-012.md). WI-005 WIP and WI-008C status remain unchanged.

Delivery complete: source `3519613` and `c65fb77` pushed. Final Windows installation CI `36517620603`, Windows client `36517618553`, transport `36517618586` and backend `36517618587` all succeeded, pinned to `c65fb775b50af8345d0f3716d566b233a683f060`. All four attachments of prerelease [windows-preview-0.1.2-c65fb77](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.2-c65fb77) were completely downloaded and anonymously verified, with matching results. Installer size: 7,843,396 bytes; SHA256: `016074d1f492913b138f92286fab6f97f48a307e6ce59a9d55eb96bc07c0ae33`. Native input, window IPC isolation/lifecycle, real local capture and connections across multiple leases passed. Physical two-computer and cross-NAT gaps are not claimed complete. Runtime closed out and knowledge notes updated; the detailed next entry point is the [final handoff](REMOTE-RELIABILITY-HANDOFF.md#completed-delivery--coordinator-entry-2026-09-29).

<a id="ui-012013014--013-桌面操作与计算机名称交付"></a>

## UI-012/013/014 — 0.1.3 desktop operations and computer-name delivery

In the independent desktop-ui-release workspace, this round's layout, operation flow and name reading were integrated on remote `5f8fb54`, retaining 0.1.2's separate native windows, input cancellation, bounded reconnect and explicit remote standby. Source `0987d42` and final fix `f2f5c6b` pushed. Final Windows client `36527793606` and installation CI `36527795316` passed. The first installation acceptance found inaccessible sharing state in the compact sidebar; it was fixed without skipping the assertion.

[0.1.3](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.3-f2f5c6b) was released. All four attachments were downloaded completely and anonymously and matched CI originals. Installer size: 7,853,825 bytes; SHA256: `dc8f43cc79af6d1b24757d217bd502653aef9b6e27bd1df258c1a851443ae258`. No server update is needed; each updated client reports its own computer name. For complete evidence and the next entry point, see [UI-RELEASE-013](verification/WI-UI-RELEASE-013.md). The original shared directory retains the old baseline and file-transfer WIP. Future UI/release work should reuse the current release workspace rather than overwrite the newer remote version with the old directory. Runtime stopped and real installation/configuration not overwritten.

<a id="input-014--普通鼠标操作后断开修复2026-09-30"></a>

## INPUT-014 — fix for disconnection after ordinary mouse operations (2026-09-30)

The user confirmed that moving/clicking the mouse in an ordinary application causes disconnection; an old screenshot shows input_failed. On the 0.1.3 baseline in the current release workspace, implemented virtual-desktop absolute SendInput, ordinary system-key support and a failure boundary that pauses control while retaining media on input rejection. Input state uses bounded monotonic generations. Explicit retry is separate from ordinary release, preserving existing authorization, sharing, session-cancellation and protected-desktop boundaries.

Source `40edacb2e17b1ac1d2dc04baa3457135fcb831e2` was pushed and [0.1.4](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.4-40edacb) released. Real input into a local self-created foreground window, GUI-less worker-thread positioning, authenticated loopback pause/continued media/recovery, two browser-regression groups and static checks passed. Final Windows client 36660400761, installer 36660402240, transport 36660400809 and backend 36660400832 all succeeded. All four attachments were completely downloaded anonymously with matching validation. Installer size: 7,849,691 bytes; SHA256: `7cbec092f01f7ee6f05d15fea1fcf4c163f4c5b0e41aeebdfb9b592e8186322b`.

Both computers need updating, especially the host; the server does not. The sole root cause of the original Windows refusal on the user's machine remains unreproduced, and the old screenshot cannot be attributed to UAC. The new version provides specific Windows return codes and prevents this kind of input rejection from directly closing video. For evidence and the next entry point, see [INPUT-014](verification/WI-INPUT-014.md). Local runtime closed; production installation/configuration and file-transfer WIP in the original directory unchanged.

<a id="viewer-015--大画面隐藏工具栏和高清切换2026-09-30"></a>

## VIEWER-015 — large screen, hidden toolbar and HD switching (2026-09-30)

On the 0.1.4 baseline in the current release workspace, the remote window defaults to maximized. The toolbar becomes a floating auto-hidden bar, with text and connection details expanded on demand. Transmission resolutions support 720p/1080p, defaulting to HD with improved chroma compression, without increasing pixel/per-frame/bandwidth budgets. Actual received dimensions can be inspected; original aspect ratio and mouse positioning through borders are retained. Updating both ends enables complete quality switching, without a server update.

Source `cf5b72aadf4854fe536fa6e4155ccc3585357da2` was pushed and [0.1.5](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.5-cf5b72a) released. Actual native default maximize/restore/fullscreen/closure isolation, DXGI in-memory capture, encoded color edges, browser full-client-area/hiding/positioning/switching and existing regressions passed. Windows installer 36666218872, client 36666217848, transport 36666217772 and backend 36666217762 all succeeded. Anonymous download validation of all four public attachments matched. Installer size: 7,871,208 bytes; SHA256: `d6d2871952212339754c599336f20159d8f015c8861d86d3bddbaf5c579e0f9c`. See [VIEWER-015](verification/WI-VIEWER-015.md). Test runtime closed; future work reuses the current managed workspace rather than overwriting released source with the old main directory.
