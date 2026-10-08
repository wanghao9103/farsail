**English** | [简体中文](DEPLOYMENT.zh-CN.md)

<a id="公网-ip-联调部署ubuntu-2404--amd64"></a>

# Public-IP integration deployment: Ubuntu 24.04 / amd64

The server imports prebuilt images only and does not compile Rust. The release bundle includes six images: coordinator, pinned iroh-relay 1.2.0, Nginx, PostgreSQL 17, Mailpit and Certbot 5.4.0. The current client uses low-frame-rate JPEG with payload bandwidth limits and frame deadlines; files, HEVC and mobile clients have not been delivered. Public certificate issuance, the user's server and a second computer still require independent acceptance testing; see the scope in [WI-008A](verification/WI-008A.md).

<a id="初始准备"></a>

## Initial preparation

Install official Docker Engine/Compose yourself. Prepare root access and at least 4 GiB of free disk space. 1.6 GiB of memory is suitable for light integration testing, with no concurrency-capacity promise. Resident memory limits: coordinator 384 MiB, relay 256 MiB, database 256 MiB, Nginx 96 MiB and Mailpit 128 MiB. The system/Docker/temporary Certbot also need headroom. Logs are limited to 3×10 MB per container.

Execute each code block below separately and replace angle-bracket placeholders. The scripts use Bash/jq/openssl/standard utilities, without a project-authored Python dependency.

```sh
apt-get update
```

```sh
apt-get install -y git curl jq openssl ca-certificates util-linux iproute2
```

Run this if not yet cloned (skip if the directory already exists):

```sh
git clone https://github.com/wanghao9103/farsail.git /home/data/farsail
```

```sh
cd /home/data/farsail
```

Obtain the complete pinned commit from the verification record; do not use latest:

```sh
DEPLOY_SHA='51e131f0bf60a3a8fa15cddd869a8b97d3db79a0'
```

Runtime images use the original verified 4252 archive. The loader is a newer commit that fixes only verification across Docker storage implementations; the two are explicitly separate:

```sh
RELEASE_SHA='4252e9d901e3022175772f563be45df967c3e9cc'
```

```sh
git fetch origin
```

```sh
git checkout --detach "$DEPLOY_SHA"
```

```sh
bash scripts/deploy/farsail.sh init '<公网IPv4>' '<ACME联系邮箱>'
```

Random database passwords and admission Bearer tokens are written to the ignored `.local/production`, without printing them or overwriting existing state. The root directory has mode 700 and env files 600; the mounted private key uses 640/group 10001 for the non-root relay to read. API and relay run as user 10001. Initial Mailpit test mode still requires ordinary email verification. Do not expose this directory, complete Docker inspect environments or database backups.

Users who already completed init on version 4252 must skip init and retain the entire state. Wait for the old load process to exit, then fetch/checkout the loader above. Loading the same 4252 release again reuses complete cached files. Failure of the old ID check does not commit new runtime configuration. The new loader permits cross-source-ref compatibility only for this audited manifest/archive SHA combination; it does not ignore version or content checks.

```sh
bash scripts/deploy/farsail.sh preflight
```

Allow TCP 80/443/8443 in the security group. Do not expose 5432/8787/8788/8025/1025. The public IP is used only in URLs/certificates; local listening on 0.0.0.0 supports EIP/NAT mappings. The coordinator still listens only on 127.0.0.1 within the shared namespace, without changing global Docker networking/proxies.

<a id="六镜像离线包"></a>

## Six-image offline bundle

On Alibaba Cloud, choose an ECS security group or a Simple Application Server firewall according to the actual product. Initialization checks dependencies before creating state. If a disk/permission error interrupts it, retain and rename the partly initialized directory that has not yet been put into use before retrying; do not overwrite secrets already in use. Linux mutations share operation.lock next to state to prevent concurrent renewal, import and upgrade. Apart from the exact 4252 compatibility exception above, load-release requires the current code HEAD to match the artifact revision.

