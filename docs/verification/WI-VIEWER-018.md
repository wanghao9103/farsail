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
- Release installation/public attachment verification pending.
