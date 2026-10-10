**English** | [简体中文](SERVER_UPDATES.zh-CN.md)

# Coordinator online updates

Once the updater is installed, run this command on the server. It discovers the newest eligible coordinator package, obtains its checksum, verifies the download and performs the existing backup and upgrade procedure. You do not supply a download URL or SHA256.

```bash
/home/data/farsail/farsail-update
```

Check the available release without downloading its image or changing the deployment:

```bash
/home/data/farsail/farsail-update --check
```

The command updates only the coordinator. [Client updates](CLIENT_UPDATES.md) use a separate installation and signing flow. Allow a short coordinator outage when an actual upgrade is needed; existing remote sessions may need to reconnect.

## Install the entry point once

Earlier server bundles contain `update-online.sh`, but their upgrade procedure does not install a persistent command. An extracted script can disappear when its temporary directory is removed. The earlier script also requires a URL and checksum; installing the new client does not replace that script on the server.

On an existing server, copy the commands below to download the first-install tool, verify its supplied checksum and install the entry point. No local upload or manually entered URL/checksum is needed. This step does not restart a service:

```bash
(
  set -e
  farsail_setup=$(mktemp -d)
  trap 'rm -rf -- "$farsail_setup"' EXIT
  curl -q --fail --show-error --location --proto '=https' --proto-redir '=https' \
    --retry 2 --connect-timeout 20 --max-time 120 --max-filesize 1048576 \
    --output "$farsail_setup/install-farsail-update.sh" \
    'https://github.com/wanghao9103/farsail/releases/download/coordinator-files-d21b298-online1/install-farsail-update.sh'
  printf '%s  %s\n' '9640cd48a0787b3ddbe0b2432ded99b0737595750f04acc3c8f7875c7fb3f7ed' \
    "$farsail_setup/install-farsail-update.sh" | sha256sum -c -
  bash "$farsail_setup/install-farsail-update.sh"
)
```