The pinned release page contains `farsail-linux-amd64-images.tar.gz`, `manifest.json` and `SHA256SUMS`. The script verifies attachment SHA256, revision, all six image IDs and linux/amd64 before updating Compose. Dependency sources are pinned by registry digest. The archive uses full image-ID-derived tags because save/load does not retain RepoDigest. All services use pull_policy=never, and startup explicitly uses --pull never.

Image content identity is determined by `config_digest` in the manifest; the configuration SHA also binds rootfs diffIDs. Docker inspect .Id under classic/containerd storage may represent a config or manifest respectively, so it cannot be compared directly across machines. After loading, Docker metadata is streamed through a fresh export to validate configuration SHA, still without accessing a registry or saving a large temporary tar. The manifest id retains the build engine's observed value for traceability.

```sh
bash scripts/deploy/farsail.sh load-release "$RELEASE_SHA"
```

Successfully cloning code does not mean GitHub attachments can be downloaded. On attachment failure, check DNS, outbound 443 and the curl result; retries can reuse downloaded files. Move aside the corresponding cache file if validation fails. If the server cannot download them, download the same release's three files on your own computer, transfer them with your own SSH/scp to `/home/data/farsail-release`, then run:

```sh
bash scripts/deploy/farsail.sh load-release "$RELEASE_SHA" /home/data/farsail-release
```

This path does not require DockerHub access. Do not substitute an image mirror of unknown provenance. SHA256 ensures integrity; the trust root remains this project's GitHub release identity. `.local/production/manifest.json` is the checked manifest. The download directory can be retained for rollback.

<a id="证书和首次启动"></a>

## Certificates and first startup

