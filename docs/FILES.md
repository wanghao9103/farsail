**English** | [简体中文](FILES.zh-CN.md)

# Desktop file transfer

The current source candidate adds bidirectional single-file transfer to Ubuntu and Windows desktops. This page describes that candidate, not the capabilities of an older published installer. Upgrade both desktops and the coordinator together: the coordinator must accept Linux file capability and keep file approval independent from screen sharing. Windows code, NTFS and native viewer CI checks passed; installer lifecycle acceptance remains under review. Native file dialogs have not yet been accepted on a physical Windows machine, and a new public installer has not been published.

## Send and receive

1. Sign in and add each computer once. Keep both applications running. On the receiving computer, select the local device or open Settings and enable **Allow receiving files**. Receiving is off by default and is not restored automatically after restarting the application.
2. On the sending computer, select the online recipient in **My devices** and click **File transfer**. A recipient with receiving disabled or an offline/disabled device cannot receive a request. The sending computer does not need to enable receiving just to send files.
3. Approve **Allow file transfer** on the recipient. Every file connection requires explicit approval, including same-account connections with remote standby enabled. This authorizes files only; it does not authorize viewing the screen or sending mouse/keyboard input.
4. After connection, open **File transfer** and click **Choose file to send**. A native Open dialog selects one regular local file. The request identifies its basename and size, while its local path stays in Rust. Cancelling the dialog creates no transfer.
5. The recipient sees the offer and chooses **Save to…** or **Reject**. A native Save As dialog selects the exact destination for that file. Cancelling this dialog keeps the offer waiting; no file is created until a destination is selected. Sending approval alone never writes a file automatically.
6. Both sides can send files through the approved connection. The record shows pending offers, progress, completion, rejection, cancellation or a failure reason. A completed file is not opened or executed automatically. Use **Cancel transfer** to stop an offered outgoing file or an active transfer, and **End file connection** to revoke this connection.

For another account, create a one-time invitation with **Only transfer files**, then give the recipient device ID and invitation code to the requester. The target still approves the connection. The invitation cannot grant screen or control access. See [client usage](CLIENT.md) and [authorization API](API.md).

## Local scope and integrity

Files use a distinct `files` authorization grant and `Channel::File`; `view` and `control` sessions cannot use this channel. Enabling reception declares `can_files=true` only while the native transport and local receiving switch are active. The switch is independent from `can_host`. Turning off receiving stops incoming file sessions locally before attempting server capability updates. Logout, transport shutdown and authorization loss also stop local operations; delayed picker results cannot revive an ended session.

The peer cannot browse local directories or select arbitrary local paths. Each native Open dialog grants access to one chosen source file, and each native Save As dialog grants a destination for one offered file. React receives session/task IDs, display names, sizes, progress and errors, never full local paths, signing keys, account/device tokens or grants.

The receiver opens the user-selected parent directory as a capability and creates a random exclusive `.farsail-part-*` file there. It checks contiguous offsets, announced size and the final SHA-256 before publishing the verified file through an atomic, non-replacing hard link. An existing file, directory or symlink at the destination causes failure, including one created after acceptance. There is no overwrite or rename/copy fallback that weakens this rule. Cancellation, disconnect and integrity failure drop the receiver and remove its partial file during normal cleanup.

Choose a filesystem with hard-link support, such as a suitable ext4 or NTFS destination. FAT/exFAT and other filesystems lacking hard links cannot publish a received file; saving fails instead of silently switching to an unsafe method. An abrupt process kill or power loss can leave a `.farsail-part-*` file. After ensuring no FarSail transfer is running, inspect only the destination selected for that transfer; the application does not recursively scan and delete unrelated files.

## Connection and limits

For a new file connection, known IP addresses receive a direct P2P attempt window of up to 3 seconds before the configured relay is used as fallback. Existing iroh endpoint identity and discovery state are retained. An explicit advanced **Always use relay** setting is respected. Without usable direct addresses, a configured relay may be used immediately. Relay connections require normal trusted HTTPS/TLS validation; certificate checks are never disabled. The panel displays the actual selected direct or relay path, not a promise that direct access will succeed on every network. This policy does not restart or replace an existing screen connection.

The protocol streams at most 64 KiB per chunk and waits for its acknowledgement before sending the next chunk. All outgoing file sessions in one application share a fixed 4 MiB/s file bandwidth budget. This is a file-protocol budget, not a guarantee about link speed, aggregate screen-plus-file traffic or network overhead. A single file is limited to 64 GiB. The sender checks the source content before offering it and rechecks the content while sending; the receiver checks the full SHA-256 before reporting successful completion. Larger files take time to hash before the offer appears.

Current limits: individual regular files only; no recursive directories, remote filesystem browser, overwrite, pause/resume across disconnects, selective compression or automatic execution. After a failed or ended transfer, obtain a new connection approval if needed, select the file again and select a new destination name. File-content validation and permission checks do not prove physical cross-NAT reachability or performance on every disk/network.

## Verification and troubleshooting

TypeScript/Vite, Chromium/WebKit file-interface and existing desktop-interface regressions passed locally. Synthetic IPC covers separate permissions, Linux file approval with screen sharing disabled, picker cancellation, accept/reject/cancel, progress/results, path labels and narrow layout.

Separately, five integration cases passed with isolated PostgreSQL, coordinator HTTP, two Linux native clients, QUIC and filesystem actors; 44 client, 15 transport and 39 desktop unit checks passed. Real transport checks cover direct preference, trusted-relay fallback after a UDP black hole, and honoring the direct window with a cached relay. Ubuntu native probes confirm receiving defaults off, viewer windows cannot invoke file commands and WebView filesystem reads are denied. Real Open/Save dialogs, physical Windows and internet two-machine acceptance remain pending; explicit fixture paths and synthetic IPC cannot prove those checks. See [FILES-031 verification](verification/WI-FILES-031.md) for scope and evidence.

| Symptom                          | Check                                                                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| File action is unavailable       | The local device is bound; the recipient is online/enabled and has enabled receiving.                                            |
| No received file appears         | Approve the file connection, then choose a destination for the individual offer. Screen approval alone is insufficient.          |
| Saving fails                     | Choose a new filename, check free space and directory permissions, and use a filesystem with hard-link support.                  |
| No direct connection             | Check the configured trusted relay and the actual path shown in the panel. Direct connectivity depends on the network.           |
| Transfer ends or integrity fails | Confirm both apps remain signed in, receiving/authorization is valid and the source file was not modified; request/choose again. |

Related: [Ubuntu installation](UBUNTU_INSTALL.md), [Windows installation](WINDOWS_INSTALL.md), [authenticated transport](TRANSPORT.md), [screen/control permissions](REMOTE.md), and [implementation status](IMPLEMENTATION.md).
