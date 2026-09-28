#!/usr/bin/env bash
# Ubuntu 24.04: bash, curl, jq, openssl, coreutils, util-linux, Docker + Compose.
set -euo pipefail
docker() { MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' command docker "$@"; }
openssl() { MSYS_NO_PATHCONV=1 command openssl "$@"; }
if [[ $(uname -s) == MINGW* ]]; then jq() { command jq -b "$@"; }; fi
umask 077
root=$(cd "$(dirname "$0")/../.." && pwd)
config="$root/deploy/production"
state=${FARSAIL_STATE_DIR:-$root/.local/production}
state=$(realpath -m "$state")
action=${1:-help}; shift || true
native() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else printf '%s\n' "$1"; fi; }
get() { sed -n "s/^$1=//p" "$state/compose.env"; }
dc() {
  local extra=()
  if [[ -f $state/settings.json ]] && [[ $(jq -r .mode "$state/settings.json") == test ]]; then extra=(-f "$(native "$config/test.override.yaml")"); fi
  docker compose --env-file "$(native "$state/compose.env")" -f "$(native "$config/compose.yaml")" "${extra[@]}" "$@"
}
fail() { printf '%s\n' "$*" >&2; exit 1; }
# Check tools before initialization can create a partial state directory.
for tool in docker jq curl openssl realpath sha256sum; do type -P "$tool" >/dev/null || fail "Missing dependency: $tool; install documented prerequisites first"; done
case "$action" in
  init|load-release|certificate|activate-test-tls|start|stop|bootstrap-admin|backup|renew|install-timer)
    if [[ $(uname -s) == Linux ]]; then
      command -v flock >/dev/null || fail 'Install util-linux (flock) first'
      mkdir -p "$(dirname "$state")"
      exec 9> "${state}.operation.lock"
      flock -n 9 || fail 'Another deployment operation is active; retry after it completes'
    fi
    ;;
