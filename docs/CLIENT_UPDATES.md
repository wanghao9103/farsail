**English** | [简体中文](CLIENT_UPDATES.zh-CN.md)

# Client builds and automatic updates

## What users install

GitHub Actions builds Windows x64 NSIS installers and Ubuntu x64 Debian packages. Successful publication places both installers in the versioned client release; Actions artifacts are temporary build attachments and are not the update channel. Existing clients without the updater need one manual installation of the first updater-enabled version. Server packages do not update the desktop application.

In Settings, Software updates shows the installed version, check result and download progress. Automatic checking is enabled initially: the installed release checks shortly after startup and once per hour. Automatic downloading and installation is an optional setting. It waits for remote sessions and viewer windows to close, then installs and restarts. Ubuntu installation may require a system authorization prompt; Windows uses the existing per-user installer. Preferences survive restarts. Development builds do not check the public channel automatically.

The channel is [client-updates](https://github.com/wanghao9103/farsail/releases/tag/client-updates), with the feed at `latest.json`. These URLs become available only after the first successful signed publication. The feature and workflow alone do not establish that packages have been published. Ubuntu 24.04 is the CI build baseline; a local package built on Ubuntu 26.04 cannot stand in for that compatibility check.

## Maintainer setup

The application embeds only the public updater key. Store the matching private key as the GitHub Actions repository secret `TAURI_SIGNING_PRIVATE_KEY`; the repository workflow sets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to an empty string for the current passwordless signing key. Back up the private key securely outside the repository. Do not add it to commits, release attachments, logs or documentation. Losing this key prevents existing installations from accepting future signatures. A different key cannot simply replace it for installed clients.

The generated key is held locally in the ignored private directory; it has not been configured as a repository secret or uploaded. Configure the secret before creating the release tag. The updater signature authenticates package bytes and version; it is separate from a Windows Authenticode publisher certificate and does not eliminate SmartScreen warnings.

## Automatic release process

Update the client version in `apps/desktop/src-tauri/tauri.conf.json`, merge reviewed source, and push a matching tag. The tag below is an example for the initial candidate, not a statement that it has been released.

```bash
git tag client-v0.1.20
git push origin client-v0.1.20
```

The workflow also supports publishing a matching client release from the GitHub web interface or selecting a source ref and matching version tag in Run workflow. This lets maintainers build and publish online without downloading a build toolchain or pushing a tag from their local terminal.

The `Client installers and signed updates` workflow automatically builds both platforms. Windows validates the installed binary, native settings, restart, sharing preferences, reinstallation and uninstallation. Ubuntu checks native WebKitGTK IPC and keyring behavior, package metadata and runtime dependencies on the build baseline. Publication verifies both package signatures, the signed version, source commit, checksums and Windows installation report before uploading. Pull-request builds run without the production signing key and provide preview artifacts only.

The versioned release retains immutable installers, signatures, checksums and verification metadata. A successful public download of each uploaded file must match the tested bytes before the channel advances. The channel cannot move to an older or equal version. Both platforms are required: a failed build, missing signature or failed installation check leaves the previous channel unchanged. The channel stages the next manifest before replacing the existing asset; clients retain their working installation if a check fails during this brief replacement window.

## Verification and limits

```bash
node --test scripts/test-client-update-manifest.mjs
cargo test --locked -p farsail-desktop --lib
```

The manifest regressions use a disposable signing key and reject altered packages, modified signed comments, wrong advertised versions, mixed source commits, missing signatures and missing installation reports. UI fixtures verify preference changes, failed-download retries and waiting for a remote connection. These checks are separate from a real installed-client download, operating-system installer authorization and post-update restart. Public signed releases and a real cross-version upgrade must still be verified after the repository secret is configured. No new updater-enabled package is claimed published by this document.
