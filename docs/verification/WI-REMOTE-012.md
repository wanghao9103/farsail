# Remote reliability / Windows 0.1.2

Baseline: `024eb05d89c65318b707605774897f0706574ff2`; planning commit `01817f0`. Work is isolated from the paused file-transfer WIP. No coordinator route, migration, deployed secret or production profile is changed. The coordinator remote/device implementations are unchanged from the published `deploy-4252e9d901e3022175772f563be45df967c3e9cc` payload; this client does not require a server upgrade.

## Findings and implementation

- The report says the connection ended after approximately 30 seconds; it does not establish that input worked before then. A real PostgreSQL/HTTP/QUIC test held a connection for 70 seconds across multiple production-cadence grant rotations, heartbeats and bidirectional data. The fixed-duration failure was not reproduced locally; production root cause remains unproven.
- The old synchronous input gate used `try_read` on the lease deadline. An in-progress renewal could therefore look like an expired session. The deadline now uses a short synchronous read/write critical section; no I/O or await occurs while that lock is held. A contention regression holds the write lock while checking the live input gate. The actual finite grant TTL and authoritative inspection are unchanged.
- The application now tolerates bounded input reordering without relying on an implicit cross-stream guarantee. The host now buffers at most 32 small input commands for at most 2 seconds, applies contiguous commands once, and closes safely on a missing, duplicate or excessive sequence. The actual iroh test delivered an incomplete earlier stream first; it verifies timeout/discard followed by a complete later frame, plus propagation of the close reason. It does not establish reordering as the cause of the reported failure. The UI explicitly focuses the desktop on mouse down, normalizes generic modifier keys, cancels queued work on blur/failure/closure and gates input again immediately before native send. Old display coordinates are discarded; actual input injection failure closes with a bounded public cause.
- Terminal status clears the JPEG URL, FPS and queued input. Public cause codes distinguish stop, authorization check, renewal, expiry, media timeout, capture and input failure without exporting tokens or raw transport logs.
- Media still has a 400 ms per-stream deadline and a 1.5 MB/s payload budget. An isolated dropped frame/ACK can retry; a static desktop retries its last available frame instead of waiting forever for new pixels. Duplicate images are acknowledged without redisplay. Each attempt waits at most 2 seconds for ACK, with at most 3 consecutive failed attempts (at most 3 unconfirmed bounded frames), then closes. New frames replace stale ones when available; this is a bounded JPEG baseline, not adaptive video.
- The viewer is an independent native window. Display selection, path/RTT and controls are compact; details are collapsed. Window actions apply only to the calling viewer. Closing it cancels recovery and revokes its current/pending session while preserving the main device manager. Main logout/close/revoke cancels affected recovery. Viewer commands enforce the bound session ID in Rust; account/admin/settings/hosting commands require `main`. Viewer webviews receive no generic cross-window permission or window-creation command.
- Network/media timeout may initiate up to 3 total automatic attempts per viewer lifetime, using fresh request IDs and fresh coordinator approval/grants every time. Refusal, revoked/expired permission, explicit stop, identity change and unavailable/disabled target do not revive the old grant. Cancellation invalidates the binding immediately and compensates late request/connection results. Old clients' ambiguous generic closure is terminal. Path migration inside an existing authenticated iroh connection is distinct from reconnect; the UI displays `P2P 直连`, `中继回退` or negotiation from the actual selected path. Forced relay stays opt-in.
- `远程值守` is a local, in-memory, default-off opt-in for the current sharing period. Text: “开启后，同账号设备可直接连接，无需逐次批准”. The native host compares the authoritative pending requester ID with the bound account; only view/control may be auto-approved. Existing coordinator identity/device/grant checks remain authoritative. Other accounts stay pending. Opt-out ends inbound sessions, removes/revokes outstanding local approvals, and resumes manual decisions. Epoch checks compensate an approval response that arrives after opt-out/stop/logout. Stop sharing, logout and process restart disable the opt-in. It requires a signed-in interactive Windows desktop and running application; it does not support UAC or the login screen.

## Verification record

- Real coordinator integration: isolated Docker project `farsail-reliability`, PostgreSQL 17, dynamically allocated loopback port, randomly generated ignored credentials, temporary DPAPI profiles, synthetic identities, schema cleanup. `cargo test -p farsail-client --test coordinator_flow` passed including 70-second production-cadence renewal, old-grant replay rejection, other-account non-approval, same-account default-off/opt-in/revoked-state behavior.
- Client unit regressions include delayed automatic decision compensation after opt-out; no grant remains locally and a revoke is sent. Existing host/transport start cancellation and offline logout checks remain.
- Browser tests `scripts/test-desktop-ui.cjs` and `scripts/test-remote-viewer.cjs` passed: register/login, sharing, request/cancel, native-window dispatch, focus/button/key submission, terminal image cleanup, queued input cancellation, no retry on terminal auth closure, single recovery invocation on a network closure, cancellation and view-only input suppression. Screenshots use synthetic graphics only. This is synthetic IPC evidence, not physical two-machine control.
- `cargo test -p farsail-windows real_input_into_own_foreground_window -- --ignored --test-threads=1` passed on this Windows host: harmless text, modifier/selection and click into the test-owned, foreground/PID-verified window. No private window was targeted and no desktop image was saved.
- `scripts/test-native-viewer.ps1` passed in a real Windows WebView2 process with its own DPAPI and WebView profiles: 8 forbidden account/admin/settings/hosting/other-session/cross-window commands were rejected; native maximize, fullscreen, minimize and viewer destruction were verified. The main window remained present and the process stayed alive after the viewer closed. This fixture does not claim a live remote media session.
- Frontend typecheck/production build, Rust format checks, targeted unit regressions and Clippy with `-D warnings` passed. Exact final CI/release evidence is appended after artifact verification.

