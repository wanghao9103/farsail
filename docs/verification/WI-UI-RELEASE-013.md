**English** | [简体中文](WI-UI-RELEASE-013.zh-CN.md)

# Windows preview 0.1.3 — desktop operations and computer names

## Release baseline and boundaries

- Base: `5f8fb54` (remote main, verified 0.1.2). Isolated branch `codex/desktop-ui-013` in the managed desktop-ui-release worktree.
- Integrate UI-012/013/014 from the primary checkout. Preserve 0.1.2 native independent viewer, scoped IPC, input cancellation, reconnect/lease behavior and explicit same-account remote-watch controls.
- Primary checkout Rust/Cargo/file-transfer WIP is excluded. Do not change its files or publish it. No server update is required.
- Write set: desktop frontend, native computer metadata module and state endpoint, Tauri version, desktop UI tests and documentation. Existing 0.1.2 runtime code remains authoritative.
- Runtime: worktree node_modules/target/.local and local Vite 1420; disposable Windows CI for real installer lifecycle. Do not install over the user's real app/profile.

## Verification and release evidence

- Local `npm run build`, `cargo fmt --all -- --check` and `git diff --check` passed.
- Both browser regression suites pass after integrating the native-window flow. Main UI coverage includes local-only name migration/custom aliases/fallback, clear actions, invitations retained across page changes and cleared on sign-out, one-step sharing, recovery failure/retry, explicit remote-watch on/off, list scrolling at desktop/compact sizes. Viewer suite preserves focus/input, terminal frame clearing, queued input cancellation, reconnect cancellation and read-only permission checks.
- The child viewer continues using scoped `viewer_window_action`/`viewer_reconnect`; it does not call main-window account APIs. The computer metadata addition preserves `viewer::main_only` on `state`.
- Historical UI-012/013/014 documents describe their original baseline. For release 0.1.3, the independent 0.1.2 viewer is preserved; it supersedes the earlier embedded-viewer layout/end-flow notes. CI build and installer evidence below will supersede the original dirty-checkout Cargo-lock limitation.
- Initial installer run `36527204879` stopped at the signed-out sharing-status accessibility assertion. Compact sidebar CSS hid the status at the runner's effective viewport width. The status now remains visible as a dot with an explicit accessible name/tooltip; browser tests assert visibility at all three desktop sizes. The assertion was preserved. No artifact from that failed run was released.

## Published release

- Implementation: `0987d424f1ee3ff71f6dccc9ec145712cd5607b9`; final compact-status fix and release source: `f2f5c6b4a4dff913c43bd1ff6e8c043eb204a6df`.
- [Windows client CI 36527793606](https://github.com/wanghao9103/farsail/actions/runs/36527793606): success, including native viewer scope/lifecycle, both browser suites, Rust tests and Clippy.
- [Installer CI 36527795316](https://github.com/wanghao9103/farsail/actions/runs/36527795316): success, including locked release build, actual install/settings IPC/restart/reinstall/uninstall retention, debug-probe absence, targeted Rust tests and Clippy. The earlier primary-checkout lock inconsistency does not affect this clean release baseline.
- [0.1.3 release](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.3-f2f5c6b) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.3-f2f5c6b/FarSail_0.1.3_x64-setup.exe).

| Public attachment              |     Bytes | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.3_x64-setup.exe    | 7,853,825 | `dc8f43cc79af6d1b24757d217bd502653aef9b6e27bd1df258c1a851443ae258` |
| SHA256SUMS.txt                 |        95 | `77fc60845086173af656ef25eabf540bdbb60933f19ac1bad0ba966702720bc5` |
| release-metadata.json          |       651 | `5d522066b682c322612c7faf7401b722d76e29622311dbfb745431dabe18e0c4` |
| installation-verification.json |       422 | `cbb560dbb1c42edb946c3b7d57feeb463ffcdb66bf19f6a6e057db6392638fe0` |

All four public attachments were downloaded in full with unauthenticated curl (curlrc disabled), then matched against the tested CI originals by byte count and SHA-256. Installation report and metadata bind the same source/installer hash. Installer remains unsigned. No new physical two-PC/cross-NAT claim is made.

Runtime stopped: task Vite and browser tests exited, no production installation/profile or server touched. Keep this managed release checkout as the current code entry point; the primary checkout remains on the old baseline with prior UI edits and unrelated paused file-transfer WIP. Do not re-publish that stale checkout over this release. Reuse this checkout for follow-up UI/release work; reconcile the unrelated file-transfer work separately.