esac
activate() {
  for name in fullchain.pem privkey.pem; do
    cp "$state/letsencrypt/live/farsail/$name" "$state/tls/$name.new"
    chmod 644 "$state/tls/$name.new"
    if [[ $name == privkey.pem ]]; then
      chmod 640 "$state/tls/$name.new"
      if [[ $(uname -s) == Linux ]]; then
        if [[ $EUID == 0 ]]; then chown 0:10001 "$state/tls/$name.new";
        elif [[ $(jq -r .mode "$state/settings.json") == test ]]; then
          # Disposable test key only, inside a private 0700 host directory.
          chmod 644 "$state/tls/$name.new"
        else fail 'Production certificate activation requires root'; fi
      fi
    fi
    mv "$state/tls/$name.new" "$state/tls/$name"
  done
  cp "$config/nginx-tls.conf" "$state/nginx/tls.conf"
  chmod 644 "$state/nginx/tls.conf"
  dc exec -T gateway nginx -t
  dc exec -T gateway nginx -s reload
}
case "$action" in
init)
  ip=${1:?public IPv4 required}; email=${2:?ACME email required}; mode=${3:-production}
  [[ $ip =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || fail 'IPv4 required'
  IFS=. read -r a b c d <<< "$ip"
  for octet in "$a" "$b" "$c" "$d"; do ((10#$octet <= 255)) || fail 'invalid IPv4'; done
  [[ $email =~ ^[^[:space:]@]+@[^[:space:]@]+$ ]] || fail 'invalid email'
  [[ ! -e $state ]] || fail 'State already exists; refusing to overwrite secrets'
  [[ $mode == production || $mode == test ]] || fail 'mode must be production or test'
  [[ $mode != test || $ip == 127.0.0.1 ]] || fail 'tests must use loopback'
  trap 'printf "Initialization interrupted; preserve/rename the partial state directory before retrying: %s\n" "$state" >&2' ERR
  mkdir -p "$state"/{nginx,tls,letsencrypt,webroot}
  chmod 700 "$state"; chmod 755 "$state"/{nginx,tls,webroot}
  cp "$config/nginx-http.conf" "$state/nginx/http.conf"; chmod 644 "$state/nginx/http.conf"
  password=$(openssl rand -hex 32); access=$(openssl rand -hex 32)
  printf 'POSTGRES_USER=farsail\nPOSTGRES_DB=farsail\nPOSTGRES_PASSWORD=%s\n' "$password" > "$state/database.env"
  http=80; api=443; relay=8443; mail=8025; bind=0.0.0.0; project=farsail-prod
  if [[ $mode == test ]]; then http=58080; api=58443; relay=58444; mail=58026; bind=127.0.0.1; project=farsail-deploy-test; fi
  cat > "$state/coordinator.env" <<EOF
FARSAIL_DATABASE_URL=postgres://farsail:$password@db:5432/farsail
FARSAIL_BIND=127.0.0.1:8787
FARSAIL_MAIL_MODE=smtp-local
FARSAIL_SMTP_HOST=127.0.0.1
FARSAIL_SMTP_PORT=1025
FARSAIL_MAIL_FROM=FarSail <noreply@localhost>
FARSAIL_RELAY_ACCESS_TOKEN=$access
FARSAIL_RELAY_URLS=https://$ip:$relay/
EOF
  printf 'IROH_RELAY_HTTP_BEARER_TOKEN=%s\n' "$access" > "$state/relay.env"
  cat > "$state/compose.env" <<EOF
STATE_DIR=$(native "$state")
COMPOSE_PROJECT_NAME=$project
COMPOSE_PROFILES=test-mail
PUBLIC_BIND=$bind
HTTP_PORT=$http
API_PORT=$api
RELAY_PORT=$relay
MAILPIT_PORT=$mail
COORDINATOR_IMAGE=farsail/coordinator:wi008a-local
RELAY_IMAGE=farsail/relay:wi008a-local
EOF
  jq -r 'to_entries[] | "\(.key)=\(.value)"' "$config/images.json" >> "$state/compose.env"
  jq -n --arg ip "$ip" --arg email "$email" --arg mode "$mode" '{ip:$ip,email:$email,mode:$mode}' > "$state/settings.json"
  printf 'Initialized private configuration with loopback test Mailpit. No secrets printed.\n'
  trap - ERR
  ;;
preflight)
  command -v jq >/dev/null; command -v curl >/dev/null; command -v openssl >/dev/null
  docker version --format '{{.Server.Version}}'; docker compose version
  dc config --quiet
  if [[ $(uname -s) == Linux ]]; then
    [[ $(uname -m) == x86_64 ]] || fail 'Linux amd64 required'
    for port in "$(get HTTP_PORT)" "$(get API_PORT)" "$(get RELAY_PORT)" "$(get MAILPIT_PORT)"; do
      [[ -z $(ss -H -lnt "sport = :$port") ]] || fail "Port $port occupied; inspect ss -lntp / docker ps"
    done
    (( $(df --output=avail -B1 "$state" | tail -1) >= 4294967296 )) || fail 'At least 4 GiB free required'
  fi
  printf 'Preflight passed. Check cloud security-group TCP 80/443/8443 separately.\n'
  ;;
load-release)
  revision=${1:?full commit SHA required}; source_dir=${2:-}
  [[ $revision =~ ^[0-9a-f]{40}$ ]] || fail 'Use the full 40-character revision'
  dest=${source_dir:-$state/downloads/$revision}; mkdir -p "$dest"; dest=$(realpath "$dest")
  for name in SHA256SUMS manifest.json farsail-linux-amd64-images.tar.gz; do
    if [[ ! -f $dest/$name ]]; then
      [[ -z $source_dir ]] || fail "Offline file missing: $name"
      curl --fail --location --retry 3 --connect-timeout 20 --output "$dest/$name.part" "https://github.com/wanghao9103/farsail/releases/download/deploy-$revision/$name"
      mv "$dest/$name.part" "$dest/$name"
    fi
  done
  # Validate exact expected filenames; never allow checksum files to reference other paths.
  for name in manifest.json farsail-linux-amd64-images.tar.gz; do
    expected=$(awk -v name="$name" '{sub(/^\*/,"",$2)} $2==name {print $1}' "$dest/SHA256SUMS")
    [[ $expected =~ ^[0-9a-f]{64}$ ]] || fail "Missing/invalid checksum: $name"
    actual=$(sha256sum "$dest/$name" | cut -d' ' -f1)
    [[ $actual == "$expected" ]] || fail "Checksum mismatch: $name"
  done
  if [[ $(git -C "$root" rev-parse HEAD) != "$revision" ]]; then
    # Narrow, audited loader-only compatibility upgrade. Runtime service/config
    # behavior is unchanged; preserve the existing state and cached archive.
    [[ $revision == 4252e9d901e3022175772f563be45df967c3e9cc &&
       $(sha256sum "$dest/manifest.json" | cut -d' ' -f1) == dca1d2632104fcfcf6ddcff5c3091b6bc92e2c8a814fd087064ed6df51ed4068 &&
       $(sha256sum "$dest/farsail-linux-amd64-images.tar.gz" | cut -d' ' -f1) == 23684d126f244ac7d3eae86438308dd099be0020b16cee60cefeda1360f689cd ]] || fail 'Checkout the same fixed revision before loading this release'
  fi
  jq -e --arg revision "$revision" '.revision==$revision and .platform=="linux/amd64" and (.images|length)==6' "$dest/manifest.json" >/dev/null
  docker load --input "$(native "$dest/farsail-linux-amd64-images.tar.gz")"
  mapfile -t tags < <(jq -r '.images[].tag' "$dest/manifest.json")
  for tag in "${tags[@]}"; do
    [[ $tag =~ ^farsail/[a-z-]+:[a-z0-9-]+$ ]] || fail 'Invalid image tag'
  done
  # Stream re-export metadata: no registry calls, no large temporary tar file.
  # Config hashes also commit to rootfs layer diffIDs and runtime configuration.
  docker save "${tags[@]}" | tar -xOf - manifest.json > "$state/loaded-image-configs.json"
  cp "$state/compose.env" "$state/compose.env.previous"
  cp "$state/compose.env" "$state/compose.env.new"
  while IFS=$'\t' read -r key tag id config_digest; do
    [[ $key =~ ^(COORDINATOR|RELAY|NGINX|POSTGRES|MAILPIT|CERTBOT)_IMAGE$ ]] || fail 'Unexpected image key'
    [[ $tag =~ ^farsail/[a-z-]+:[a-z0-9-]+$ && $id =~ ^sha256:[0-9a-f]{64}$ ]] || fail 'Invalid image metadata'
    [[ $config_digest =~ ^sha256:[0-9a-f]{64}$ ]] || fail 'Invalid image config digest'
    [[ $(docker image inspect "$tag" --format '{{.Os}}/{{.Architecture}}') == linux/amd64 ]] || fail "Platform mismatch: $key"
    loaded_config=$(jq -r --arg tag "$tag" '.[]|select(.RepoTags|index($tag))|.Config' "$state/loaded-image-configs.json")
    [[ "sha256:$(basename "$loaded_config" .json)" == "$config_digest" ]] || fail "Image config digest mismatch: $key"
    sed -i "/^$key=/d" "$state/compose.env.new"
    printf '%s=%s\n' "$key" "$tag" >> "$state/compose.env.new"
  done < <(jq -r '.images|to_entries[]|[.key,.value.tag,.value.id,(.value.config_digest // .value.id)]|@tsv' "$dest/manifest.json")
  mv "$state/compose.env.new" "$state/compose.env"
  cp "$dest/manifest.json" "$state/manifest.json"
  printf 'Verified all six portable image config digests, platforms and archive SHA256. Services have not been changed.\n'
  ;;
certificate)
  [[ $(jq -r .mode "$state/settings.json") == production ]] || fail 'Public CA forbidden in tests'
  dc up -d --pull never gateway
  extra=()
  if [[ ${1:-} == staging ]]; then extra=(--staging --config-dir /etc/letsencrypt/staging --deploy-hook true); fi
  dc run --rm --no-deps certbot certonly --non-interactive --agree-tos --email "$(jq -r .email "$state/settings.json")" --preferred-profile shortlived --webroot -w /var/www/acme --ip-address "$(jq -r .ip "$state/settings.json")" --cert-name farsail --deploy-hook 'touch /etc/letsencrypt/reload-required' "${extra[@]}"
  if [[ ${1:-} != staging ]]; then
    activate
    dc up -d --pull never --wait --wait-timeout 180
    dc restart relay
    rm -f "$state/letsencrypt/reload-required"
  fi
  ;;
activate-test-tls)
  [[ $(jq -r .mode "$state/settings.json") == test ]] || fail 'Only for test instance'
  activate
  ;;
