**English** | [简体中文](WI-VIEWER-018.zh-CN.md)

# Windows preview 0.1.8 — full viewer area without letterboxing by default

- Base `7213ecf60b5ccaa2c564e18cb16490d354898b06`, clean managed desktop-ui-release worktree, branch `codex/viewer-fill-018`.
- User reports side bars when maximized and top/bottom bars when restored. Source and client aspect ratios differ; `object-fit: contain` produces these bars even though the image element fills the client area.
- Write set: viewer fit mode/pointer mapping/preferences/CSS, synthetic browser regression, installer version and delivery docs. No capture codec, remote OS resolution, server, primary file-transfer WIP or production profile changes.
- Default fill stretches the entire source into the viewer area without cropping; aspect-preserving fit remains available in More actions and is remembered locally. Pointer/button/wheel mapping must match each mode, and changing modes must cancel/release queued input.
- Checks: frontend build, screenshot pixel coverage in wide/tall/fullscreen/restored viewports, both coordinate paths and letterbox rejection, preference retention, existing quality/FPS/pause/terminal/read-only regressions, native window isolation, exact-source Windows CI install and public hashes.

## Evidence

- TypeScript/Vite production build, Rust formatting and actual native viewer smoke passed. Native default-maximize/restore/fullscreen/titlebar/minimize/close isolation and nine denied IPC operations remain intact; native Rust window logic unchanged.
- Both browser suites passed. Four distinct source-corner colors are decoded from actual page screenshots at each viewing-area corner: 640×420, 1920×1080, tall 900×1200, full-screen and restored 1200×800. This verifies pixel coverage and absence of cropping, beyond checking image-element dimensions or CSS declarations.
- Fill mode quarter/three-quarter click maps to source 0.25/0.75 through separate axis scaling. Aspect fit maps the rendered image's quarter/three-quarter independently, with only newly sent input accepted as evidence. A visible letterbox click with the toolbar hidden sends no remote button input. Both preferences survive reload; quality/FPS/input-pause/late-status/terminal/read-only checks remain passing.
- Presentation can stretch a source with a different aspect ratio; this is explained next to the setting, and the optional aspect-fit mode intentionally retains bars. No source resolution or source-pixel-detail claim changed. Local test images are synthetic.
- [Windows client 36687041306](https://github.com/wanghao9103/farsail/actions/runs/36687041306) and [installer 36687086372](https://github.com/wanghao9103/farsail/actions/runs/36687086372) both passed at source `0f43f106f31e6065e6c2c45569d27f64930ab695`. CI confirms native viewer/browser regressions and real installation, embedded frontend/native settings restart, reinstall/uninstall retention and debug-probe absence.

## Published evidence

[Release 0.1.8](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.8-0f43f10) / [Windows x64 installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.8-0f43f10/FarSail_0.1.8_x64-setup.exe). Tag points to the exact tested source; all four public files downloaded anonymously over verified HTTPS and matched the CI originals by bytes and SHA-256.

| Public attachment              |     Bytes | SHA-256                                                            |
| ------------------------------ | --------: | ------------------------------------------------------------------ |
| FarSail_0.1.8_x64-setup.exe    | 7,871,373 | `6d1608d7e68f044e72a3cc5dd0c2f814c0707d82aab544e411f2048e09f3b8c7` |
| SHA256SUMS.txt                 |        95 | `d8313fc1d078d9fdfdc97c117a3193dd7b228c406f9b2eaed6e7dbbecce87eba` |
| release-metadata.json          |       651 | `b0e49cd81da9b33142f4e2df6ebcce0cc100b891d79e96a69b33e2b799c939d8` |
| installation-verification.json |       422 | `129d5f38f18cd4d9fa6b5bea71c45447cbfd45a8f1e90ef220bd9d782d5d7a08` |

Installer unsigned. Test/Vite processes stopped. No production profile/installation, server or primary file-transfer WIP changed. Personal inbox knowledge updated with screenshot pixel-coverage and per-mode input geometry checks. Continue viewer work in this managed checkout; only the viewing client needs this presentation update.
