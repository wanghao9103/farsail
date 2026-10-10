**English** | [简体中文](UPGRADE_UBUNTU_LOGIN.zh-CN.md)

# Production upgrade for Ubuntu binding and login feedback

The unauthenticated compatibility probe on 2026-10-09 returned HTTP 422 with `platform: unknown variant linux, expected windows/android/ios`. The running coordinator accepts Windows, Android and iOS only. Upgrade the coordinator to enable Ubuntu binding; reinstalling the client alone is insufficient.

The coordinator update adds Linux controller support, an additive database migration and distinct login errors: `account_not_found`, `invalid_password`, `email_not_verified`, `account_disabled`. Desktop 0.1.18 renders those reasons in Chinese dialogs. Ubuntu is a controller; the remote Windows computer must enable sharing.

## Server online upgrade

For ongoing updates, use [the persistent server updater](SERVER_UPDATES.md), which discovers the latest eligible coordinator package and checksum automatically. The commands below intentionally pin the historical Ubuntu/login fix: they download its coordinator package over HTTPS, check SHA256 and internal checksums, then run the backup and rollback procedure. [That server preview package is published](https://github.com/wanghao9103/farsail/releases/tag/coordinator-ubuntu-login-20261009). [Client updates](CLIENT_UPDATES.md) are separate.

Run on the server as a user with Docker and deployment-file access:

```bash
set -euo pipefail
update_dir=$(mktemp -d /tmp/farsail-update.XXXXXX)
curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
  --retry 3 --connect-timeout 20 --output "$update_dir/package.tar.gz" \
  "https://github.com/wanghao9103/farsail/releases/download/coordinator-ubuntu-login-20261009/FarSail_coordinator_ubuntu-login_20261009.tar.gz"
printf '%s  %s\n' 'bab23db531d2c79acba9666fb699e3465f03cca2718a1d8d66717b5a128aa12a' "$update_dir/package.tar.gz" | sha256sum -c -
mkdir "$update_dir/release"
tar -xzf "$update_dir/package.tar.gz" -C "$update_dir/release"
(cd "$update_dir/release" && sha256sum -c SHA256SUMS)
bash "$update_dir/release/upgrade-coordinator.sh" "$update_dir/release" /home/data/farsail
```

The default deployment root is `/home/data/farsail`; replace the final argument if different. Keep any existing `FARSAIL_STATE_DIR` override. Download or checksum failures leave deployment unchanged. Upgrade requires existing Docker, Compose, jq and database access. The unpublished wording and metadata inside the immutable package record its build-time state; use this document or the release page for the live URL.

A checkout containing the new helper can also run:

```bash
bash scripts/deploy/update-online.sh HTTPS_PACKAGE_URL RELEASE_PACKAGE_SHA256 /home/data/farsail
```

This older pinned mode remains compatible. For routine upgrades, the new entry point removes both placeholders; follow its installation instructions instead of entering a URL and SHA256 each time. Do not execute unchecked remote scripts.

## 1. Upload the offline package

Copy the delivered `FarSail_coordinator_ubuntu-login_20261009.tar.gz` to the server with SCP, using the actual SSH login. The package contains one offline coordinator image, checksums, an upgrade script and instructions. Do not pass it to the existing six-image `load-release` command.

```bash
scp /本机路径/FarSail_coordinator_ubuntu-login_20261009.tar.gz SSH登录名@39.105.83.33:/tmp/
```

## 2. Upgrade on the server

Run as a user with Docker and deployment-file access:

```bash
mkdir -p /tmp/farsail-ubuntu-login-20261009
tar -xzf /tmp/FarSail_coordinator_ubuntu-login_20261009.tar.gz -C /tmp/farsail-ubuntu-login-20261009
cd /tmp/farsail-ubuntu-login-20261009
sha256sum -c SHA256SUMS
bash upgrade-coordinator.sh "$PWD" /home/data/farsail
```

Replace `/home/data/farsail` with the actual checkout directory when different. Preserve any existing `FARSAIL_STATE_DIR` override. The script validates image platform/config digest, backs up PostgreSQL and `compose.env`, and recreates only `coordinator`. Startup applies the new migration automatically. A failed health check restores the previous image configuration. Schedule a short coordination outage outside active remote sessions.

Backups are private and stored under the existing state directory at `backups/coordinator-TIMESTAMP-PID/`. Existing accounts, devices, volumes, certificates, gateway and relay remain. Do not initialize again or run `docker compose down -v`. The server needs no Rust compiler.

## 3. Verify

```bash
cd /home/data/farsail
bash scripts/deploy/farsail.sh status
curl --fail --silent --show-error https://39.105.83.33/healthz
curl --silent --show-error -i https://39.105.83.33/v1/devices/bind \
  -H 'Content-Type: application/json' \
  --data '{"challenge_id":"00000000-0000-0000-0000-000000000000","signature":"00","name":"compatibility-check","platform":"linux","can_host":false,"can_files":false}'
```

The last request has no credentials and must return **401**, proving Linux parsing reaches authentication without creating a device. The old server returns **422 unknown variant linux**. Health status 200 alone cannot identify the server version.

Install the delivered Ubuntu client 0.1.18:

```bash
sudo apt install ./FarSail_0.1.18_ubuntu26.04_amd64.deb
```

Check nonexistent-account and incorrect-password dialogs, log in correctly, add this computer from Overview, then connect to a Windows computer with sharing enabled. Real accounts, binding and cross-computer remote control require production acceptance checks.

## 4. Roll back the coordinator

Restore `compose.env` from the exact backup directory printed by the upgrade, then recreate only the coordinator:

```bash
cd /home/data/farsail
# Replace with the backup directory printed by the upgrade
backup_dir=/home/data/farsail/.local/production/backups/coordinator-ACTUAL-TIMESTAMP-PID
cp "$backup_dir/compose.env" .local/production/compose.env
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml up -d --pull never --no-deps \
  --force-recreate --wait --wait-timeout 180 coordinator
```

Adjust paths for a non-default state directory. Rollback loses Linux binding and detailed login errors. Retain the additive migration and its history; do not narrow the platform constraint. Linux devices created after the upgrade are new-feature data whose full compatibility with the old service needs separate verification. Image rollback does not automatically restore the database dump over current data.

See package metadata in `coordinator-release.json` for local verification scope. These instructions and local tests do not mean production has been upgraded.
