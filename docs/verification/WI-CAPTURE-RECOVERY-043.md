**English** | [简体中文](WI-CAPTURE-RECOVERY-043.zh-CN.md)

# Windows capture interruption and session-scoped recovery

Date: 2026-11-05. Implemented after desktop 0.1.36 and released in desktop 0.1.37; acceptance is below.

## Observed incident

The user reported a working Windows remote session suddenly ending without locking or switching users. The local viewer trace still received media 45 ms before closure and recorded the bounded reason `session_ended`; the supplied host screenshot showed remembered sharing/watch choices still enabled, files available and screen sharing stopped. These observations establish loss of runtime screen capability, not the Windows driver error that triggered it. The screenshot’s local Ubuntu “remote access disabled” indicator describes that controller’s incoming sharing and does not itself disable outgoing Windows control.

## Mechanism and change

Previously any capture-worker error ended production and eventually disabled the device’s global screen capability. The dropped producer channel could make the media sender close the transport generically before `capture_failed` was published. This explains a verified code path from one session’s capture failure to disabled reconnect buttons, while the actual incident trigger remains unknown.

DXGI access loss, device removal and device reset are now typed separately from access denial and other permanent errors. Only these reset conditions receive up to three capture recreations, delayed 150/300/450 ms. A successful constructor or a single frame does not replenish the budget; actual healthy frames spanning five seconds do. Reset discards cached media and coordinates, releases held input and checks the live session and sharing permission before continuing. Rebuilt capture must match the previously selected display’s name and desktop rectangle; layout changes are not silently redirected to another monitor. The coordinate layout becomes usable only with a fresh frame. Keyboard and pointer input wait while there is no current capture layout. Input cleanup failure ends with `input_failed`, without automatic capture retry.

Persistent failure publishes `capture_failed` before dropping the media channel, ends only the affected session, and preserves the user’s global sharing/watch choice and file capability. It does not auto-create an endless series of fresh sessions. Initial absence of an interactive display still fails closed. Normal opt-out, authorization expiry and input failure retain their existing checks. A newly approved connection can retry capture; no grant, lock-screen or Windows desktop security check is bypassed.

Bounded reset and closed-session diagnostics now retain the capture error. The device hint distinguishes stopped/interrupted sharing from an assumption that the user never permitted it. Microsoft documents that DXGI duplication can become invalid on mode/desktop/fullscreen changes and must be recreated: [AcquireNextFrame](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-acquirenextframe).

## Verification and limits

Evidence: `.local/viewer-compact-20261105/`.

- Desktop Rust: 55 passed, one unrelated interactive test ignored. Added bounded-budget and actual-frame replenishment tests plus authenticated QUIC/media-producer cleanup proving the peer still receives `capture_failed` and does not automatically retry terminal capture failure.
- Viewer policy tests: 17 passed. TypeScript/Vite, Windows capture cross-target check, desktop all-target Clippy and formatting passed.
- Native Linux smoke: 23 unauthorized commands denied; retained geometry, fullscreen and new connection action restoring the existing main window passed (`native-final/report.json`). Failed native setup/old-selector attempts are retained.
- In-app browser exercised reference-based viewer changes, 1280/440/natural 571px layouts, tabs, device list, close, fullscreen, tool reveal/collapse and active-session Alt+Tab. See [visual comparison](../../design-qa.md).

No Windows GPU reset was injected or reproduced on the physical host, and its host log was not obtained. These tests do not establish the incident’s exact cause, real multi-monitor driver recovery, games, WAN or overnight acceptance. The initial implementation did not change the public package/channel or installed personal-device apps; publication was accepted subsequently below. The current installed version can use “Retry remote connection” on the host to reinitialize sharing.

## Release acceptance

Desktop [0.1.37](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.37) is a regular release and GitHub Latest, from source `9e0ee37b8dafcbf9c7e621506ad7f32aa0be6922`. Its [release workflow](https://github.com/wanghao9103/farsail/actions/runs/41980249187) succeeded. Windows x64 and both native Ubuntu architectures passed their package checks, including installation/restart/reinstall/uninstall on the Windows runner, native IPC, retained window geometry and unauthorized-command denial.

All 12 public assets were independently downloaded and matched API SHA-256 digests. Three platform signatures, source metadata, 11 checksum entries, native tray evidence and byte-for-byte publication reconstruction passed; the ordinary fixed update URL returns the same 0.1.37 manifest. The first attempt failed only its final channel comparison. A subsequent ordinary-URL check matched; only failed publication was retried, with all 12 version assets retaining their IDs, digests, sizes and timestamps. The initial failed job response body was not captured, so its exact mismatch content is unknown. A slow redundant Windows download was replaced with an independently verified anonymous HTTP/1.1 transfer of the same public URL; the original interrupted-download log remains.

Evidence: `.local/client-public-0.1.37/delivery-verification.json`, `verification.json`, `download-provenance.json`, `windows-http1-provenance.json`, `package-attempt1-status.json` and `immutable-assets-final.json`. The Windows installer SHA-256 is `5b0e0adad38246653b0fd519dde361ff35494b322c8f2bde1e7b3d890641ae34`. Update both controller and Windows host to receive both interface and capture fixes. No Android package or coordinator was released. This does not accept the original physical GPU fault or overnight operation.
