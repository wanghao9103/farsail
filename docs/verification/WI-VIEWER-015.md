# Windows preview 0.1.5 — give the remote desktop the window space

- Baseline `1e49528`, clean managed desktop-ui-release checkout; branch `codex/viewer-space-015`.
- User reports the remote window is too small. Existing native viewer opens at 1200×800; multiple permanent toolbar/form rows reduce the image area further.
- Default: open the independent viewer maximized. Use a compact toolbar for display/fullscreen/end; connection diagnostics and text tools expand on demand. Keep full image aspect ratio and correct letterbox pointer mapping.
- User additionally requests resolution switching and hideable tools. Default HD is bounded 1080p quality 85 with 4:4:4 chroma; alternative is 720p. JPEG byte/pixel and payload-rate limits remain. The stream profile changes encoding only, not the remote OS display mode.
- Write set: native viewer builder/smoke assertions, frontend viewer/CSS, capture/JPEG profile handling and scoped media-profile IPC, viewer/browser/native tests, release version and documentation. Runtime stays in this checkout and isolated test profiles; primary file-transfer WIP remains separate.
- Required checks: frontend build, viewport/frame-area and coordinate regression, input pause/retry regression, actual native default-maximize/window isolation, Windows release installation and public artifact hashes.

## Evidence
- `npm run build`, Rust formatting and Clippy `-D warnings` passed. Media 4, Windows 9, Desktop 6 ordinary tests passed; 2 interactive Windows checks remain deliberately separate.
- Actual native WebView smoke passed: default-maximized, restore/maximize/fullscreen/minimize/close with main preserved. Nine forbidden IPC operations include another session's media-profile command.
- Actual DXGI→JPEG encode/decode passed in memory, no screenshot/private pixels saved. Profile geometry checks confirm native 1920×1080 preserved in HD, 720p downscale, portrait handling, no upscaling and invalid-profile rejection.
- Colored 1px edge test confirms HD chroma/quality materially reduces decoded color error; oversized JPEG is still rejected. Capture retries lower quality within the same 1 MB frame limit for complex images instead of treating byte-limit overflow as an immediate capture failure.
- Both browser suites passed. Viewer checks: 640×420, 1200×800 and 1920×1080 viewport bounds, image uses full client area, automatic toolbar hide/reveal/pin, on-demand text tools, profile IPC/confirmation, quarter/three-quarter letterbox coordinates and previous pause/retry/terminal/read-only behavior.
- Profile request/ack uses bounded 13-byte Media packets and monotonically increasing generations. Receiver accepts only known presets and expected acknowledgments. Capture restarts for a changed preset to produce a new static frame and layout generation; full-screen input still maps through physical display geometry.
- Pending Windows CI, installer publication and anonymous hash verification. Both PCs need the new client for profile switching. Existing capture frame/payload limits remain; HD prioritizes clarity, not a promised high frame rate.
