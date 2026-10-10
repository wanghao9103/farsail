#!/usr/bin/env bash
# Apply a verified coordinator-only offline update to an existing deployment.
set -Eeuo pipefail
umask 077
release=$(realpath "${1:?Usage: upgrade-coordinator.sh RELEASE_DIR [DEPLOY_ROOT]}")
root=$(realpath "${2:-/home/data/farsail}")
state=$(realpath "${FARSAIL_STATE_DIR:-$root/.local/production}")
config="$root/deploy/production"
wait_seconds=${FARSAIL_UPGRADE_WAIT_SECONDS:-180}
fail() { printf '%s\n' "$*" >&2; exit 1; }
[[ $wait_seconds =~ ^[0-9]+$ ]] && ((wait_seconds >= 1 && wait_seconds <= 600)) || fail 'Wait timeout must be 1..600 seconds'
for tool in docker jq sha256sum flock; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
[[ -f $state/compose.env && -f $config/compose.yaml ]] || fail 'Existing deployment configuration not found'
exec 9> "${state}.operation.lock"
flock -n 9 || fail 'Another deployment operation is active'
for name in coordinator-images.tar.gz coordinator-release.json; do
  expected=$(awk -v name="$name" '$2==name {print $1}' "$release/SHA256SUMS")
  [[ $expected =~ ^[0-9a-f]{64}$ ]] || fail "Missing checksum: $name"
  [[ $(sha256sum "$release/$name" | cut -d' ' -f1) == "$expected" ]] || fail "Checksum mismatch: $name"
done
tag=$(jq -er '.image' "$release/coordinator-release.json")
digest=$(jq -er '.config_digest' "$release/coordinator-release.json")
[[ $tag =~ ^farsail/coordinator:[a-z0-9-]+$ && $digest =~ ^sha256:[0-9a-f]{64}$ ]] || fail 'Invalid release metadata'
dc() {
  local extra=()
  if [[ -f $state/settings.json ]] && [[ $(jq -r .mode "$state/settings.json") == test ]]; then extra=(-f "$config/test.override.yaml"); fi
  docker compose --env-file "$state/compose.env" -f "$config/compose.yaml" "${extra[@]}" "$@"
}
dc config --quiet
previous_tag=$(sed -n 's/^COORDINATOR_IMAGE=//p' "$state/compose.env")
[[ -n $previous_tag && $previous_tag != "$tag" ]] || fail 'Use a new immutable image tag; current coordinator is already selected'
docker image inspect "$previous_tag" >/dev/null || fail 'Previous image unavailable for rollback'
docker load --input "$release/coordinator-images.tar.gz"
[[ $(docker image inspect "$tag" --format '{{.Os}}/{{.Architecture}}') == linux/amd64 ]] || fail 'Linux amd64 image required'
# Config hashes work with both classic and containerd Docker storage.
metadata=$(docker save "$tag" | tar -xOf - manifest.json)
loaded=$(jq -er --arg tag "$tag" '.[]|select(.RepoTags|index($tag))|.Config' <<< "$metadata")
[[ "sha256:$(basename "$loaded" .json)" == "$digest" ]] || fail 'Loaded image config digest mismatch'
backup="$state/backups/coordinator-$(date -u +%Y%m%dT%H%M%SZ)-$$"
mkdir -p "$backup"
cp "$state/compose.env" "$backup/compose.env"
[[ ! -f $state/coordinator-release.json ]] || cp "$state/coordinator-release.json" "$backup/coordinator-release.json"
dc exec -T db pg_dump -U farsail -d farsail -Fc > "$backup/database.dump"
[[ -s $backup/database.dump ]] || fail 'Empty database backup'
dc exec -T db pg_restore --list < "$backup/database.dump" > /dev/null
printf 'Private backup: %s\n' "$backup"
rollback() {
  trap - ERR
  cp "$backup/compose.env" "$state/compose.env"
  printf 'Upgrade failed; restoring previous coordinator image. Database migration is retained.\n' >&2
  if ! dc up -d --pull never --no-deps --force-recreate --wait --wait-timeout "$wait_seconds" coordinator; then
    printf 'Rollback failed; inspect coordinator logs. Backup: %s\n' "$backup" >&2
  fi
  exit 1
}
trap rollback ERR
sed '/^COORDINATOR_IMAGE=/d' "$state/compose.env" > "$state/compose.env.new"
printf 'COORDINATOR_IMAGE=%s\n' "$tag" >> "$state/compose.env.new"
mv "$state/compose.env.new" "$state/compose.env"
dc up -d --pull never --no-deps --force-recreate --wait --wait-timeout "$wait_seconds" coordinator
cp "$release/coordinator-release.json" "$state/coordinator-release.json"
trap - ERR
printf 'Coordinator upgraded and healthy: %s\n' "$tag"
printf 'Existing users/devices, gateway, relay, certificates and database volume retained.\n'
