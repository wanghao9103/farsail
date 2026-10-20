**English** | [简体中文](CLIENT_UPDATES.zh-CN.md)

# Client builds and automatic updates

## What users install

GitHub Actions builds Windows x64 NSIS installers and Ubuntu x64 and ARM64 Debian packages. Successful publication places all three installers in the versioned client release; Actions artifacts are temporary build attachments and are not the update channel. Existing clients without the updater need one manual installation of the first updater-enabled version. Server packages do not update the desktop application.

In Settings, Software updates shows the installed version, check result and download progress. Automatic checking is enabled initially: the installed release checks shortly after startup and once per hour. Automatic downloading and installation is an optional setting. It waits for remote sessions and viewer windows to close, then installs and restarts. Windows uses the existing per-user installer. Preferences survive restarts. Development builds do not check the public channel automatically.

The channel is [client-updates](https://github.com/wanghao9103/farsail/releases/tag/client-updates), with the feed at `latest.json`. The [signed 0.1.21 release](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.21) and feed were publicly available on 2026-10-10 for Windows x64, Ubuntu x64 and Ubuntu ARM64. GitHub Actions advanced the channel after all three builds, native checks, signatures and public-download checks passed. Ubuntu 24.04 is the x64/ARM64 CI baseline; a local x64 build on Ubuntu 26.04 cannot establish ARM64 compatibility.

On 2026-10-19, the channel advanced to [signed 0.1.22](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.22) for all three targets after the [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39199106307) and public-download verification passed. Installations on 0.1.20 or 0.1.21 can open Sharing and settings → Software updates → Check for updates, then download and follow the installation prompts. The update dialog described below is available after upgrading to 0.1.22.

The channel previously provided [signed 0.1.23](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.23), built from `c217f20823e864ccd2d76bf19f29ad9ccd377309` by the [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39208981511). All three platform packages, signatures and public-download checks passed; the fixed channel feed matches the versioned manifest. Existing clients can use Sharing and settings → Software updates → Check for updates, then Install and restart after active connections end.

The channel previously advanced to [0.1.24](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.24), built from `c3ffb8aab093f2c3db4deeb98c7efcff97d9aa56` by the [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39213734063). All three native/package checks, public signatures and hashes passed, and the ordinary fixed feed matches the versioned manifest byte for byte. This update improves capture scheduling and bounded media pipelining and removes image-hover hints. Update both clients before comparing results. The 30 FPS cadence is a target, not measured game performance on user hardware.

Version 0.1.23 combines screen sharing and file receiving under **Allow remote connections**, with saved preferences and platform-specific capabilities. Closing the main window keeps the app running through the tray/taskbar; choose Quit FarSail from the tray or sidebar before a manual installation. Windows requests administrator permission at startup through system UAC, and cancellation cancels that launch; the installer remains per-user. Windows CI verified the embedded manifest, elevated process, tray close/restore and NSIS `/UPDATE /R` restart. Interactive UAC approval/cancellation and an actual installed-client cross-version update remain unverified. See [file transfer](FILES.md) for the current behavior.

ARM64 uses system name `aarch64`, Debian suffix `arm64.deb` and manifest target `linux-aarch64-deb`. x64 retains `linux-x86_64-deb`; Windows retains `windows-x86_64-nsis`. The client selects the exact entry for its build architecture and validates the installer filename. On ARM64, an older feed without this entry reports no compatible update instead of falling back to amd64. See the [Ubuntu client](UBUNTU_INSTALL.md) for installation and architecture queries.

Native ARM64 CI uses GitHub's official [hosted runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job); an x64 build or available QEMU is not evidence of native ARM execution.

The channel previously pointed to [0.1.27](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.27), built from `bca87a13e4ea246e0a5e5eed5ade4221d4c781ba` by the [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39233924894). Three-platform native/package checks, public signatures and hashes passed, and the ordinary fixed feed matches the versioned manifest byte for byte. Both Linux architectures passed the real signed-helper sandbox and package-permission checks; live desktop authentication, authorization after reboot and managed-system policy remain unverified.

The channel now points to [0.1.28](https://github.com/wanghao9103/farsail/releases/tag/client-v0.1.28), built from `11a75bbfe5a49fd2665c98b34bb3d7c817cd255c` by the [release workflow](https://github.com/wanghao9103/farsail/actions/runs/39365699777). Three-platform signed packages, native installation checks and public byte/signature verification passed; the ordinary fixed feed matches the versioned and rebuilt manifest. This release adds file copy/paste within one remote-control approval. Update both clients, then use **Settings → Software updates → Check for updates**, finishing active connections before installation. Physical two-machine file paste remains unverified.

## Update dialog in 0.1.22

Version 0.1.22 adds the update dialog and behavior described below, alongside clearer copy and layout improvements across the desktop interface.

Discovering a new version opens a Software update dialog when the client has no active remote screen or file connection, auxiliary native window or other modal dialog. Automatic reminders also wait during ongoing operations, while a connection or registration invitation is retained, and while registering, verifying email or recovering a password. Each available version prompts at most once during an application run; restarting the client allows a reminder again. This does not depend on how long the keyboard or mouse has been idle.

Choose Later to close the dialog without another automatic reminder for that version during the same run. If automatic downloading and installation is already enabled, the close action is labelled Update in background; once installation starts, it becomes Close window. Closing the dialog does not change either update preference or cancel a download or installation already in progress. The update notice and Software updates settings both have View update, which reopens the dialog manually.

The dialog groups downloading, download progress, retrying a failed update and Install and restart. When the total size is unknown, it shows the downloaded amount instead of a percentage. With automatic installation off, Update now downloads and verifies the package; then choose Install and restart. If a remote screen or file connection or an auxiliary window is still open, installation waits. End those activities and choose Install and restart again. With automatic installation on, the client retries the waiting installation automatically.

Automatic checking controls scheduled discovery; automatic downloading and installation controls what happens to an available update. Enabling automatic downloading and installation also enables automatic checking. Turning off automatic checking alone does not cancel automatic processing of an update already found. Discovering a version, opening the dialog or dismissing it never enables either preference. Preferences survive restarts; invalid preference content disables both automatic settings rather than enabling them.

## Ubuntu update authorization

Version 0.1.27 removes repeated update explanations and maps failures to brief, actionable messages. Native diagnostics are not displayed in the update window. Its Debian package installs a root-owned helper and a dedicated Polkit action. The first successful in-app update authorizes the calling account for later signed FarSail updates. Upgrading through the older pkexec-based updater reuses that installation authorization; a manual installation without a caller identity waits for the first in-app update instead.

The rule applies only to this helper's install action for the same account in an active local session. The helper independently verifies the embedded release key, signed version, package name and native architecture, rejects equal/older versions, and copies the package into a root-owned directory before invoking dpkg. It accepts signed bytes on standard input, never an arbitrary file path or command. Upgrades preserve authorization; uninstalling revokes it. To revoke it separately:

```bash
pkexec /usr/libexec/farsail-update-helper revoke
```

The signed-package sandbox tests signature rejection, metadata/version checks, account rule matching, explicit revocation and package maintenance scripts without changing the host installation. It does not establish actual desktop authentication, retained authorization after reboot, or managed-system policy precedence.

## Maintainer setup

The application embeds only the public updater key. Store the matching private key as the GitHub Actions repository secret `TAURI_SIGNING_PRIVATE_KEY`; the repository workflow sets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to an empty string for the current passwordless signing key. Back up the private key securely outside the repository. Do not add it to commits, release attachments, logs or documentation. Losing this key prevents existing installations from accepting future signatures. A different key cannot simply replace it for installed clients.

The repository Secret was confirmed by name on 2026-10-10, and cloud signed 0.1.20 and 0.1.21 publication completed. Restore the matching key on another maintainer machine rather than generating a replacement public key. The updater signature authenticates package bytes and version; it is separate from a Windows Authenticode publisher certificate and does not eliminate SmartScreen warnings.

For maintainers who prefer not to paste a private key, the setup helper verifies the local key against the embedded public key and saves the secret through the authenticated GitHub CLI. Secret values pass over standard input, not command arguments or logs; GitHub CLI encrypts them locally before upload. Ordinary client users never configure a signing secret. Running the helper without the apply flag only performs a local check. The apply command requires an account allowed to write this repository's Secrets. Restore the backed-up matching key on another machine instead of generating a replacement. This repository is already initialized; ordinary client users do not run the maintainer commands below.

```bash
gh auth login --hostname github.com
node scripts/configure-client-updates.mjs --apply
```

## Automatic release process

Update the client version in `apps/desktop/src-tauri/tauri.conf.json`, merge reviewed source, and push a matching tag. The tag below records the 0.1.21 publication. It already exists; do not recreate or retarget it. Future publication must increment the application version and use a corresponding new tag.

```bash
git tag client-v0.1.21
git push origin client-v0.1.21
```

The workflow also supports publishing a matching client release from the GitHub web interface or selecting a source ref and matching version tag in Run workflow. Run workflow builds signed candidates by default; only explicitly selecting `publish_release` publishes the version and advances the feed. Tag and client Release events still follow the publication flow, without a local build toolchain.

The `Client installers and signed updates` workflow automatically builds three architecture targets. Windows validates the installed binary and administrator manifest, elevated runtime, native settings, restart, remote preferences, tray restoration, NSIS updater restart, reinstallation and uninstallation. Ubuntu uses native `ubuntu-24.04` and `ubuntu-24.04-arm` runners to check WebKitGTK IPC, keyring behavior, package metadata, actual ELF architecture and runtime dependencies, keeping a generic ARM64 instruction target. Publication verifies all three signatures, signed versions, source commits, checksums, Linux architectures and the Windows installation report before uploading. Pull-request builds run without the production signing key and provide preview artifacts only.

The versioned release retains immutable installers, signatures, checksums and verification metadata. A successful public download of each uploaded file must match the tested bytes before the channel advances. The channel cannot move to an older or equal version. All three targets are required: a failed build, missing signature or failed installation check leaves the previous channel unchanged. The channel stages the next manifest before replacing the existing asset; clients retain their working installation if a check fails during this brief replacement window.

## Verification and limits

```bash
node --test scripts/test-client-update-manifest.mjs
cargo test --locked -p farsail-desktop --lib
```

The manifest regressions use a disposable signing key and reject altered packages, modified signed comments, wrong advertised versions, mixed source commits, missing signatures and missing installation reports. UI fixtures verify preference changes, failed-download retries and waiting for a remote connection. These checks are separate from a real installed-client download, operating-system installer authorization and post-update restart. The three signed 0.1.21 packages and anonymous downloads passed verification; the channel matches the versioned manifest byte for byte. ARM64 native keyring, GTK IPC, database file flows and transport checks passed. The published 0.1.22 installers, signatures and metadata also passed public-download verification, and the channel manifest matches the versioned release byte for byte. The published 0.1.23 artifacts also passed those signature and public-byte checks. Its native checks include Linux tray-host and no-host behavior and Windows `/UPDATE /R` restart, which do not establish interactive UAC or an old installed client's complete upgrade. Physical ARM installation, real installed-client cross-version download/installation/restart, interactive UAC, game input and public two-computer acceptance remain pending.
