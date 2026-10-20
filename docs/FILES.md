**English** | [简体中文](FILES.zh-CN.md)

# Desktop file transfer

[Client 0.1.21](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.21) provides bidirectional single-file transfer on Ubuntu and Windows desktops, with x64 and ARM64 Ubuntu packages. This does not establish the same capability for older installers. Upgrade both desktops and the coordinator together: the coordinator must accept Linux file capability and keep file approval independent from screen sharing. Windows code, NTFS, native viewer and installation/restart/reinstallation/uninstallation CI checks passed. Native file dialogs have not yet been accepted on a physical Windows machine; the three signed 0.1.21 installers are publicly released.

[Client 0.1.28](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.28) is published from `11a75bbfe5a49fd2665c98b34bb3d7c817cd255c`. Its [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39365699777), public package signatures and byte-for-byte feed verification passed. Both clients must update to use remote file paste. Physical two-machine clipboard/Explorer acceptance remains pending.

## Copy and paste during remote control (0.1.28)

With this feature on both clients, one **Remote control** approval includes file transfer. No additional Files connection or per-file confirmation is required. Screen-capable devices integrate file actions into remote control; devices that only receive files retain the standalone entry.

1. Connect with remote control, open the destination folder remotely and keep that window in the foreground.
2. Copy regular files in the local file manager, focus the remote picture and press **Ctrl+V**, or use **Transfer files** in the viewer toolbar.
3. Up to 32 regular files transfer sequentially with progress. After every file passes integrity checks, the remote client publishes the file clipboard and sends paste. Success means the paste action was sent; the file manager still performs the final copy and handles existing destination names.

Local sources support Windows file lists and Ubuntu URI file lists; the destination currently requires Windows. Folders, links, network URIs and copying remote files back locally are not supported. View-only sessions cannot transfer files, and old peers without the capability keep the button disabled. Receivers use a private native staging directory without a Save dialog, retaining it until application exit so ending a session does not remove a source while the system is copying it. Abnormal exit can leave temporary files. A changed foreground window, ended authorization or paused input prevents automatic paste. When no local files are copied, Ctrl+V retains ordinary paste from the remote clipboard.

## Send and receive through a standalone file connection

These steps apply from client 0.1.23. Older 0.1.22 packages use the separate **Allow receiving files** switch, which starts off after each application restart; update to use the unified setting and validated restoration.

1. Sign in and add each computer once. Keep both applications running. On the receiving computer, select the local device or open Settings and enable **Allow remote connections**. This single setting enables screen and file requests on Windows, and file requests only on Ubuntu. It is initially off. The choice is saved and restored after an ordinary restart only when the same login/device and required native services pass validation; manually turning it off keeps it off. Logout or an identity change clears enabled preferences.
2. On the sending computer, select the online recipient in **My devices** and click **File transfer**. A recipient with remote connections disabled or an offline/disabled device cannot receive a request. The sending computer does not need to enable incoming remote connections just to send files.
3. Approve **Allow file transfer** on the recipient. Every file connection requires explicit approval, including same-account connections with remote standby enabled. This authorizes files only; it does not authorize viewing the screen or sending mouse/keyboard input.
4. After connection, open **File transfer** and click **Choose file to send**. A native Open dialog selects one regular local file. The request identifies its basename and size, while its local path stays in Rust. Cancelling the dialog creates no transfer.
5. The recipient sees the offer and chooses **Save to…** or **Reject**. A native Save As dialog selects the exact destination for that file. Cancelling this dialog keeps the offer waiting; no file is created until a destination is selected. Sending approval alone never writes a file automatically.
6. Both sides can send files through the approved connection. The record shows pending offers, progress, completion, rejection, cancellation or a failure reason. A completed file is not opened or executed automatically. Use **Cancel transfer** to stop an offered outgoing file or an active transfer, and **End file connection** to revoke this connection.

Closing the main window from 0.1.23 hides FarSail to an available system tray or minimizes it otherwise, without stopping file transfers. Reopen the interface for approvals and save choices; explicitly quitting through the tray or sidebar stops transfers. Older 0.1.22 packages retain their previous close behavior.

For another account, create a one-time invitation with **Only transfer files**, then give the recipient device ID and invitation code to the requester. The target still approves the connection. The invitation cannot grant screen or control access. See [client usage](CLIENT.md) and [authorization API](API.md).

## Local scope and integrity

Standalone file connections use a `files` authorization grant and `Channel::File`. Client 0.1.28 also permits this channel in `control` sessions, sharing the same identity, connection and short lease; `view` still cannot use it. The unified remote setting declares `can_files=true` only while native transport and file handling are available. Windows also enables `can_host` after its screen checks pass; Ubuntu keeps `can_host=false`. These remain separate protocol capabilities and grants, without separate user switches. Turning off remote connections stops incoming screen and file sessions locally before attempting server capability updates. Logout, transport shutdown and authorization loss also stop local operations; delayed picker results cannot revive an ended session.