## Limits and compatibility

Both endpoints should upgrade for reordered-input tolerance, richer causes and media retry behavior. The wire protocol/ALPN is unchanged and the old coordinator remains compatible; no server redeploy or new image archive is necessary. Static desktops may correctly show 0 FPS while the session is active. Screen capture remains roughly 5 FPS JPEG. No cross-NAT success rate, physical two-PC interaction, elevated-window control, unattended login, HEVC, files or mobile support is claimed. Installers remain unsigned previews.

References: [Tauri capability defaults](https://tauri.app/security/capabilities/), [iroh Connection API](https://docs.rs/iroh/1.2.0/iroh/endpoint/struct.Connection.html). Custom app commands are callable by default, so window labels and current session bindings are checked inside every relevant Rust command; capability names alone are not the security boundary.

## Final delivery (2026-09-29)

Implementation `3519613797c3469024a0ee9979afe76ffd340235`, follow-up `c65fb775b50af8345d0f3716d566b233a683f060`. The latter preserves the peer close cause before iroh local cleanup overwrites it, cancels an in-progress recovery dial when its viewer is cancelled, corrects the incomplete-stream test assumption and removes remaining misleading connected notices. Release tag and all following CI runs point to the exact latter commit.

| Check | Result / evidence |
| --- | --- |
| Backend | [36517618587](https://github.com/wanghao9103/farsail/actions/runs/36517618587), success |
| Transport / real database integration | [36517618586](https://github.com/wanghao9103/farsail/actions/runs/36517618586), success |
| Windows client / native viewer / UI | [36517618553](https://github.com/wanghao9103/farsail/actions/runs/36517618553), success |
| Release installer lifecycle | [36517620603](https://github.com/wanghao9103/farsail/actions/runs/36517620603), success |
| Local transport | 9/9, including TLS relay, expiry/revoke, renewal contention, incomplete stream recovery and close-cause retention |
| Local client / desktop / media / Windows unit checks | 9 / 2 / 3 / 6 passed; two interactive tests are separate from the default Windows unit suite |
| Real Windows input | Owned foreground-window test passed; no unrelated window targeted |
| Real capture and authorization | Final real coordinator flow passed in 91.55s, including 70-second connection hold and DXGI/JPEG round trip in memory |
| Native viewer IPC | 8 denied operations; maximize/fullscreen/minimize/independent close passed; main remained alive |
| Browser UI | Both checked-in scripts passed; additional 640×420 viewer check had no horizontal overflow; screenshots contain synthetic graphics only |
| Static/build | Frontend typecheck/build, Rust fmt, Clippy `-D warnings`, native debug build passed |

The initial transport CI `36517020950` failed on the new test's incorrect earlier-stream assumption; the corrected test then exposed loss of the peer close cause during cleanup. Both issues were addressed and the final full transport suite above passed. Initial installer `36517024650` and client `36517020854` passed but their artifacts were **not** published; only final installer run `36517620603` was used.

[Release](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.2-c65fb77) / [Installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.2-c65fb77/FarSail_0.1.2_x64-setup.exe) / [SHA256SUMS](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.2-c65fb77/SHA256SUMS.txt).

| Public asset | Bytes | SHA-256 |
| --- | ---: | --- |
| FarSail_0.1.2_x64-setup.exe | 7,843,396 | `016074d1f492913b138f92286fab6f97f48a307e6ce59a9d55eb96bc07c0ae33` |
| SHA256SUMS.txt | 95 | `7908a923c8648177eb3c72b29a44e90af3f381beae99b2905fdf41c41e91f8cd` |
| release-metadata.json | 651 | `a82a818f68613795d276cc53b696c8421f0e21535299509f288b6b061eecec15` |
| installation-verification.json | 422 | `dc36b391cb007552ebc362aaec1bc36e32cf02cc987aa162cf85f233315772a7` |

All four assets were fully downloaded anonymously with curl (no credentials or curlrc), and individually matched the CI artifact byte counts and hashes. GitHub tag `windows-preview-0.1.2-c65fb77` resolves to `c65fb775b50af8345d0f3716d566b233a683f060`. Installer is `NotSigned`. Native installation, embedded frontend/settings IPC and restart, same-version reinstall, uninstall retaining synthetic user data, and absence of debug probes passed on a disposable Windows runner. This is not a test of physical cross-network control or cross-version data migration.

Runtime cleanup: task-owned Vite and its esbuild child stopped; all debug/native test windows exited. `farsail-reliability` PostgreSQL container, task network and task volumes removed. No project executable remains running from the isolated checkout. Only ignored `target`, `node_modules`, `.local` build/download/test evidence remains. The primary paused file-transfer checkout, existing Docker services, production app/profile, user credentials and public server were not modified. The existing Obsidian media/input-cancellation note was updated with verified mechanisms and explicit evidence limits.