start)
  [[ -f $state/tls/fullchain.pem ]] || fail 'Obtain certificate first'
  # A recreated gateway requires every namespace-sharing service to be recreated.
  dc up -d --pull never --force-recreate --wait --wait-timeout 180
  ;;
stop) dc down ;; # Never delete data volumes.
status) dc ps ;;
logs) dc logs --tail 100 gateway coordinator relay ;;
bootstrap-admin) dc exec -T coordinator farsail-coordinator bootstrap-admin "${1:?verified email required}" ;;
backup)
  mkdir -p "$state/backups"; chmod 700 "$state/backups"
  file="$state/backups/$(date -u +%Y%m%dT%H%M%SZ).dump"
  dc exec -T db pg_dump -U farsail -d farsail -Fc > "$file"
  chmod 600 "$file"; printf 'Private backup: %s\n' "$file"
  ;;
renew)
  [[ $(uname -s) == Linux ]] || fail 'Renew runs on Linux'
  dc run --rm --no-deps certbot renew --non-interactive --deploy-hook 'touch /etc/letsencrypt/reload-required'
  if [[ -f $state/letsencrypt/reload-required ]]; then
    activate; dc restart relay
    dc up -d --pull never --wait --wait-timeout 180
    rm -f "$state/letsencrypt/reload-required"
  fi
  date -u +%FT%TZ > "$state/renew-last-success.txt"
  ;;