Certbot 5.4+ webroot supports `--ip-address` with `--preferred-profile shortlived`. IP certificates last about 160 hours. The current nginx/apache installers do not support IPs, so deployment/renewal loading must be handled separately. [Official Let’s Encrypt explanation](https://letsencrypt.org/2026/03/11/shorter-certs-certbot).

Rehearse with staging first (an isolated directory, which does not replace production certificates and is not trusted by the system):

```sh
bash scripts/deploy/farsail.sh certificate staging
```

Then issue the production certificate, load it and start:

```sh
bash scripts/deploy/farsail.sh certificate
```

Initial HTTP serves only the ACME directory. The production certificate is first reloaded into nginx in place, then application services start. Resolve certificate/routing errors; do not use -k or skip verification.

```sh
bash scripts/deploy/farsail.sh install-timer
```

```sh
systemctl start farsail-renew.service
```

```sh
systemctl status farsail-renew.timer farsail-renew.service --no-pager
```

Checks run twice daily, with up to 30 minutes of random delay and catch-up for missed jobs. The deploy hook leaves a pending-load marker; it is removed only after certificates are copied, nginx validation/reload succeeds, and the relay restarts and becomes healthy. Failure retains the marker and systemd failure state. Inspect `journalctl -u farsail-renew.service` and the private `renew-last-success.txt`. Integrate your own operations notifications; this bundle has no external alert-recipient account. It explicitly uses Manual + relay restart, so renewal disconnects relayed sessions and the client needs fresh approval. Uninterrupted hot reload is not claimed.

```sh
curl --fail --show-error 'https://<公网IPv4>/healthz'
```

```sh
bash scripts/deploy/farsail.sh status
```

<a id="邮箱管理员和双端配置"></a>

## Email, administrator and both client configurations

SMTP is at 127.0.0.1:1025 in the shared namespace; the inbox is published only to host 127.0.0.1:8025. Open another terminal on your computer and keep SSH forwarding running:

```sh
ssh -N -L 18025:127.0.0.1:8025 root@<公网IPv4>
```

On the computer, open `http://127.0.0.1:18025`. Set the client's API to `https://<公网IPv4>`, relay to `https://<公网IPv4>:8443/`, and UDP binding to `0.0.0.0:0`. For the first run, enable forced relay. Register → retrieve the token from the inbox → verify in the client → log in → bind the device. Mailpit does not deliver to real mailboxes. All test email is visible to the current operator; configure real SMTP and clean up test accounts before opening registration publicly.

```sh
bash scripts/deploy/farsail.sh bootstrap-admin '<已验证且启用的邮箱>'
```

The first registrant is not automatically elevated. The administrator page can change registration to invitation-only/closed. The host must still enable sharing locally and approve each request. Check both verification codes, actual selected path=relay, and stop/revocation behavior. Configuring a relay URL does not mean the actual path uses relay. Public discovery/automatic port mapping are disabled by default; cross-NAT direct connections need separate validation. Advertised bandwidth does not equal measured throughput or image quality.

Real SMTP: edit the private coordinator.env and set `FARSAIL_MAIL_MODE=smtp-tls`, HOST/USER/PASSWORD/MAIL_FROM (full names FARSAIL_SMTP_HOST, FARSAIL_SMTP_USER, FARSAIL_SMTP_PASSWORD, FARSAIL_MAIL_FROM), `FARSAIL_SMTP_TLS=implicit` with PORT=465 or `starttls` with PORT=587 (full PORT name FARSAIL_SMTP_PORT). TLS is mandatory, without downgrade. Follow Compose env-file escaping syntax. Clear COMPOSE_PROFILES in compose.env, stop first, then start; Mailpit will no longer start. Real SMTP was not verified in this run.

<a id="备份升级回滚"></a>

## Backup, upgrade and rollback

```sh
bash scripts/deploy/farsail.sh backup
```

The private backups directory receives a pg_dump custom-format file. Separately encrypt and back up the entire `.local/production` (including certificates, secrets and manifest) and the code ref. Backups include account/email data and must not be uploaded publicly.

Back up before upgrading, then check out the new pinned SHA, load-release the new SHA and run:

```sh
bash scripts/deploy/farsail.sh start
```

Start always force-recreates gateway/coordinator/relay/Mailpit together to prevent dependent services from retaining the old network namespace. There is a brief interruption; volumes are retained. Recreating gateway alone is prohibited. For rollback, first check out the old code and import the old archive, then start. This work item does not change database migrations; future irreversible migrations require evaluation of backup compatibility.

Database restore: stop first, start db alone with Compose using the same state, pass the selected dump through standard input to `pg_restore -U farsail -d farsail --clean --if-exists --exit-on-error`, then start. --clean overwrites the application database, so the operator must confirm the backup/downtime before executing it. Do not run development test scripts on the public server. A full disaster-recovery restore drill has not been verified.

Before restoring, stop both the timer and any running renewal to prevent application services restarting during restoration:

```sh
systemctl stop farsail-renew.timer farsail-renew.service
```

```sh
bash scripts/deploy/farsail.sh stop
```

```sh
docker compose --env-file .local/production/compose.env -f deploy/production/compose.yaml up -d --pull never --wait db
```

After confirming the selected backup, the single command below holds the instance operation lock and overwrites current database contents:

```sh
flock .local/production.operation.lock docker compose --env-file .local/production/compose.env -f deploy/production/compose.yaml exec -T db pg_restore -U farsail -d farsail --clean --if-exists --exit-on-error < '<选定的.dump完整路径>'
```

```sh
bash scripts/deploy/farsail.sh start
```

```sh
systemctl start farsail-renew.timer
```

```sh
bash scripts/deploy/farsail.sh logs
```

```sh
bash scripts/deploy/farsail.sh stop
```

Stop removes only this project's containers/network and retains data volumes. Diagnose failures by checking Compose health, nginx configuration, the database, admission Bearer token, IP SAN/full chain, clock and ACME port-80 routing. Admission validates new relay connections only; existing application streams use the 30-second grant cutoff. For rate limits and unimplemented connection limits, see the [relay instructions](../deploy/relay/README.md).

<a id="开发者测试"></a>

## Developer tests

Windows `scripts/test-deploy.ps1 -Build` requires Git Bash/openssl/jq. On Linux, build the two local images, then run `bash scripts/deploy/test.sh`. It operates only on farsail-deploy-test, using loopback ports 58080/58443/58444/58026/55433. Tests cover normal temporary-CA validation, real registration/email/public-key proof, bidirectional relay data and selected path, rejection of unknown/disabled identities/service downtime, namespace recreation and persistence, and backend tests/Clippy. Containers stop after the test; private state and volumes remain. It does not log in to the user's server.
