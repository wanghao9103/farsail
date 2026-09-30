# Windows preview 0.1.6 — ordinary remote pointer

- Baseline `c72d76a`, clean current managed checkout; branch `codex/viewer-cursor-016`.
- User clarified the crosshair merely looks non-interactive, not an observed control failure. The CSS crosshair is replaced with a normal arrow; paused control has a not-allowed pointer and an explanatory tooltip.
- Scope: pointer CSS, viewer tooltip/state style, release version and docs.
- Runtime: current checkout caches, owned foreground test only, loopback Vite/QUIC, disposable Windows CI. Primary WIP and production profile unchanged.

## Evidence
- `npm run build` and `git diff --check` passed. An ignored temporary copy of the existing viewer regression script checked actual computed cursor styles: `default` for normal control, `not-allowed` for paused control. Existing mouse/keyboard, toolbar/profile, pause/retry, read-only and terminal behavior passed. No permanent test was added for this cosmetic change.
- Source `bdbfd17f9565a770b136a48e2feacc779056dd99`, [Windows client 36675505835](https://github.com/wanghao9103/farsail/actions/runs/36675505835) and [installer 36675506639](https://github.com/wanghao9103/farsail/actions/runs/36675506639): success. Installer includes actual install/settings IPC/restart/reinstall/uninstall retention and Rust regressions.
- [0.1.6 release](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.6-bdbfd17) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.6-bdbfd17/FarSail_0.1.6_x64-setup.exe).
- All four public attachments downloaded anonymously and matched CI originals by hash/size. Installer: 7,869,856 bytes, SHA256 `a55eb905d795e3332e99d756e28250976ead476d1fc3d3e9738c0a7413197160`. Metadata and installation report bind the same source/hash. Installer remains unsigned.
- Only pointer appearance and tooltip changed; no claim of fixing an unreported functional input failure. Test Vite/browser runtime stopped, no production app/profile/server touched. This trivial appearance adjustment does not create a new knowledge note.
