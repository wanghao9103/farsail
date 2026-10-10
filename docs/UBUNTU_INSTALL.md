**English** | [简体中文](UBUNTU_INSTALL.zh-CN.md)

# Ubuntu desktop client

The Ubuntu client provides the account, device and connection interface and reuses the viewer protocol to connect to Windows hosts. Local Ubuntu screen sharing and remote standby are unavailable; Settings and the local device panel explain this limit. The current source candidate can send and receive individual files through a separately approved [file connection](FILES.md); older published installers do not include this addition. Windows capture and input are unchanged.

## Run from source

Use a logged-in Ubuntu desktop with an unlocked login keyring, Rust 1.93.0 and Node/npm. Ubuntu CI uses the 24.04 baseline on separate native x64 and ARM64 runners. ARM64 uses `aarch64-unknown-linux-gnu` and Debian architecture `arm64`; earlier local native checks used x64 Ubuntu 26.04.1. Older Ubuntu releases, 32-bit ARM and other distributions are outside the verified scope. Tauri uses WebKitGTK 4.1 on Linux; see the [official prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
sudo apt update
sudo apt install build-essential curl pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev librsvg2-dev patchelf gnome-keyring fonts-noto-cjk pkexec
npm ci
npm run tauri -w @farsail/desktop -- dev
```

Configure the coordinator address in Settings, sign in and add this computer. Devices bind with platform `linux` and the system hostname as the initial display name. Select an online Windows device with sharing enabled, request viewing or control, then connect after approval. The Linux app can also use an existing self-hosted coordinator; Docker is not required on the controlling desktop.

The coordinator must also be upgraded to accept `linux`; startup applies the additive `20261009000000_linux_platform.sql` migration without changing historical migration checksums. Linux devices cannot publish screen-host capability. To receive files in the current source candidate, enable the independent receiving switch; it then publishes file capability. Upgrade the coordinator for Linux file capability and manual file approval independent of screen sharing.

Add this computer once on first use. Reopening the app or signing back into the same account restores the existing device connection and immediately updates its online lease, preserving its id and name; logout still revokes the previous login and device credential. Identity is scoped to the coordinator and account, and disabled or unbound devices are never restored automatically. Clearing the keyring/profile, or having already signed out with an older client that discarded its binding record, still requires adding the computer once. Being online on Ubuntu means the client is connected, not that local screen sharing is supported. Recovery failures report a reason; check the network and refresh to retry.

## Build a Debian package

```bash
npm run tauri -w @farsail/desktop -- build --bundles deb \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
sudo apt install ./target/release/bundle/deb/*.deb
```

Tauri automatically merges `apps/desktop/src-tauri/tauri.linux.conf.json`, selecting the Debian target and declaring runtime and Chinese font dependencies. The Windows configuration continues to select NSIS. Build on the oldest Ubuntu release you intend to support: a package built on a newer distribution may depend on its newer system libraries. The local package is a candidate, not a published release.

## ARM64 installation and architecture identification

Use `FarSail_0.1.21_arm64.deb` on an ARM64 Linux desktop and the same-version `amd64.deb` on x64. Check the operating-system architecture and the package before installation, rather than selecting solely by processor model: a 64-bit ARM processor can run a 32-bit operating system.

| Operating system | uname -m | Debian architecture | Rust target               |
| ---------------- | -------- | ------------------- | ------------------------- |
| Linux x64        | x86_64   | amd64               | x86_64-unknown-linux-gnu  |
| Linux ARM64      | aarch64  | arm64               | aarch64-unknown-linux-gnu |

After downloading the ARM64 installer, run from its directory:

```bash
uname -m
dpkg --print-architecture
dpkg-deb -f ./FarSail_0.1.21_arm64.deb Package Version Architecture
test "$(dpkg --print-architecture)" = arm64
test "$(dpkg-deb -f ./FarSail_0.1.21_arm64.deb Architecture)" = arm64
sudo apt install ./FarSail_0.1.21_arm64.deb
```

Native ARM64 desktops use the same source startup and build commands above; Tauri packages for the native Rust target. Merely adding a Rust target to an x64 machine does not establish ARM GTK/WebKitGTK compilation or execution. The release flow uses a separate ARM64 runner and checks the actual packaged ELF, dependencies, system keyring and native IPC. Automatic updates select `linux-aarch64-deb`; an older feed lacking this architecture reports no compatible update instead of downloading an amd64 package. See [client updates](CLIENT_UPDATES.md).

0.1.21 is the current source candidate and does not mean that a signed version has been published. Actions build artifacts and client Releases are separate delivery layers; install a package with verified source and checksum for the correct architecture. ARM64 has the same Linux client scope: Windows connections and independent file transfers, without local Linux screen sharing.

## Credential storage

Ubuntu uses the desktop Secret Service via GNOME Keyring, with a namespace derived from the application profile directory. Account sessions, device credentials, signing keys and settings stay in Rust and the keyring; tokens never enter React or browser storage. A missing or inaccessible keyring fails with guidance to unlock it; there is no plaintext fallback. Run in the desktop session rather than as root or a system service.

## Verification

```bash
sudo apt install xvfb dbus-x11
bash scripts/test-desktop-linux.sh
```

The script runs Rust tests, Clippy, the frontend and native build, then creates an isolated session bus, data directory and keyring. The actual WebKitGTK window checks Linux metadata, unavailable host controls, workspace bounds, settings IPC and binary media IPC. The Ubuntu workflow also runs the synthetic account/device and viewer regressions using Playwright WebKit and builds a Debian artifact. Local checks do not establish Ubuntu-to-Windows physical two-computer operation, a Wayland session, Ubuntu 24.04 installation or a public release.

For Ubuntu binding HTTP 422 or distinct login feedback, follow the [production upgrade steps](UPGRADE_UBUNTU_LOGIN.md), then install desktop 0.1.18.
