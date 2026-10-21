**English** | [简体中文](CLIENT.zh-CN.md)

<a id="farsail-windows-客户端wi-002003004"></a>

# FarSail Windows client (WI-002/003/004)

For build automation, update preferences, signing and publication, see [Client automatic updates](CLIENT_UPDATES.md).

Ubuntu controller support, source commands, Debian packaging and the Secret Service boundary are documented in [Ubuntu desktop client](UBUNTU_INSTALL.md). Local Linux screen sharing remains unavailable.

Version 0.1.23 uses **Allow remote connections** for both incoming screen and file requests on Windows, and for files only on Ubuntu. It is initially off, saves the choice and restores it only after validating the same login/device and required native services. Turning it off stops incoming screen and file sessions; logout or an identity change clears enabled preferences. File grants, per-connection approval and per-file native saving remain separate; see [file transfer](FILES.md).

For the local sharing and remote standby preferences in 0.1.10, see [SHARE-020](verification/WI-SHARE-020.md). DPAPI saves only these two choices after they have been successfully enabled, together with their service/account/device/login-session scope; it does not save advanced connection parameters. Native startup attempts restoration only once, revalidating the login, heartbeat, transport and desktop before publishing sharing capability. The interface separately shows the remembered intent and effective activity. Manual disablement, logout or an identity change cancels delayed restoration; explicit application exit or a fault stops activity while retaining valid preferences. Both are off initially. Windows must still be logged in and the application running; there is no service, autostart or secure-desktop support.

From 0.1.23, closing the main window keeps enabled remote access and active file transfers running. With an available system tray, the window hides there; otherwise it minimizes to the taskbar. Choose “打开 FarSail” (Open FarSail) from the tray or launch FarSail again to reopen the existing window. Choose “退出 FarSail” (Quit FarSail) from the tray or “退出” (Quit) from the window menu to stop connections and quit. Pending requests and file-save choices still need local action in the main window. Closing a separate viewer still ends only that viewer's session. This is not system startup or a background service.

The Windows client is in `apps/desktop`. Its React/Vite pages call Tauri 2 commands, while `crates/client` sends account, device and authorization HTTP requests. For the service protocol, see [API.md](API.md); for transport, see [TRANSPORT.md](TRANSPORT.md); for screen and input details, see [REMOTE.md](REMOTE.md). The Windows remote-viewing and mouse/keyboard baseline is integrated. Independent [file transfer](FILES.md) for Ubuntu/Windows has been available since 0.1.21; resume/compression and physical Windows validation remain outside this baseline.

Windows client 0.1.23 requests administrator permission through the system UAC prompt at startup. Cancelling authorization cancels that launch. Ubuntu continues to run as the signed-in desktop user; it does not request root for controlling Windows or transferring files. Account/profile and secure-desktop limits are described in [Windows installation](WINDOWS_INSTALL.md).

For Windows x64 preview installation without a development environment, WebView2, the unsigned-build explanation and integration steps for a second computer, see [WINDOWS_INSTALL.md](WINDOWS_INSTALL.md). For pinned downloads and actual verification results, see [WI-008B](verification/WI-008B.md).

<a id="本地运行"></a>

## Running locally

Requires Windows, Rust 1.93, Node/npm and Docker. The server example binds only to loopback, and test email goes only to local Mailpit. The current Windows manifest also applies to debug builds: run the desktop development/native test commands from an administrator terminal. Command-line process creation does not automatically display the Explorer launch prompt.

```powershell
./scripts/start-local.ps1
# 另开终端
npm ci
npm run tauri -w @farsail/desktop -- dev
```

Alternatively, run `./scripts/test-remote.ps1 -RealCapture`: it starts the project-specific `farsail-dev` PostgreSQL/Mailpit services, runs backend, transport, client and Windows synthetic tests, then uses the local physical monitor to complete capture → JPEG → authenticated connection → decoding in memory. Finally, it runs Clippy, the frontend build and the Tauri native build. Actual system input is a separate ignored test; it injects harmless content only after confirming that the window created by the test program itself is in the foreground. No real email is sent. The default service address is `http://127.0.0.1:8787`; it can be changed in Settings after logging out. The Mailpit UI is at `http://127.0.0.1:58025`. Afterward, use `./scripts/test-coordinator.ps1 -Stop` to stop this project's containers; the development database volume is retained. Use Ctrl+C in the running `start-local.ps1` window to stop the coordinator.

<a id="使用流程"></a>

