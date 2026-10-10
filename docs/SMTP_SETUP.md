**English** | [简体中文](SMTP_SETUP.zh-CN.md)

# Real email verification setup

FarSail supports SMTP delivery for email verification and password recovery. The default Mailpit inbox captures test messages without delivering to real mailboxes. This guide covers an existing Linux Docker Compose production deployment using a 126 sender account. Only the sender needs SMTP enabled.

## 1. Prepare the sender account

Open the sender's webmail settings, enable SMTP under POP3/SMTP/IMAP, and generate a client authorization code. Use the authorization code rather than the webmail password. Keep it in the private server configuration, outside Git, release attachments and chat screenshots.

- [NetEase: create a client authorization code](https://help.mail.126.com/faqDetail.do?code=d7a5dc8471cd0c0e8b4b8f4f8e49998b374173cfe9171305fa1ce630d7f67ac286624f309a1a7089)
- [NetEase: mail-client server settings](https://help.mail.126.com/faqDetail.do?code=d7a5dc8471cd0c0e8b4b8f4f8e49998b374173cfe9171305fa1ce630d7f67ac25c12dcb3d46222b6)

The example uses `smtp.126.com`, port 465 and implicit TLS. Follow the provider's official settings for other mailboxes. The project also supports STARTTLS, commonly on port 587. The server must resolve the SMTP hostname and reach its outbound port; no inbound mail port is required.

## 2. Automatically update existing configuration (recommended)

New deployment templates include SMTP user, authorization-code and TLS fields while retaining the test inbox as the default. Existing servers are not automatically changed by template updates. The standalone configuration script requires Python 3 and existing Docker Compose, reuses the deployment lock, backs up the private file and updates it atomically. Enter the sender account and authorization code interactively on the server; the code is hidden and is not a command-line argument.

Download configure-smtp.py using the URL and SHA256 on the server release page and verify it before running. Its source is scripts/deploy/configure-smtp.py; this standalone configuration tool requires no coordinator image rebuild or update.

```bash
python3 /tmp/configure-smtp.py --deploy-root /home/data/farsail --apply
```

The script asks for host, TLS mode, port, sender, SMTP user and authorization code, preserving existing database and relay settings. Missing base settings are recovered from existing private deployment configuration: database credentials from the database container's environment, relay credentials from the relay container's environment, and the relay URL from deployment settings and the gateway port. It does not generate new passwords, reset the database or overwrite existing base settings.

The error `FARSAIL_DATABASE_URL is required` means the current container has no database connection parameter. A partial snippet may have replaced the whole file, or a stale container may not have reloaded a corrected file. Preserve existing mail settings and restore only base parameters:

```bash
python3 /tmp/configure-smtp.py --deploy-root /home/data/farsail --repair-only --apply
```

Preserve `FARSAIL_STATE_DIR`. Only the coordinator is recreated. Failure restores the original file and attempts recreation with the original configuration; an already broken original file does not guarantee service recovery, so inspect logs. Omit --apply for file-only changes and apply them using the commands below. The script prints the backup location; configuration and backup permissions are 600. It does not print credentials or send test mail. Missing database or relay source configuration causes refusal rather than guessing credentials.

The alternative manual procedure follows.

## 3. Manually back up and edit configuration

The default checkout is `/home/data/farsail` and state directory is `.local/production`. Preserve any existing `FARSAIL_STATE_DIR` override; replace the checkout path if different.

```bash
cd /home/data/farsail
umask 077
smtp_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
smtp_backup="$smtp_state/coordinator.env.smtp-backup-$(date +%Y%m%d-%H%M%S)-$$"
cp "$smtp_state/coordinator.env" "$smtp_backup"
chmod 600 "$smtp_backup"
nano "$smtp_state/coordinator.env"
```

Edit only mail settings; do not replace the whole file with the following snippet. Preserve `FARSAIL_DATABASE_URL`, `FARSAIL_BIND`, `FARSAIL_RELAY_ACCESS_TOKEN` and `FARSAIL_RELAY_URLS`.

| Setting                 | Default test configuration                  | Real email action                                   |
| ----------------------- | ------------------------------------------- | --------------------------------------------------- |
| `FARSAIL_MAIL_MODE`     | `smtp-local`                                | Change to `smtp-tls`                                |
| `FARSAIL_SMTP_HOST`     | `127.0.0.1`                                 | Change to the sender's SMTP host                    |
| `FARSAIL_SMTP_PORT`     | 1025                                        | Change to 465 for implicit TLS                      |
| `FARSAIL_MAIL_FROM`     | Local test sender                           | Change to the authenticated sender address          |
| `FARSAIL_SMTP_USER`     | Missing in old templates, empty in new ones | Add or fill the full sender email address           |
| `FARSAIL_SMTP_PASSWORD` | Missing in old templates, empty in new ones | Add or fill the SMTP authorization code             |
| `FARSAIL_SMTP_TLS`      | Missing in old templates                    | Explicitly set `implicit`; new templates include it |

Replace the example address and authorization code below with your values. Each setting should occur only once.

```dotenv
FARSAIL_MAIL_MODE=smtp-tls
FARSAIL_SMTP_HOST=smtp.126.com
FARSAIL_SMTP_PORT=465
FARSAIL_SMTP_TLS=implicit
FARSAIL_SMTP_USER=sender@126.com
FARSAIL_SMTP_PASSWORD='REPLACE_WITH_SMTP_AUTHORIZATION_CODE'
FARSAIL_MAIL_FROM=FarSail <sender@126.com>
```

This is a Compose environment file, not a shell script to source. [Docker environment-file rules](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/) describe quoting and interpolation. Single quotes prevent Compose interpolation of dollar signs in authorization codes; escape any quotes according to Compose environment-file rules. For STARTTLS, set `FARSAIL_SMTP_TLS=starttls` and the corresponding port. Keep TLS certificate validation enabled.

## 4. Apply the settings

Save the file and run as a user with Docker and deployment-file access:

```bash
cd /home/data/farsail
smtp_state=${FARSAIL_STATE_DIR:-"$PWD/.local/production"}
chmod 600 "$smtp_state/coordinator.env"
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml config --quiet
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml up -d --pull never \
  --no-deps --force-recreate --wait --wait-timeout 180 coordinator
```

[Docker documentation](https://docs.docker.com/reference/cli/docker/compose/restart/) confirms restart does not apply configuration changes; recreate the coordinator. These commands update only the coordinator and briefly interrupt the API. Database, gateway and relay containers are not recreated. No reinitialization or certificate issuance is needed.

Once real SMTP is configured, remove `test-mail` from `COMPOSE_PROFILES` in the private `compose.env`, keeping any other required profiles. If it is the only profile, use an empty value. Stop the running Mailpit container with the following command, preserving its volume:

```bash
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml --profile test-mail stop mailpit
```

## 5. Verify delivery

1. Register an email account whose inbox you can access. For an existing unverified account, resend verification with its correct account password.
2. Check the inbox and spam folder. Updated servers send a formal Chinese message with a prominent six-digit verification code, matching plain-text and HTML content, and instructions.
3. Enter the six-digit code on the client’s email-verification page and log in. Codes last 10 minutes and are single-use. Resend directly on this page; use the newest message because previous codes are invalidated. The client retains registration credentials only in memory for this request.

Six-digit verification requires the updated coordinator and client 0.1.19 or later, which includes the email in verification requests. Already-issued 43-character legacy codes remain accepted until their original expiry. Existing SMTP settings do not need to change; install the new server image and client for the new format. Recovery codes keep their existing entropy and 30-minute expiry, with a formal Chinese email template.

Registration may commit the account before mail delivery fails. If registering again reports an existing account, resend verification instead of repeatedly registering. A successful resend response does not guarantee a message was sent: the account must be enabled and unverified, and its password must be correct.

## 6. Troubleshoot missing messages

```bash
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml ps coordinator
docker compose --env-file "$smtp_state/compose.env" \
  -f deploy/production/compose.yaml logs --tail 100 coordinator
```

| Symptom                              | Check                                                                               |
| ------------------------------------ | ----------------------------------------------------------------------------------- |
| Missing variables or startup failure | Add the SMTP user and authorization code, save the file, and recreate the container |
| SMTP authentication failure          | Enabled service, valid authorization code and full email username                   |
| Connection timeout or DNS failure    | Server DNS, cloud outbound rules, host firewall and provider network restrictions   |
| TLS handshake failure                | Matching hostname, port and TLS mode; correct system clock and CA certificates      |
| Sender rejection                     | Sender address matches the authenticated account and has send permission            |
| Successful request without delivery  | Spam folder, provider delivery records and sending limits; valid resend conditions  |

Redact passwords, tokens and database credentials when sharing diagnostics, and avoid printing the complete Compose configuration. SMTP acceptance does not prove inbox delivery; verify real receipt and successful email verification.

## Verification scope and related documentation

On 2026-10-09 this guide was checked against coordinator SMTP parameters, message content, account verification and Compose configuration generation. Real sender credentials, production SMTP connectivity and inbox delivery require server acceptance checks. Mail delivery and resend logs record stages and skip reasons without credentials or codes. SMTP acceptance is distinct from inbox delivery. Documentation does not imply the server is configured or mail has been sent.

- [Deployment guide](DEPLOYMENT.md)
- [Server upgrade for Ubuntu binding and login feedback](UPGRADE_UBUNTU_LOGIN.md)
