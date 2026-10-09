**English** | [简体中文](UBUNTU_INSTALL.zh-CN.md)

# Ubuntu desktop client

The Ubuntu client provides the account, device and connection interface and reuses the viewer protocol to connect to Windows hosts. Local Ubuntu screen sharing and remote standby are unavailable; Settings and the local device panel explain this limit. Files remain unavailable. Windows capture and input are unchanged.

## Run from source

Use a logged-in Ubuntu desktop with an unlocked login keyring, Rust 1.93.0 and Node/npm. The Ubuntu CI build target is 24.04; local native checks were performed on 26.04.1. Older Ubuntu releases and ARM builds have not been verified. Tauri uses WebKitGTK 4.1 on Linux; see the [official prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
sudo apt update
sudo apt install build-essential curl pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev librsvg2-dev patchelf gnome-keyring fonts-noto-cjk pkexec
npm ci
npm run tauri -w @farsail/desktop -- dev
```

Configure the coordinator address in Settings, sign in and add this computer. Devices bind with platform `linux` and the system hostname as the initial display name. Select an online Windows device with sharing enabled, request viewing or control, then connect after approval. The Linux app can also use an existing self-hosted coordinator; Docker is not required on the controlling desktop.

The coordinator must also be upgraded to accept `linux`; startup applies the additive `20261009000000_linux_platform.sql` migration without changing historical migration checksums. Linux devices cannot publish host or file capabilities.

## Build a Debian package

```bash
npm run tauri -w @farsail/desktop -- build --bundles deb \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
sudo apt install ./target/release/bundle/deb/*.deb
```

Tauri automatically merges `apps/desktop/src-tauri/tauri.linux.conf.json`, selecting the Debian target and declaring runtime and Chinese font dependencies. The Windows configuration continues to select NSIS. Build on the oldest Ubuntu release you intend to support: a package built on a newer distribution may depend on its newer system libraries. The local package is a candidate, not a published release.

## Credential storage

Ubuntu uses the desktop Secret Service via GNOME Keyring, with a namespace derived from the application profile directory. Account sessions, device credentials, signing keys and settings stay in Rust and the keyring; tokens never enter React or browser storage. A missing or inaccessible keyring fails with guidance to unlock it; there is no plaintext fallback. Run in the desktop session rather than as root or a system service.

## Verification

```bash
sudo apt install xvfb dbus-x11
bash scripts/test-desktop-linux.sh
```

The script runs Rust tests, Clippy, the frontend and native build, then creates an isolated session bus, data directory and keyring. The actual WebKitGTK window checks Linux metadata, unavailable host controls, workspace bounds, settings IPC and binary media IPC. The Ubuntu workflow also runs the synthetic account/device and viewer regressions using Playwright WebKit and builds a Debian artifact. Local checks do not establish Ubuntu-to-Windows physical two-computer operation, a Wayland session, Ubuntu 24.04 installation or a public release.

For Ubuntu binding HTTP 422 or distinct login feedback, follow the [production upgrade steps](UPGRADE_UBUNTU_LOGIN.md), then install desktop 0.1.18.