The peer cannot browse local directories or select arbitrary local paths. Each native Open dialog grants access to one chosen source file, and each native Save As dialog grants a destination for one offered file. React receives session/task IDs, display names, sizes, progress and errors, never full local paths, signing keys, account/device tokens or grants.

The receiver opens the user-selected parent directory as a capability and creates a random exclusive `.farsail-part-*` file there. It checks contiguous offsets, announced size and the final SHA-256 before publishing the verified file through an atomic, non-replacing hard link. An existing file, directory or symlink at the destination causes failure, including one created after acceptance. There is no overwrite or rename/copy fallback that weakens this rule. Cancellation, disconnect and integrity failure drop the receiver and remove its partial file during normal cleanup.

Choose a filesystem with hard-link support, such as a suitable ext4 or NTFS destination. FAT/exFAT and other filesystems lacking hard links cannot publish a received file; saving fails instead of silently switching to an unsafe method. An abrupt process kill or power loss can leave a `.farsail-part-*` file. After ensuring no FarSail transfer is running, inspect only the destination selected for that transfer; the application does not recursively scan and delete unrelated files.

## Connection and limits

File connections reuse the existing iroh endpoint identity and path-discovery state, reserving one direct P2P attempt window of up to 3 seconds. Known IP addresses are dialed directly first. With only a relay address, a trusted connection can carry discovery and hole-punch signaling, but file authorization and payload wait for direct upgrade. An available direct path is used immediately; relay file payload is allowed only after the window expires without direct connectivity. Fallback does not start a second wait. An explicit advanced **Always use relay** setting is respected. Relay connections require normal trusted HTTPS/TLS validation; certificate checks are never disabled. The panel displays the actual selected direct or relay path, not a promise that direct access will succeed on every network. This policy does not restart or replace an existing screen connection.

The protocol streams at most 64 KiB per chunk and waits for its acknowledgement before sending the next chunk. All outgoing file sessions in one application share a fixed 4 MiB/s file bandwidth budget. This is a file-protocol budget, not a guarantee about link speed, aggregate screen-plus-file traffic or network overhead. A single file is limited to 64 GiB. The sender checks the source content before offering it and rechecks the content while sending; the receiver checks the full SHA-256 before reporting successful completion. Larger files take time to hash before the offer appears.

Standalone connections select one regular file at a time; remote paste supports up to 32 files. Shared limits: no recursive directories, remote filesystem browser, overwrite, pause/resume across disconnects, selective compression or automatic execution. After a failed or ended transfer, obtain a new connection approval if needed, select the file again and select a new destination name. File-content validation and permission checks do not prove physical cross-NAT reachability or performance on every disk/network.

## Verification and troubleshooting

The 0.1.23 unified setting and preference restoration passed local regressions and the three-platform [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39208981511). This includes Windows installation/restart and Linux native checks; physical transfer dialogs and two-computer acceptance remain pending. The following evidence records the earlier file-transfer baseline, rather than new physical-device verification.

TypeScript/Vite, Chromium/WebKit file-interface and existing desktop-interface regressions passed locally. Synthetic IPC covers separate permissions, Linux file approval with screen sharing disabled, picker cancellation, accept/reject/cancel, progress/results, path labels and narrow layout.

Separately, five integration cases passed with isolated PostgreSQL, coordinator HTTP, two Linux native clients, QUIC and filesystem actors; 44 client, 17 transport and 39 desktop unit checks passed. Real transport checks cover direct preference, UDP black holes and cached relays, relay-only discovery of a direct path, and fallback after the shared window. Ubuntu native probes confirm receiving defaults off, viewer windows cannot invoke file commands and WebView filesystem reads are denied. Real Open/Save dialogs, physical Windows and internet two-machine acceptance remain pending; explicit fixture paths and synthetic IPC cannot prove those checks. See [FILES-031 verification](verification/WI-FILES-031.md) for scope and evidence.

| Symptom                          | Check                                                                                                                                           |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| File action is unavailable       | The local device is bound; the recipient is online/enabled and allows remote connections.                                                       |
| No received file appears         | Approve the file connection, then choose a destination for the individual offer. Screen approval alone is insufficient.                         |
| Saving fails                     | Choose a new filename, check free space and directory permissions, and use a filesystem with hard-link support.                                 |
| No direct connection             | Check the configured trusted relay and the actual path shown in the panel. Direct connectivity depends on the network.                          |
| Transfer ends or integrity fails | Confirm both apps remain signed in, remote connections/authorization remain enabled and the source file was not modified; request/choose again. |

Related: [Ubuntu installation](UBUNTU_INSTALL.md), [Windows installation](WINDOWS_INSTALL.md), [authenticated transport](TRANSPORT.md), [screen/control permissions](REMOTE.md), and [implementation status](IMPLEMENTATION.md).
