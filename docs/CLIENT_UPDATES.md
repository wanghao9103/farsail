**English** | [简体中文](CLIENT_UPDATES.zh-CN.md)

# Client builds and automatic updates

## What users install

GitHub Actions builds Windows x64 NSIS installers and Ubuntu x64 and ARM64 Debian packages. Successful publication places all three installers in the versioned client release; Actions artifacts are temporary build attachments and are not the update channel. Existing clients without the updater need one manual installation of the first updater-enabled version. Server packages do not update the desktop application.

In Settings, Software updates shows the installed version, check result and download progress. Automatic checking is enabled initially: the installed release checks shortly after startup and once per hour. Automatic downloading and installation is an optional setting. It waits for remote sessions and viewer windows to close, then installs and restarts. Ubuntu installation may require a system authorization prompt; Windows uses the existing per-user installer. Preferences survive restarts. Development builds do not check the public channel automatically.

The channel is [client-updates](https://github.com/wanghao9103/farsail/releases/tag/client-updates), with the feed at `latest.json`. The signed 0.1.20 release and feed were publicly available on 2026-10-10 for Windows x64 and Ubuntu x64. ARM64 is a new target in the 0.1.21 source candidate and has not been publicly signed and released. The feature and workflow alone do not establish publication. Ubuntu 24.04 is the x64/ARM64 CI baseline; a local x64 build on Ubuntu 26.04 cannot establish ARM64 compatibility.

ARM64 uses system name `aarch64`, Debian suffix `arm64.deb` and manifest target `linux-aarch64-deb`. x64 retains `linux-x86_64-deb`; Windows retains `windows-x86_64-nsis`. The client selects the exact entry for its build architecture and validates the installer filename. On ARM64, an older feed without this entry reports no compatible update instead of falling back to amd64. See the [Ubuntu client](UBUNTU_INSTALL.md) for installation and architecture queries.

Native ARM64 CI uses GitHub's official [hosted runner](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job); an x64 build or available QEMU is not evidence of native ARM execution.

## Maintainer setup

The application embeds only the public updater key. Store the matching private key as the GitHub Actions repository secret `TAURI_SIGNING_PRIVATE_KEY`; the repository workflow sets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to an empty string for the current passwordless signing key. Back up the private key securely outside the repository. Do not add it to commits, release attachments, logs or documentation. Losing this key prevents existing installations from accepting future signatures. A different key cannot simply replace it for installed clients.

The repository Secret was confirmed by name on 2026-10-10, and the cloud signed 0.1.20 publication completed. Restore the matching key on another maintainer machine rather than generating a replacement public key. The updater signature authenticates package bytes and version; it is separate from a Windows Authenticode publisher certificate and does not eliminate SmartScreen warnings.

For maintainers who prefer not to paste a private key, the setup helper verifies the local key against the embedded public key and saves the secret through the authenticated GitHub CLI. Secret values pass over standard input, not command arguments or logs; GitHub CLI encrypts them locally before upload. Ordinary client users never configure a signing secret. Running the helper without the apply flag only performs a local check. The apply command requires an account allowed to write this repository's Secrets. Restore the backed-up matching key on another machine instead of generating a replacement. This repository is already initialized; ordinary client users do not run the maintainer commands below.

```bash
gh auth login --hostname github.com
node scripts/configure-client-updates.mjs --apply
```

## Automatic release process

Update the client version in `apps/desktop/src-tauri/tauri.conf.json`, merge reviewed source, and push a matching tag. The tag below is an example for the current candidate, to run after review and verification; it does not mean this version is released.

```bash
git tag client-v0.1.21
git push origin client-v0.1.21
```

The workflow also supports publishing a matching client release from the GitHub web interface or selecting a source ref and matching version tag in Run workflow. Run workflow builds signed candidates by default; only explicitly selecting `publish_release` publishes the version and advances the feed. Tag and client Release events still follow the publication flow, without a local build toolchain.

The `Client installers and signed updates` workflow automatically builds three architecture targets. Windows validates the installed binary, native settings, restart, sharing preferences, reinstallation and uninstallation. Ubuntu uses native `ubuntu-24.04` and `ubuntu-24.04-arm` runners to check WebKitGTK IPC, keyring behavior, package metadata, actual ELF architecture and runtime dependencies, keeping a generic ARM64 instruction target. Publication verifies all three signatures, signed versions, source commits, checksums, Linux architectures and the Windows installation report before uploading. Pull-request builds run without the production signing key and provide preview artifacts only.

The versioned release retains immutable installers, signatures, checksums and verification metadata. A successful public download of each uploaded file must match the tested bytes before the channel advances. The channel cannot move to an older or equal version. All three targets are required: a failed build, missing signature or failed installation check leaves the previous channel unchanged. The channel stages the next manifest before replacing the existing asset; clients retain their working installation if a check fails during this brief replacement window.

## Verification and limits

```bash
node --test scripts/test-client-update-manifest.mjs
cargo test --locked -p farsail-desktop --lib
```

The manifest regressions use a disposable signing key and reject altered packages, modified signed comments, wrong advertised versions, mixed source commits, missing signatures and missing installation reports. UI fixtures verify preference changes, failed-download retries and waiting for a remote connection. These checks are separate from a real installed-client download, operating-system installer authorization and post-update restart. The two signed 0.1.20 packages and anonymous downloads passed verification; an actual installed-client cross-version installation/restart remains pending. The new ARM64 candidate requires separate native CI and subsequent public signed-release evidence. This document does not claim a public ARM64 installer.
