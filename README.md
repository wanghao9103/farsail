**English** | [简体中文](README.zh-CN.md)

<p align="center">
  <img src="assets/branding/farsail-v2/128x128.png" width="96" height="96" alt="FarSail icon candidate" />
</p>

<a id="farsail--遥舟"></a>

# FarSail

FarSail is an open-source remote desktop project with Windows and Ubuntu clients built using Rust, Tauri 2 and React, encrypted peer-to-peer connections, and self-hosted relay support. Public version 0.1.21 provides bidirectional single-file transfer and an Ubuntu ARM64 client; older installations need an upgrade for these additions. Mobile clients remain planned.

The account, device and authorization coordinator is written in Rust. **WI-001 (coordinator), WI-002 (Windows client), WI-003 (authenticated encrypted transport), and WI-004 (Windows remote viewing/input baseline) have been verified locally. WI-008B provides a verified [Windows x64 preview installer](docs/WINDOWS_INSTALL.md).** The iroh data channel supports local direct connections and a self-hosted TLS relay. Windows hosts can enable local sharing and transmit JPEG frames and mouse/keyboard input after per-request approval. Automatic-update packages use the release signing key; Windows publisher certificates are configured separately; public cross-NAT behavior, physical Windows file transfer, HEVC and mobile clients remain unverified or unimplemented. See [file transfer](docs/FILES.md) for usage and the precise verification boundary.

## Documented preview milestones

- 0.1.2 adds an independent native viewer, disconnect cleanup and host-side opt-in for same-account unattended access. See the [verification record](docs/verification/WI-REMOTE-012.md).
- 0.1.3 adds a fixed desktop workspace, clearer connection actions and the computer name. See the [delivery record](docs/verification/WI-UI-RELEASE-013.md).
- 0.1.4 fixes mouse input and keeps the screen connection available when Windows rejects input. See the [input fix](docs/verification/WI-INPUT-014.md).
- 0.1.5 adds a maximized viewer, an automatically hidden toolbar and 720p/1080p quality selection. See the [window and quality record](docs/verification/WI-VIEWER-015.md).
- 0.1.6 uses a normal arrow cursor for the remote image. See the [cursor update](docs/verification/WI-INPUT-016.md).
- 0.1.7 adds a consistent dark viewer title bar, automatic viewport adaptation, 2K/4K JPEG profiles and static-image keepalive. FPS reflects received frame updates. Upgrade both endpoints for the complete feature set. Use the installation guide above and [VIEWER-017](docs/verification/WI-VIEWER-017.md).
- 0.1.8 fills the available viewport with the complete remote image by default, removing letterboxing when aspect ratios differ. More Actions can preserve the original aspect ratio; the choice is retained and input coordinates use the same display mode. See [VIEWER-018](docs/verification/WI-VIEWER-018.md).
- 0.1.9 refreshes address discovery before connecting and performs bounded periodic direct-path retries for relay sessions. Refresh preserves the endpoint, approval and screen connection, and publishes changed addresses promptly. Both endpoints should be updated; the existing server remains compatible. See [P2P-019](docs/verification/WI-P2P-019.md) for downloads, real QAD failure-to-recovery, uninterrupted sessions and artifact evidence. Physical cross-NAT migration still needs two-computer testing; direct connectivity is not guaranteed for every network.
- 0.1.10 remembers local sharing and unattended-access choices. After a normal restart, login, device, connection and the interactive desktop are rechecked before restoration. Manual disable remains disabled on the next start; logout clears enabled preferences. Advanced connection settings remain effective for the current run only. See [SHARE-020](docs/verification/WI-SHARE-020.md).
- 0.1.11 fixes lost mouse-button releases under stale geometry, adds pointer capture and cleanup on blur or dragging outside the image, and includes P2P discovery refresh and saved sharing/unattended preferences. Continuous clicks and focus switches between owned ordinary windows passed. See [INPUT-021](docs/verification/WI-INPUT-021.md) for delivery evidence and downloads.

- 0.1.21 publicly provides [signed Windows x64, Ubuntu x64 and ARM64 installers](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.21), including automatic updates, device-login restoration, independent file transfer and Windows background-window activation improvements. Physical two-computer and special-game acceptance remains pending.

These are recorded milestones. Local candidate work and its verification limits are retained in the verification documents; a local installer build is not a public release.

<a id="计划能力"></a>

## Planned capabilities

