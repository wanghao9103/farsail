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