The script fetches two tools from a fixed commit and verifies their embedded SHA256 values. If `raw.githubusercontent.com` is unreachable, download `FarSail_server_updater_fc17a7c.tar.gz` and its same-name `.sha256` from [this release](https://github.com/wanghao9103/farsail/releases/tag/coordinator-files-d21b298-online1), verify and extract it, then follow its installation instructions.

The verified server bundle also includes `install-online-updater.sh`. After transferring and verifying that bundle, install the same entry point:

```bash
bash /tmp/farsail-coordinator-release/install-online-updater.sh \
  /tmp/farsail-coordinator-release /home/data/farsail
```

Use the existing deployment root instead of `/home/data/farsail` if different. Preserve the original `FARSAIL_STATE_DIR` before installation when a custom state directory was used. The installer records the absolute root and state paths, installs the updater privately under the state directory, and creates `farsail-update` in the deployment root. Subsequent invocations need neither paths nor release parameters. Run as the deployment owner with access to Docker, configuration files and backups; installation needs write access to the root and state directory.

A current source checkout also supports the direct entry point:

```bash
bash scripts/deploy/update-online.sh --latest /home/data/farsail
bash scripts/deploy/update-online.sh --latest --check /home/data/farsail
```

**Availability:** the [Files and automatic-update server release](https://github.com/wanghao9103/farsail/releases/tag/coordinator-files-d21b298-online1) was published on 2026-10-10. Asset `FarSail_coordinator_files_d21b298_online1_linux_amd64.tar.gz` has SHA256 `3f78775afff275c6ff8652c5300e759f88e4b3a3a96e17b8f5038081f48f970c`; its matching checksum and first-install tools are included. The image source is `d21b298d9b4327032c204f852a540c7d4a88f433`; tools and the release tag use `fc17a7ce4ef84b5569b791da54534a95ff42a450`. Verified archive bytes are unchanged. The bundled “not yet published” text and `published: false` are build-time snapshots; this Release records the public status. Production has not been upgraded. The old URL/checksum command remains available for an intentionally selected historical package; see [the Ubuntu/login upgrade](UPGRADE_UBUNTU_LOGIN.md) and [the email upgrade](EMAIL_VERIFICATION_UPDATE.md).

## How the latest server package is selected

The updater queries the public releases of `wanghao9103/farsail`, selects published `coordinator-*` releases, and accepts both regular and preview releases in the current server channel. It accepts only `FarSail_coordinator_*_linux_amd64.tar.gz` and its exact same-name `.sha256` companion. Client installers, client update manifests, full deployment archives, automatic Source code archives, drafts and historical packages without the new protocol are excluded.

Selection uses the package asset creation time and asset ID, because a later server package can be added to an existing release. If the newest package is incomplete, malformed or fails verification, the updater stops instead of selecting an older package. Requests and pagination are bounded. GitHub's generic latest endpoint is unsuitable here: the repository mixes client and server releases, and its currently published releases are previews. See the [GitHub Releases API](https://docs.github.com/en/rest/releases/releases).

The selected identity stays fixed for the entire invocation. HTTPS verification, exact checksum companion syntax, the API asset digest when supplied, the actual archive digest, safe archive paths, internal checksums and coordinator metadata must pass before the upgrade script can run. No GitHub login, personal token, SMTP password or client signing key is needed to download public updates.

New packages declare `online_update_protocol: 1` and the full `source_revision` of the coordinator image in `coordinator-release.json`. A separately refreshed script bundle can record `package_source_revision` without claiming that its unchanged image was rebuilt. Publishing a package requires its checksum companion and the new upgrade/installation scripts; renaming an old package does not make it compatible.

## Applying an update and repeated checks

The upgrade script takes the same deployment operation lock as the other maintenance commands. It rechecks the selected image, installed source and successful online-update receipt under that lock, rejects environment overrides that would redirect Compose to another image or deployment, and verifies the loaded Linux amd64 image configuration digest.

A healthy coordinator already using the selected immutable image and matching metadata returns success without another image load, database backup or restart. An unhealthy or mismatched same-tag deployment reports a problem rather than claiming to be current.

For an actual upgrade, the script verifies that the old image remains available, creates a private PostgreSQL dump, checks that it can be read, and backs up the existing image selection and release records. It recreates only the coordinator and waits for health. Success writes the release metadata and online-update receipt atomically and refreshes the persistent updater when supplied. Upgrade or record-publication failure restores the previous coordinator selection and corresponding release records, then attempts to recreate the old coordinator. Database migrations remain applied; rollback does not restore an older database over current user data. If only the subsequent tool installation fails, the coordinator remains at its healthy, committed version and the installer restores its earlier tool entry; the diagnostic distinguishes this case.

Receipts reject older known release assets and changed checksums for the same asset. Where installed source metadata exists, the fixed repository comparison must confirm that the candidate source is identical or ahead, and the locked baseline must still match. The two known historical coordinator packages can establish the initial baseline. Unknown older deployments or unpublished source histories that cannot be compared stop with a diagnostic and require one explicitly verified migration; asset timestamps alone do not prove source compatibility.

## Verification and troubleshooting

After a successful upgrade, inspect the coordinator and then test the affected business feature:

```bash
cd /home/data/farsail
cloud_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
docker compose --env-file "$cloud_state/compose.env" \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file "$cloud_state/compose.env" \
  -f deploy/production/compose.yaml logs --tail 80 coordinator
```

For a custom state directory, use the same recorded path as the installed updater. Container health proves startup, not email delivery, Files compatibility or internet remote control. Upgrade both clients to the matching Files-capable version before testing transfers.

| Result                                         | Next step                                                                                           |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| No eligible automatic-update package           | Publish a verified package using the new protocol; do not substitute a client or old email package. |
| API rate limit, network or certificate failure | Resolve outbound access, DNS, clock or CA issues and retry; do not disable TLS verification.        |
| Missing or incorrect checksum                  | Stop and correct the published release. The deployment has not been upgraded.                       |
| Another deployment operation is active         | Wait for the existing operation to complete, then retry.                                            |
| Already up to date and healthy                 | Continue business acceptance checks; no restart is needed.                                          |
| Same tag with unhealthy or inconsistent image  | Inspect configuration, running image and coordinator logs before repairing.                         |
| Source baseline changed or candidate is older  | Check concurrent upgrades and the selected release; do not bypass the source/receipt guard.         |
| Startup or metadata publication failed         | Read the rollback result and the exact private backup path.                                         |

Local HTTPS/selection tests, upgrade/rollback/installation contracts, and isolated real Docker/PostgreSQL upgrades and backup restoration passed. Actual anonymous public HTTPS downloads, outer and inner checksums, and the anonymous latest-version check passed after publication; those checks do not mean that this production server has been upgraded. The command checks on invocation; it does not install an unattended schedule.
