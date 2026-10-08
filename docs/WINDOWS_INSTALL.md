**English** | [简体中文](WINDOWS_INSTALL.zh-CN.md)

<a id="windows-x64-预览安装与双机联调"></a>

# Windows x64 preview installation and two-computer integration

FarSail 0.1.11 is a JPEG viewing/mouse-and-keyboard control preview supporting automatic fitting and 720p/1080p/2K/4K presets. The remote window starts maximized, uses the same dark title bar as the client and hides it in fullscreen. The toolbar hides automatically, can be opened by moving to the top or clicking its reveal control, and can be pinned. By default, the complete screen fills the available area and stretches if aspect ratios differ. “More actions → Screen display” offers aspect-ratio preservation (intentional borders); the choice is retained and mouse coordinates stay synchronized. Encoding does not upscale the source monitor or change the remote system resolution. An ordinary interactive desktop on Windows 10/11 x64 is required; Rust, Node and development tools are not needed. File transfer, HEVC, adaptive video bitrate and mobile clients have not been delivered. The secure desktop, UAC and logged-out desktop are unsupported.

Discovery is actively refreshed before new connections, relayed sessions periodically retry direct connections, and address changes are reported promptly; refresh preserves the existing session and screen. The window shows direct-connection attempts, periodic retries or forced relay. Real loopback QAD failure → recovery and continuous authenticated frame transmission passed. Migrating the same session from relay → direct on two physical computers after network repair still requires on-site acceptance testing; direct connections cannot be guaranteed for every NAT.

<a id="下载与安装"></a>

## Download and installation

Current pinned prerelease: [download installer](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.11-70d9393/FarSail_0.1.11_x64-setup.exe), [SHA256SUMS.txt](https://github.com/wanghao9103/farsail/releases/download/windows-preview-0.1.11-70d9393/SHA256SUMS.txt); other metadata is on the [release page](https://github.com/wanghao9103/farsail/releases/tag/windows-preview-0.1.11-70d9393). For the source SHA, installer SHA256 and installation-verification evidence, see [INPUT-021](verification/WI-INPUT-021.md). Download `FarSail_0.1.11_x64-setup.exe`, `SHA256SUMS.txt` and `release-metadata.json` together. In the download directory, run PowerShell:

```powershell
$expected = ((Get-Content ./SHA256SUMS.txt -Raw).Trim() -split '\s+')[0]
$actual = (Get-FileHash ./FarSail_0.1.11_x64-setup.exe -Algorithm SHA256).Hash
if ($actual -ine $expected) { throw 'SHA256 mismatch: do not install' }
```

Double-click to install and choose Simplified Chinese or English. The installer **has no Authenticode signature**. Windows may display an unknown publisher or SmartScreen warning; a digest proves download consistency, not a trusted signature. Do not disable system protection. If organizational policy blocks unsigned programs, follow the organization's procedure.

Installation is per-user, defaults to the user's local application directory, installs no system service and does not enable sharing at startup. The package includes the Microsoft WebView2 Evergreen bootstrapper. If WebView2 is absent, an internet connection is needed to download and install the runtime before launching. A compatible existing runtime is reused. This package is not a full offline WebView2 installer. For the official mechanism, see [Tauri's Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/).

<a id="两台-windows-首次连接"></a>

## First connection between two Windows computers

1. Install the same preview on both computers. First launch is logged out with sharing disabled. In “Settings”, enter your own valid HTTPS API address, such as `https://<公网IPv4>`, and click “Save address”. Do not put the relay's port 8443 in the API field; there is no option to skip certificate validation.
2. First start the server successfully using the [deployment instructions](DEPLOYMENT.md). If using Mailpit for initial integration, keep this forwarding terminal open on a computer with SSH access, then open `http://127.0.0.1:18025` in a browser:

   ```sh
   ssh -N -L 18025:127.0.0.1:8025 <SSH用户>@<公网IPv4>
   ```

   Register in the client, read the verification token from the corresponding test email, submit it in “Verify email” and log in normally. Test email is not delivered to a real mailbox; do not expose the inbox port publicly. If production TLS SMTP is configured, retrieve email from your own mailbox. The second computer can log in with the same verified account without registering again.

