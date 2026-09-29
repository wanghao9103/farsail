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
- Pending CI, artifact publication and anonymous hash verification.
