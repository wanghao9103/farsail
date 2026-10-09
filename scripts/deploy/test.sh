#!/usr/bin/env bash
# Build-host regression only: creates disposable accounts in farsail-deploy-test.
set -euo pipefail
docker() { MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' command docker "$@"; }
openssl() { MSYS_NO_PATHCONV=1 command openssl "$@"; }
if [[ $(uname -s) == MINGW* ]]; then jq() { command jq -b "$@"; }; fi
# The disposable CA has no public CRL distribution point. Schannel still verifies
# its chain/SAN; only unavailable revocation metadata is tolerated in this test.
curl() {
  if [[ $(uname -s) == MINGW* ]]; then command curl --ssl-revoke-best-effort "$@";
  else command curl "$@"; fi
}
umask 077
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
export FARSAIL_STATE_DIR=${FARSAIL_STATE_DIR:-$root/.local/deploy-smoke}
state=$FARSAIL_STATE_DIR
native() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else printf '%s\n' "$1"; fi; }
manage() { bash "$root/scripts/deploy/farsail.sh" "$@"; }
dc() { docker compose --env-file "$(native "$state/compose.env")" -f "$(native "$root/deploy/production/compose.yaml")" -f "$(native "$root/deploy/production/test.override.yaml")" "$@"; }
[[ -d $state ]] || manage init 127.0.0.1 local@example.test test
[[ $(jq -r .mode "$state/settings.json") == test ]]
[[ $(sed -n 's/^COMPOSE_PROJECT_NAME=//p' "$state/compose.env") == farsail-deploy-test ]]
trap 'dc down' EXIT
mkdir -p "$state/letsencrypt/live/farsail"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$state/ca.key" -out "$state/ca.pem" -days 2 -subj '/CN=FarSail disposable test CA' -addext 'basicConstraints=critical,CA:TRUE' >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -keyout "$state/letsencrypt/live/farsail/privkey.pem" -out "$state/cert.csr" -subj '/CN=127.0.0.1' >/dev/null 2>&1
printf 'subjectAltName=IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' > "$state/extensions"
openssl x509 -req -in "$state/cert.csr" -CA "$state/ca.pem" -CAkey "$state/ca.key" -CAcreateserial -days 2 -extfile "$state/extensions" -out "$state/letsencrypt/live/farsail/fullchain.pem" >/dev/null 2>&1
openssl x509 -in "$state/ca.pem" -outform DER -out "$state/ca.der"
dc up -d --pull never gateway
manage activate-test-tls
manage start
curl --fail --silent --show-error --cacert "$state/ca.pem" https://127.0.0.1:58443/healthz
curl --fail --silent --show-error --cacert "$state/ca.pem" https://127.0.0.1:58444/healthz >/dev/null
code=$(curl --silent --cacert "$state/ca.pem" -o /dev/null -w '%{http_code}' -X POST https://127.0.0.1:58443/internal/relay-access)
[[ $code == 404 ]]
# Default OS trust must reject this temporary CA.
if curl --silent --fail https://127.0.0.1:58443/healthz >/dev/null 2>&1; then echo 'Unexpected default trust' >&2; exit 1; fi
api() {
  local path=$1 body=$2 token=${3:-}
  local header=()
  [[ -z $token ]] || header=(-H "Authorization: Bearer $token")
  curl --fail --silent --show-error --cacert "$state/ca.pem" -H 'Content-Type: application/json' "${header[@]}" --data "$body" "https://127.0.0.1:58443$path"
}
email="smoke-$(openssl rand -hex 8)@example.test"; password=$(openssl rand -hex 24)
api /v1/auth/register "$(jq -nc --arg email "$email" --arg password "$password" '{email:$email,password:$password}')" >/dev/null
mail_id=$(curl --fail --silent http://127.0.0.1:58026/api/v1/messages | jq -r --arg email "$email" '.messages[]|select(.To[0].Address==$email)|.ID' | head -1)
[[ -n $mail_id ]]
verify=$(curl --fail --silent "http://127.0.0.1:58026/api/v1/message/$mail_id" | jq -r '.Text' | sed -n 's/^验证码：//p' | tr -d '\r')
api /v1/auth/verify "$(jq -nc --arg email "$email" --arg token "$verify" '{email:$email,token:$token}')" >/dev/null
auth=$(api /v1/auth/login "$(jq -nc --arg email "$email" --arg password "$password" '{email:$email,password:$password}')" | jq -r .access_token)
keys=(); ids=()
for n in 1 2; do
  openssl genpkey -algorithm ed25519 -out "$state/device$n.key"
  public=$(openssl pkey -in "$state/device$n.key" -pubout -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n')
  private=$(openssl pkey -in "$state/device$n.key" -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n')
  keys+=("$private")
  challenge=$(api /v1/devices/challenge "$(jq -nc --arg public_key "$public" '{public_key:$public_key}')" "$auth")
  jq -jr .message <<< "$challenge" > "$state/challenge"
  signature=$(openssl pkeyutl -sign -inkey "$state/device$n.key" -rawin -in "$state/challenge" | od -An -v -tx1 | tr -d ' \n')
  body=$(jq -nc --arg challenge_id "$(jq -r .challenge_id <<< "$challenge")" --arg signature "$signature" --arg name "Deploy $n" '{challenge_id:$challenge_id,signature:$signature,name:$name,platform:"windows",can_host:false,can_files:false}')
  ids+=("$(api /v1/devices/bind "$body" "$auth" | jq -r .id)")
done
fixture="$state/probe.json"
probe() {
  jq -n --arg relay https://127.0.0.1:58444/ --arg ca_der "$(native "$state/ca.der")" --arg a "$1" --arg b "$2" --argjson deny "$3" '{relay:$relay,ca_der:$ca_der,keys:[$a,$b],deny:$deny}' > "$fixture"
  FARSAIL_DEPLOY_FIXTURE="$(native "$fixture")" cargo run --locked -p farsail-coordinator --example deploy_probe
}
probe "${keys[0]}" "${keys[1]}" false
probe "$(openssl rand -hex 32)" "$(openssl rand -hex 32)" true
manage bootstrap-admin "$email"
api "/v1/admin/devices/${ids[0]}/enabled" '{"enabled":false}' "$auth" >/dev/null
probe "${keys[0]}" "${keys[0]}" true
api "/v1/admin/devices/${ids[0]}/enabled" '{"enabled":true}' "$auth" >/dev/null
# Namespace recreation and database persistence must preserve real registrations.
manage start
probe "${keys[0]}" "${keys[1]}" false
# Certificate reload must actually change the served TLS leaf on API and relay.
old=$(openssl x509 -in "$state/tls/fullchain.pem" -noout -serial)
openssl x509 -req -in "$state/cert.csr" -CA "$state/ca.pem" -CAkey "$state/ca.key" -set_serial "0x$(openssl rand -hex 16)" -days 2 -extfile "$state/extensions" -out "$state/letsencrypt/live/farsail/fullchain.pem" >/dev/null 2>&1
manage activate-test-tls
dc restart relay
dc up -d --pull never --wait
for port in 58443 58444; do
  served=$(openssl s_client -connect "127.0.0.1:$port" -CAfile "$state/ca.pem" -verify_return_error </dev/null 2>/dev/null | openssl x509 -noout -serial)
  [[ $served != "$old" && $served == "$(openssl x509 -in "$state/tls/fullchain.pem" -noout -serial)" ]]
done
cp "$state/tls/fullchain.pem" "$state/valid-certificate.pem"
printf 'invalid certificate\n' > "$state/tls/fullchain.pem"
if dc exec -T gateway nginx -t >/dev/null 2>&1; then echo 'Invalid certificate unexpectedly accepted' >&2; exit 1; fi
cp "$state/valid-certificate.pem" "$state/tls/fullchain.pem"
dc stop relay
probe "${keys[0]}" "${keys[1]}" true
dc up -d --pull never --wait relay
dc stop coordinator
probe "${keys[0]}" "${keys[1]}" true
dc up -d --pull never --wait coordinator
manage backup
docker stats --no-stream $(dc ps -q) --format '{{.Name}} {{.MemUsage}}'
for service in gateway db; do
  docker inspect "$(dc ps -q "$service")" --format '{{json .NetworkSettings.Ports}}' | jq -e 'all(.[]?[]?; .HostIp=="127.0.0.1")' >/dev/null
done
dc run --rm --no-deps certbot --version
export FARSAIL_TEST_DATABASE_URL="postgres://farsail:$(sed -n 's/^POSTGRES_PASSWORD=//p' "$state/database.env")@127.0.0.1:55433/farsail"
cargo test --locked -p farsail-core -p farsail-coordinator
cargo clippy --locked -p farsail-coordinator --all-targets -- -D warnings
printf 'Deployment regression passed; test containers stopped, private state/volumes retained.\n'
