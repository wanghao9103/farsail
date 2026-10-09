**English** | [简体中文](EMAIL_VERIFICATION_UPDATE.zh-CN.md)

# Email verification code and formal mail update

This update adds formal Chinese verification and recovery messages with matching plain-text and HTML bodies, six-digit email verification codes, and resend directly on the verification page. Mail explains its purpose, 10-minute expiry, single use and use of the newest message. Recovery codes retain their existing entropy and 30-minute expiry.

Verification codes are bound to the normalized email and stored only as digests. Each email allows five attempts in 15 minutes. Resending invalidates previous codes; existing 43-character legacy verification tokens remain accepted until their original expiry. Session, device and relay credentials retain their entropy. Logs record sending stages and skip reasons without codes, passwords or recipients.

## 1. Install the client first

Numeric verification requires an email in the request. Install client 0.1.19 or later before upgrading the coordinator. The client accepts legacy tokens from old servers. Resend uses registration email and password retained in memory, without showing credential fields or navigating away. If restarting loses this context, fill existing account credentials on the login page before returning to verification.

The local package targets Ubuntu 26.04 amd64; other platforms were not built. Place the provided package in the current directory and run:

```bash
sudo apt install ./FarSail_0.1.19_ubuntu26.04_amd64.deb
```

Client in-app updating is deferred. Public server attachments do not include the client installer.

## 2. Download and upgrade the server from GitHub

The [server release page](https://github.com/wanghao9103/farsail/releases/tag/coordinator-ubuntu-login-20261009) gains these attachments. Earlier Ubuntu/login package bytes are preserved.

- `FarSail_coordinator_verification-code_20261009.tar.gz`
- `FarSail_coordinator_verification-code_20261009.tar.gz.sha256`

The deployment directory defaults to /home/data/farsail; change the final argument if needed. Preserve existing FARSAIL_STATE_DIR. Database and SMTP settings do not need rewriting. The command checks outer and inner SHA256, saves a private database backup, then recreates only the coordinator. Failed startup restores the previous image; the backup path is printed. Existing remote connections may be interrupted; reconnect after the update. Rust and compilation are unnecessary on the server. Do not repeat an update once this image is selected.

```bash
set -euo pipefail
mail_update_dir=$(mktemp -d /tmp/farsail-mail-update.XXXXXX)
curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
  --retry 3 --connect-timeout 20 --output "$mail_update_dir/package.tar.gz" \
  "https://github.com/wanghao9103/farsail/releases/download/coordinator-ubuntu-login-20261009/FarSail_coordinator_verification-code_20261009.tar.gz"
printf '%s  %s\n' '4500264bfb0c47437fe9dacb3f2af9f994f91a03a56c38121d7bd6394df53171' "$mail_update_dir/package.tar.gz" | sha256sum -c -
mkdir "$mail_update_dir/release"
tar -xzf "$mail_update_dir/package.tar.gz" -C "$mail_update_dir/release"
(cd "$mail_update_dir/release" && sha256sum -c SHA256SUMS)
bash "$mail_update_dir/release/upgrade-coordinator.sh" "$mail_update_dir/release" /home/data/farsail
```

## 3. Acceptance checks

Resend directly for an unverified account and confirm the client remains on verification. Check inbox and spam for the formal Chinese verification message, submit its newest six-digit code and log in. Old codes after resend, expired codes and replay must fail. Wait 15 minutes after reaching the attempt limit. Existing SMTP settings remain usable.

```bash
cd /home/data/farsail
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file .local/production/compose.env \
  -f deploy/production/compose.yaml logs --tail 80 coordinator
```

Replace the environment-file path for custom state directories. mail_delivery / smtp_accepted means SMTP accepted the mail, not inbox delivery. verification_resend with skipped means sending conditions were unmet. Do not share passwords, codes or full configuration.

## 4. Verification scope

Rust 1.93 and PostgreSQL 17 checks passed for email binding, expiry, replay, concurrent verification/resend, limits and legacy compatibility. Existing account, device and remote-authorization regressions passed. Synthetic WebKit tests confirm successful and failed resend retain the verification page and input without credential fields. The actual new image passed local SMTP receipt, Chinese MIME, rejected-send error, retry and recovery-mail checks. A disposable Compose deployment passed old-image upgrade, retained users/database container, private backup and failed-start rollback. Ubuntu client packaging passed.

Production installation and real-provider delivery of the new template were not performed. User screenshots prove old messages reached spam, not the new template's destination. Formatting cannot guarantee inbox placement. Public redownload is constrained by local network; verify uploaded asset state and digest after publication.

The image uses an uncommitted working tree; see coordinator-release.json for its baseline and verification scope. GitHub's automatic Source code archives contain the old baseline; deploy using the named attachments. No new code-table migration is added. The earlier Linux platform migration is included and retained on rollback.

- [Real SMTP configuration](SMTP_SETUP.md)
- [API](API.md)
