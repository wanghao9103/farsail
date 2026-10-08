**English** | [简体中文](WI-UI-011.zh-CN.md)

# Windows preview 0.1.1 — device workspace and onboarding

## Scope

- Dark device workspace with list/detail panes, distinct local and remote actions.
- Local sharing and consent requests in the device detail panel; remote view/control can request approval and connect without the request form.
- Registration advances to email verification, then login. Login failures have a Chinese modal without claiming to know which credential failed.
- Compact custom title bar; native minimize/maximize/close permissions are explicitly scoped to the main window.
- Default relay suggestion uses the configured HTTPS server host on port 8443; custom deployments can edit it in settings.
- File transfer WIP is excluded. Existing JPEG capture and consent requirements remain; no unattended mode.

## Local verification

- `npm run build` passed (TypeScript and Vite).
- `scripts/test-desktop-ui.cjs` passed against a local Vite server using Playwright/Chrome and synthetic IPC only: register/verify/login, invalid verification stays on form, 401 modal, local-only sharing controls, offline guard, cancel request, view approval triggers exactly one connection, narrow viewport without horizontal overflow.
- Screenshots reviewed in `.local/ui-verification/`: local device, remote device, narrow layout. Synthetic accounts only.
- Browser fixtures do not establish real peer connectivity or native title-bar behavior. Release CI must pass actual Windows installation and UI Automation checks before publishing.

## Release gate

Use `.github/workflows/windows-release.yml`; `scripts/test-windows-package.ps1` now checks custom maximize/minimize and close, native settings persistence, reinstall and uninstall retention. Publish exactly the artifact whose hash is in the passing installation report. Source and public asset evidence to be recorded after completion.

## Published evidence

- Source: `9ad61751506d44837e3e5d8df271914ef9ce7222`.
- Windows installer CI `36511486164`: success, including native custom maximize/minimize/close, install, settings IPC/restart, reinstall/uninstall retention, Rust tests and Clippy.
- Windows client CI `36511468941`: success.
- Release: https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.1-9ad6175
- Installer: `FarSail_0.1.1_x64-setup.exe`, 7,744,845 bytes, SHA-256 `a79e7dbde64d3f4aec0dfad7861f1562413d26a0448f5df1512d0584f73ab3da`.
- All four public attachments were downloaded anonymously and matched the CI artifacts byte-for-byte (SHA-256). Package remains unsigned. This release does not claim new real-peer capture/input verification.
- The existing file-transfer WIP remains unstaged in the shared workspace and is not included in this release. Existing production server images need no update for this client release.