3. After logging in on both computers, add each local computer. The Windows computer name is used by default and can be changed. Each newer client updates its own old “This Windows computer” name while retaining manual names. Connection services are prepared automatically when enabling sharing or connecting. If an administrator requires network-parameter changes, expand advanced connection settings. Keep the local UDP listener at `0.0.0.0:0`, set relay to `https://<公网IPv4>:8443/`, and leave “Always connect through relay” unchecked by default. P2P is attempted first, falling back to relay when direct connection is unavailable; enable forced relay only to investigate direct-connection problems. Transport configuration applies to the current run.
4. On the host computer, click “Enable local sharing” and wait for the shared state. On the controlling computer, refresh “My devices” and send a viewing or control request to the online target. By default, the target checks the requesting account, device and permission and approves each request; administrator status cannot substitute for approval. To allow direct connection under the same account, explicitly enable “Remote standby” in the host's local-computer panel under “My devices”. Since 0.1.10, these two switches remember choices and are initially off. After an ordinary application restart, they are restored only after all login, device, transport and ordinary Windows desktop checks pass. During checks or on failure, the interface does not show sharing as active; restoration can be cancelled or retried. Manually stopping sharing remembers both switches as off; disabling standby remembers it as off and ends inbound connections. Logging out clears enabled preferences. Advanced connection parameters still apply only to the current run, and automatic restoration uses default transport parameters. Environments requiring a self-hosted relay must still re-enter and apply connection settings during this run.
5. After approval, the device-panel request automatically opens a separate remote window. The “Remote connection” page also offers “Connect and view” or “Connect and control” according to approved permission. Check whether the window's actual path is P2P or relay and compare both sides' verification codes in the collapsed connection details; merely entering a relay URL does not prove relay use. The window shows actual received FPS, network RTT and monitor selection. Clicking the remote screen gives it keyboard focus; maximize and fullscreen are available. Closing the viewing window ends that session while retaining the main device-management window. View first, then test mouse and keyboard in an approved control session against a non-sensitive window you prepared yourself.
6. Stop the session at either end. Disable local sharing on the host and confirm that screen and control stop. Closing the window or logging out also stops local capture and input first. Do not attach actual desktop screenshots, verification tokens or account passwords to public issues.

Quality defaults to “Automatic fit”, selecting a preset based on window physical pixels and Windows scaling; manual switching is also available. 2K/4K require upgrades on both ends and corresponding pixels on the source monitor. “More actions” shows the actual received dimensions. Frame rate is calculated from receive intervals over about four seconds; static screens send periodic keepalives, and the interface explicitly indicates when there are no updates. The capture loop's approximate 15 FPS cap excludes encoding and network overhead and does not guarantee that rate in practice. If neither end shows a screen, first check account/device online status, target sharing and approval, then valid certificates and relay configuration at both ends. Network RTT does not equal screen latency; the JPEG baseline does not represent future video performance.

<a id="升级卸载与凭据"></a>

## Upgrades, uninstallation and credentials

Upgrading both ends to 0.1.9 is recommended so both can actively refresh discovery within bounds; subsequent network recovery no longer requires restarting the client for rediscovery. A viewer at least at 0.1.8 can use fill/aspect-ratio-preserving display. 2K/4K presets and static-screen keepalives still require both ends to be at least 0.1.7; older endpoints offer only 720p/1080p. The deployed 4252 server is compatible with this client, without redeployment, initialization or key reset. Close FarSail before installing a new version. The application identifier remains `app.farsail.desktop`; native configuration and DPAPI-encrypted credentials are stored in `%LOCALAPPDATA%\app.farsail.desktop` for the current Windows user. Reinstallation and default uninstallation retain that directory. If the uninstaller shows a delete-application-data option, leave it unchecked to retain credentials. To revoke login, log out in the application first; uninstalling alone does not revoke the server session. Do not copy this directory to others or upload it.

<a id="开发者复现"></a>

## Developer reproduction

On a clean pinned commit, use Windows x64 MSVC Rust 1.93 and Node 22+:

```powershell
pwsh -File scripts/package-windows.ps1
```

The script installs from lockfiles, type-checks, builds the production frontend, packages Tauri release/NSIS, and checks AMD64/GUI PE, absence of debug probes, the v2 icon and digests. Output is in the ignored `.local/windows-package/artifacts`. Installation testing with `scripts/test-windows-package.ps1` is restricted to a disposable GitHub-hosted Windows runner to avoid overwriting local user configuration/registry. The workflow runs manually at a pinned ref and publishes that run's attachments only after validation passes; it does not substitute another local build with a different digest.