install-timer)
  [[ $(uname -s) == Linux && $EUID == 0 ]] || fail 'Requires Linux root'
  [[ $(jq -r .mode "$state/settings.json") == production ]] || fail 'Not for tests'
  [[ $root != *'"'* && $state != *'"'* && $root != *'%'* && $state != *'%'* ]] || fail 'Unsupported path for systemd unit'
  cat > /etc/systemd/system/farsail-renew.service <<EOF
[Unit]
Description=Renew FarSail short-lived IP certificate
After=docker.service
Requires=docker.service
[Service]
Type=oneshot
Environment="FARSAIL_STATE_DIR=$state"
ExecStart=/bin/bash "$root/scripts/deploy/farsail.sh" renew
TimeoutStartSec=15min
EOF
  cat > /etc/systemd/system/farsail-renew.timer <<'EOF'
[Unit]
Description=Check FarSail certificate twice daily
[Timer]
OnCalendar=*-*-* 00,12:00:00
RandomizedDelaySec=30m
Persistent=true
[Install]
WantedBy=timers.target
EOF
  chmod 644 /etc/systemd/system/farsail-renew.{service,timer}
  systemctl daemon-reload; systemctl enable --now farsail-renew.timer
  ;;
*) fail 'Usage: farsail.sh init IP EMAIL [test] | preflight | load-release SHA [DIR] | certificate [staging] | start | stop | status | logs | bootstrap-admin EMAIL | backup | renew | install-timer' ;;
esac
