# Windows preview 0.1.7 — unified chrome, adaptive quality and 4K

- Base `494bb03` (0.1.6), current managed checkout; branch `codex/viewer-4k-017`.
- User requests matching titlebar, automatic adaptation to the local view, 2K/4K quality, and correct frame-rate feedback. Screenshot shows 3 FPS; zero can also result from unchanged desktop pixels in the previous one-second counter.
- Write set: viewer/window IPC/frontend/CSS, capture/media/transport size/deadline bounds, profile negotiation, targeted native/browser/codec tests, version and docs. No remote system display-mode change, primary file-transfer WIP or production profile/server change.
- Runtime: local owned test window/capture in memory, loopback tests and Vite, isolated native profile, disposable Windows release CI.
- Requirements: dark custom titlebar scoped to this viewer, automatic quality via real viewport/DPR and source limits, 720p/1080p/1440p/2160p bounded encoding, static heartbeat plus honest recent-frame stats, aspect-preserving input coordinates, existing input pause/revoke safety. Verify and publish exact CI artifact, anonymously compare hashes.

## Evidence
- Frontend production build and both browser suites passed. Synthetic viewer checks cover automatic profile selection at 1200/2560/3840 physical pixels, manual 2K/4K, an old peer's disabled 4K option, titlebar/fullscreen layout, static heartbeat FPS vs stalled updates, full remaining area, letterbox input coordinates and input pause/retry/terminal cleanup.
- Rust media 5, Windows 10, Desktop 6 and transport 10 ordinary tests passed; formatting and Clippy `-D warnings` passed. Native 2K/4K JPEG roundtrips preserve actual dimensions; noisy 4K falls back within 4 MB and legacy HD remains within 1 MB. Authenticated Media carries exactly 4,000,049 bytes; max+1 send and max+1 receive headers are rejected. Input/control/file/handshake limits stay unchanged.
- Explicit interactive checks passed: owned foreground-window mouse/keyboard input and actual DXGI→JPEG/decode in memory. No production profile or captured desktop pixels were written.
- Native WebView smoke passed: decorations disabled with the shared custom titlebar, default maximized, restore/maximize, real frontend fullscreen button hides/restores titlebar, minimize, viewer close preserves main. Nine forbidden IPC calls remained denied. Native testing exposed that entering fullscreen from maximized may not produce a WebView resize; fullscreen IPC now returns the requested state explicitly. Release checks pending.
- Profile capability is bounded `FSV1` plus max preset byte; absent capability retains 720p/1080p compatibility. Encoding requests/acks retain their monotonic generation. 4K is an encoding ceiling without source upscaling or remote OS resolution changes.
- Capture sleeps 66 ms instead of 180 ms (about 15 vs 5 FPS ceiling before capture/encoding/network). Static snapshots repeat about once per second; UI reports actual receipt intervals over four seconds and explicitly labels missing updates. Payload rate is capped at 6 MB/s; larger frames have bounded size-based Media timeout and high-profile ACK deadline. These are not measured physical two-PC performance guarantees.

## Published evidence
- Source `d7753f2806827dbf60f003b7095075bf7c7c2a33` pushed to main; tag points to the same source.
- [Installer 36680856936](https://github.com/wanghao9103/farsail/actions/runs/36680856936), [Windows client 36680825793](https://github.com/wanghao9103/farsail/actions/runs/36680825793), [transport 36680825771](https://github.com/wanghao9103/farsail/actions/runs/36680825771), [backend 36680825765](https://github.com/wanghao9103/farsail/actions/runs/36680825765): all success at the exact source.
- [Release 0.1.7](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.7-d7753f2) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.7-d7753f2/FarSail_0.1.7_x64-setup.exe).

| Public attachment | Bytes | SHA-256 |
| --- | ---: | --- |
| FarSail_0.1.7_x64-setup.exe | 7,873,131 | `1ee9c1deab63567a03d4a9647bee702e688712c2365f91d045cde35ce8ca5ca5` |
| SHA256SUMS.txt | 95 | `3c82b76514087c1d35204fe4520d167625af95f9bb31d54df820586c9bfeedaf` |
| release-metadata.json | 651 | `0a14167be23810c2bcee336325390dd245738329f1964878c3e4d0bbe18c3b60` |
| installation-verification.json | 422 | `9194737f1b2065e4a5c557550b6e06f25cbe4cf41137d06f326c1bdb1c03ce0d` |

All four files downloaded anonymously over verified HTTPS and matched CI originals by bytes and SHA-256. Installer CI passed actual install, embedded native settings IPC/restart, reinstall/uninstall retention and absence of debug probes. No production installation, profile, server or primary file-transfer WIP changed. Local test/Vite processes stopped. Installer unsigned; real two-PC/4K-display network performance remains unmeasured. Personal inbox knowledge updated with the bounded profile/transport/budget/fullscreen-state mechanism.