- Account registration and login, device binding, and a list of remotely accessible devices for the same account.
- Windows viewing and mouse/keyboard control; Android and iPhone clients controlling Windows.
- Encrypted communication across networks, direct peer-to-peer (P2P) connections and relay fallback.
- Multiple displays, resolution switching, negotiable 4:4:4 quality and adaptive bandwidth.
- H.265/H.264 video, evaluation of AV1 support, and lossless compression before transfer for suitable data.
- File-transfer resume, selective compression and configurable combined file/media bandwidth controls beyond the current single-file baseline.
- Later mobile-host capabilities, verified separately against each platform's public APIs.

<a id="文档与资产"></a>

## Documentation and assets

English is the default documentation language. Use the language links at the top of each page to switch to Simplified Chinese. See [documentation languages](docs/LANGUAGES.md) for the file convention and checks.

- [Project design](PROJECT_DESIGN.md): scope, architecture, accounts and authorization, transport, implementation and verification.
- [Codec survey](CODEC_SURVEY.md): video/image formats and selection rationale.
- [Icon notes](assets/branding/README.md): candidate assets, generation prompts and initial visual-similarity screening.
- [Coordinator API](docs/API.md): implemented account, device, invitation, authorization state-machine and administrator endpoints.
- [Ubuntu desktop client](docs/UBUNTU_INSTALL.md): source startup, Debian packaging, desktop keyring and controller scope.
- [Windows client](docs/CLIENT.md): how to run it, credential boundaries and implemented screens.
- [Windows preview installation](docs/WINDOWS_INSTALL.md): x64 installation without development tools, checksums, WebView2 and two-computer testing.
- [Authenticated transport](docs/TRANSPORT.md): iroh handshake, short leases, data channels and path state.
- [Windows remote control](docs/REMOTE.md): DXGI/JPEG, viewer, input permissions and safe shutdown.
- [Desktop file transfer](docs/FILES.md): separate approval, native file selection, safe save, progress and limits.
- [Self-hosted relay](deploy/relay/README.md): HTTPS certificates and public-IP configuration examples.
- [Public-IP deployment bundle](docs/DEPLOYMENT.md): six-image offline artifacts, short-lived IP certificates, loopback Mailpit and backup/upgrade.
- [Coordinator online updates](docs/SERVER_UPDATES.md): persistent command, automatic server package selection, checksums, backups and rollback.
- [Implementation and verification](docs/IMPLEMENTATION.md): work-item progress and local evidence.

The icon was generated with the built-in image-generation tool; v2 is the recommended candidate. Initial similarity screening does not establish uniqueness or complete trademark clearance.

<a id="开发与提交"></a>

## Development and contributions

Local development requires Rust 1.93 and Docker. `scripts/test-coordinator.ps1` generates the ignored `.local/dev.env`, starts only the `farsail-dev` PostgreSQL and Mailpit services, then runs the Rust tests. Example coordinator start in PowerShell:

```powershell
./scripts/test-coordinator.ps1
$password = ((Get-Content .local/dev.env) -split '=',2)[1]
$env:FARSAIL_DATABASE_URL = "postgres://farsail:$password@127.0.0.1:55432/farsail"
$env:FARSAIL_MAIL_MODE = 'smtp-local'
$env:FARSAIL_SMTP_PORT = '51025'
cargo run -p farsail-coordinator
```

The service listens only on `127.0.0.1:8787`; Mailpit UI is at `http://127.0.0.1:58025`. `GET http://127.0.0.1:8787/healthz` checks PostgreSQL. `./scripts/test-coordinator.ps1 -Stop` stops only this project's containers and retains the local data volume. Production requires a TLS reverse proxy, valid certificates, TLS SMTP, backups and rate limits. Do not disable certificate verification. Completion status is governed by the verification records [WI-001](docs/verification/WI-001.md), [WI-002](docs/verification/WI-002.md), [WI-003](docs/verification/WI-003.md) and [WI-004](docs/verification/WI-004.md).

Run the Windows client locally in separate terminals:

```powershell
./scripts/start-local.ps1
npm ci
npm run tauri -w @farsail/desktop -- dev
```

`./scripts/test-remote.ps1 -RealCapture` runs frontend, real PostgreSQL/HTTP/iroh/DXGI integration checks and the Tauri build. The normal two-endpoint flow is: bind devices → start transport → enable sharing on the host → request authorization → explicitly approve on the host → connect and view. The separate file workflow is documented in [file transfer](docs/FILES.md); it requires 0.1.21 on both desktops and a coordinator supporting the file protocol; older installers do not provide it. Public networking, cross-NAT, a second Windows machine and mobile clients are not proven by this work item's evidence.

## License

[MIT](LICENSE). Third-party dependencies remain subject to their respective licenses.