## Usage

1. Register an account, retrieve the local verification token from Mailpit and submit it on the email-verification page; a real deployment must configure TLS SMTP.
2. After logging in, confirm binding this computer in Overview. Rust generates an Ed25519 key, signs the service challenge and initially declares `can_host=false` and `can_files=false`. Heartbeats start after binding. Logging out or switching accounts clears the old login and device credentials and stops heartbeats for the old identity.
3. “My devices” reads all devices belonging to the same account page by page, including offline and administrator-disabled devices. It supports search, capability filters, renaming and unbinding. “Account security” supports password changes, logout and revocation of other login sessions.
4. After starting secure transport and enabling “Allow remote connections” in “Settings”, the client declares `can_host=true` only when the Windows DXGI probe succeeds. Another client under the same account can refresh its device list and select the online host. By default, the target must still explicitly approve each viewing or control request; the approval window shows the requester's email and device name. On the requester, click “Connect and view”. The viewer receives binary JPEG and displays monitor selection, actual received FPS, path RTT and a verification code that both sides can compare. `control` allows mouse and keyboard input; `view` only shows the screen. Both sides can stop immediately and locally. Client 0.1.28 includes file paste in an approved control session; view remains read-only. Older packages and standalone file connections retain their separate approval flow. See [file transfer](FILES.md).
5. An administrator account can use the same client to view users, login sessions and audit metadata; enable or disable accounts/devices; change registration policy; and create or revoke signup invitations. Administrators cannot approve remote control on behalf of the target device.

<a id="凭据边界"></a>

## Credential boundaries

- The serialized state lock in `crates/client` handles access-token refresh rotation, HTTP and device signatures. The WebView never obtains access/refresh tokens, device tokens, device private keys or grant tokens. The frontend only temporarily holds user-entered passwords, email-verification tokens and one-time invitation codes, and does not use `localStorage`.
- Login sessions, device credentials and device private keys scoped separately to each service address and account are stored in Windows-user-scoped DPAPI-encrypted files. Tauri's `app_local_data_dir` supplies the directory. Iroh uses that same device private key. Logout first cancels local sessions, then deletes login and device credentials; it retains the private key so that the same account can prove the device's identity again at its next login. Mobile clients require separate platform secure-storage adapters.
- Public service addresses must use certificate-validated HTTPS, including HTTPS IP addresses. HTTP is allowed only for explicit loopback addresses. Requests do not follow redirects, and each response is limited to 1 MiB. There is no option to skip certificate verification.
- The server determines login revocation and device-token invalidation. Logging out while offline still deletes local credentials and reports that remote revocation is unconfirmed. The next login must sign and bind the device again. Authorization grants stay in native memory and are delivered to the correct requester through a device-authenticated iroh handshake; pages never access this credential. The native layer handles at most 8 concurrent handshakes and 16 active sessions. Results from an old startup or connection after logout or a transport restart cannot be written back.
- An exclusive lock protects the configuration directory, so a second desktop process cannot concurrently read and rotate the same refresh token. Debug IPC tests can specify an isolated `FARSAIL_TEST_PROFILE_DIR`. Disabling remote connections, logging out or explicitly quitting the application first stops capture, input and incoming file operations; server updates/revocation follow on a best-effort basis. The lease remains the fallback boundary.

<a id="已知边界与-wi-005006-接口"></a>

## Known boundaries and WI-005/006 interfaces

The low-frame-rate JPEG path is the currently usable baseline. The system secure desktop, UAC, a logged-out Windows session, cross-NAT/public-network operation and a second Windows computer have not been verified. Video encoding, 4:4:4 and bandwidth adaptation belong to WI-006. A web preview can check layout only; it cannot represent Windows native capture, input or IPC.

The native interfaces `NativeClient::start_transport`, `connect_transport`, `transport_session` and `farsail_transport::Session::send/receive` remain available for reuse by the file work item. `Media`, `Control` and `File` have separate permission and size boundaries. The current file baseline enables `can_files=true` through the unified remote setting only while native file handling is available. Initial binding keeps `can_files=false`; startup restoration must revalidate the saved choice before publishing the capability. Chunking, SHA-256, a shared file bandwidth budget and safe non-overwriting destinations are implemented; disconnect resumption is not. See [file transfer](FILES.md). Local two-endpoint validation covers direct connections and forced TLS relay, but there is no evidence for a public TLS proxy, a second computer, cross-NAT traversal rates or physical mobile devices.
