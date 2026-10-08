**English** | [简体中文](WI-INPUT-014.zh-CN.md)

# Windows preview 0.1.4 — input rejection must not end screen viewing

## Baseline and scope

- Base `cc95071` (published 0.1.3), clean managed desktop-ui-release worktree; branch `codex/input-recovery-014`.
- User reports moving/clicking in an ordinary desktop/application immediately disconnects. Screenshot identifies `input_failed`; it does not expose the original Windows error or prove UIPI.
- Confirmed code path: Move/Button/Wheel first use SetCursorPos, convert any failure to InputDenied, then close the entire session. Normal keyboard keys such as Caps Lock/Windows are also excluded and produce the same misleading terminal cause.
- Write set: Windows native input, desktop host/viewer input-status protocol and UI, targeted browser/native tests, version/release docs and relevant CI. Preserve auth/lease/stop and protected-desktop boundaries. Primary checkout file-transfer WIP is untouched.
- Runtime: this worktree's target/node_modules/.local, owned foreground test window only, loopback Vite, disposable Windows installer CI. No private desktop capture or input into unrelated apps.

## Implementation and verification

Mouse uses SendInput absolute virtual-desktop pixel-centre coordinates instead of SetCursorPos. Normal system/navigation keys are supported; unsupported input and stale geometry are ignored rather than being reported as a permission failure. SendInput captures inserted/expected counts and the Windows error code, with no claim that code zero identifies UIPI.

Input-state messages are bounded, sequenced and sent over the existing authenticated Control channel. OS rejection pauses host input and attempts to release held keys, while the media task continues. Viewer input queues are invalidated on pause. Explicit `resume_control` uses a distinct ordered recovery marker; ordinary blur/release cannot implicitly resume control. Fresh input remains gated by the original control grant, local sharing state, current session and cancellation checks under the input lock. Revocation, media/network terminal state and protected-desktop capture failure retain their existing stop behavior. Both PCs need this client update; server changes are unnecessary.

### Local checks

- `cargo test --locked -p farsail-windows --lib`: 8 passed, 2 interactive tests deliberately ignored. New coverage includes negative virtual origins/pixel-centre round trips, common system keys and unsupported input rejected before injection.
- `cargo test --locked -p farsail-windows real_input_into_own_foreground_window -- --ignored --test-threads=1`: passed on Windows. Test confirms foreground HWND/PID belongs to its own process before injecting harmless text, combination keys, movement/clicks. A GUI-less worker moves the cursor within that test window and verifies actual pixel position; no unrelated app receives input.
- `cargo test --locked -p farsail-desktop --lib`: 5 passed. Includes actual authenticated loopback QUIC carrying pause diagnostics, media after pause and explicit resume, plus stale/oversized/unknown-field notice rejection. Synthetic grant authority is test-only; this is not a physical two-PC Windows failure reproduction.
- Clippy for desktop/windows all targets with `-D warnings`, Rust formatting, frontend build and both browser regressions passed. Viewer test confirms retained image, suppressed paused input, explicit resume, terminal frame/queue cleanup and unchanged read-only/reconnect rules. Screenshot `input-paused.png` contains only synthetic pixels.
- Screenshot from the user lacks native API error details, so the exact Windows failure on their machine remains unproven. This change repairs the identified mouse path and prevents an input rejection alone from closing video; field verification on both updated PCs is still needed.

### Release

- Source `40edacb2e17b1ac1d2dc04baa3457135fcb831e2` pushed to main.
- [Windows client 36660400761](https://github.com/wanghao9103/farsail/actions/runs/36660400761), [installer 36660402240](https://github.com/wanghao9103/farsail/actions/runs/36660402240), [transport 36660400809](https://github.com/wanghao9103/farsail/actions/runs/36660400809), [backend 36660400832](https://github.com/wanghao9103/farsail/actions/runs/36660400832): all success at that exact commit.
- Installer CI includes locked native release build, real install/settings IPC/restart/reinstall/uninstall retention, Rust tests/Clippy and absence of debug probes. Client CI includes native window isolation and both browser suites.
- [0.1.4 release](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.4-40edacb) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.4-40edacb/FarSail_0.1.4_x64-setup.exe).

| Public attachment              |     Bytes | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.4_x64-setup.exe    | 7,849,691 | `7cbec092f01f7ee6f05d15fea1fcf4c163f4c5b0e41aeebdfb9b592e8186322b` |
| SHA256SUMS.txt                 |        95 | `be02c790b0b02eecf076101bc691f60838b463e98abd798f853272db77f18608` |
| release-metadata.json          |       651 | `7defe47b8379b3cb04c974366433e4431ba35536ce41f3a125d6abc55c6a6bf9` |
| installation-verification.json |       422 | `d19f4bb8ad4265f29492e3476389cf5032d6c708f6c41d617b4e85d4e5020a9c` |

All four public files were downloaded anonymously in full and matched the tested CI originals by byte count and SHA-256. The first public installer download suffered TLS/timeout interruptions; bounded HTTP/1.1 Range resume completed the existing partial file, followed by full hash validation. TLS verification remained enabled. Installer is unsigned; no production installation/profile or public server was changed.

Runtime closed: owned input test window destroyed, browser processes and Vite stopped, loopback transport test endpoints closed. Ignored build/download evidence remains. Future work should reuse this current managed checkout rather than the stale primary checkout with paused file-transfer WIP.

References: [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput), [MOUSEINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput), [SetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcursorpos).
