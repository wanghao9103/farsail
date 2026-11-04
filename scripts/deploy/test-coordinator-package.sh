#!/usr/bin/env bash
# Disposable CI deployment: verifies real backup/replacement and installed updater.
set -Eeuo pipefail
release=$(realpath "${1:?Extracted release directory required}")
root=$(pwd)
export FARSAIL_STATE_DIR="$root/.local/coordinator-package-smoke"
state=$FARSAIL_STATE_DIR
[[ ! -e $state ]]
bash scripts/deploy/farsail.sh init 127.0.0.1 ci@example.test test
dc() { docker compose --env-file "$state/compose.env" -f "$root/deploy/production/compose.yaml" -f "$root/deploy/production/test.override.yaml" "$@"; }
trap 'dc down --volumes' EXIT
image=$(jq -r .image "$release/coordinator-release.json")
baseline=farsail/coordinator:package-smoke-baseline
# This fixture uses the candidate binary on both sides; API before/after behavior
# is covered by coordinator integration tests, rather than claimed by this smoke.
docker tag "$image" "$baseline"
sed -i "s|^COORDINATOR_IMAGE=.*|COORDINATOR_IMAGE=$baseline|" "$state/compose.env"
for key in NGINX_IMAGE POSTGRES_IMAGE; do
  docker pull "$(sed -n "s/^$key=//p" "$state/compose.env")"
done
dc up -d --wait db gateway coordinator
dc exec -T db psql -U farsail -d farsail -v ON_ERROR_STOP=1 -c "INSERT INTO users(id,email,password_hash,verified) VALUES('00000000-0000-4000-8000-000000000001','package-fixture@example.test','fixture-not-a-login',true)"
gateway=$(dc ps -q gateway)
database=$(dc ps -q db)
bash "$release/upgrade-coordinator.sh" "$release" "$root"
[[ $(dc ps -q gateway) == "$gateway" && $(dc ps -q db) == "$database" ]]
[[ $(dc exec -T db psql -U farsail -d farsail -Atc "SELECT count(*) FROM users WHERE email='package-fixture@example.test'") == 1 ]]
[[ $(sed -n 's/^COORDINATOR_IMAGE=//p' "$state/compose.env") == "$image" ]]
cmp "$release/coordinator-release.json" "$state/coordinator-release.json"
test -x "$root/farsail-update"
test -s "$(find "$state/backups" -name database.dump -print -quit)"
printf 'Real coordinator package backup, health, identity and data-preservation smoke passed.\n'
