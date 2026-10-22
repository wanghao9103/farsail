#!/usr/bin/env bash
# Apply a verified coordinator-only offline update to an existing deployment.
set -Eeuo pipefail
umask 077
# The online downloader checks this compatibility marker before executing us.
FARSAIL_ONLINE_UPDATE_PROTOCOL=1
readonly FARSAIL_ONLINE_UPDATE_PROTOCOL
release=$(realpath "${1:?Usage: upgrade-coordinator.sh RELEASE_DIR [DEPLOY_ROOT]}")
root=$(realpath "${2:-/home/data/farsail}")
state=$(realpath "${FARSAIL_STATE_DIR:-$root/.local/production}")
config="$root/deploy/production"
wait_seconds=${FARSAIL_UPGRADE_WAIT_SECONDS:-180}
selection=
if (( $# > 2 )); then
  [[ $# == 4 && $3 == --online-selection ]] || { printf 'Use RELEASE_DIR DEPLOY_ROOT [--online-selection SELECTION_JSON]\n' >&2; exit 1; }
  selection=$(realpath "$4")
fi
fail() { printf '%s\n' "$*" >&2; return 1; }
[[ $wait_seconds =~ ^[0-9]+$ ]] && ((wait_seconds >= 1 && wait_seconds <= 600)) || fail 'Wait timeout must be 1..600 seconds'
for tool in docker jq sha256sum flock; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
[[ -f $state/compose.env && -f $config/compose.yaml ]] || fail 'Existing deployment configuration not found'
exec 9> "${state}.operation.lock"
flock -n 9 || fail 'Another deployment operation is active'
temporary_files=()
temporary_directories=()
trap 'for temporary in "${temporary_files[@]}"; do rm -f -- "$temporary"; done; for temporary in "${temporary_directories[@]}"; do if [[ $temporary != "${preserved_noop_backup:-}" ]]; then rm -rf -- "$temporary"; fi; done' EXIT
verify_file() {
  local name=$1 expected
  expected=$(awk -v name="$name" '$2==name {print $1}' "$release/SHA256SUMS")
  [[ $expected =~ ^[0-9a-f]{64}$ ]] || fail "Missing checksum: $name"
  [[ -f $release/$name && ! -L $release/$name ]] || fail "Release file must be a regular file: $name"
  [[ $(sha256sum < "$release/$name" | cut -d' ' -f1) == "$expected" ]] || fail "Checksum mismatch: $name"
}
for name in coordinator-images.tar.gz coordinator-release.json; do
  verify_file "$name"
done
tag=$(jq -er '.image' "$release/coordinator-release.json")
digest=$(jq -er '.config_digest' "$release/coordinator-release.json")
[[ $tag =~ ^farsail/coordinator:[a-z0-9-]+$ && $digest =~ ^sha256:[0-9a-f]{64}$ ]] || fail 'Invalid release metadata'
dc() {
  local extra=()
  if [[ -f $state/settings.json ]] && [[ $(jq -r .mode "$state/settings.json") == test ]]; then extra=(-f "$config/test.override.yaml"); fi
  docker compose --env-file "$state/compose.env" -f "$config/compose.yaml" "${extra[@]}" "$@"
}
# Compose's process environment wins over --env-file. Refuse exported controls
# rather than locking/backing up one deployment while modifying another one.
mapfile -t compose_keys < <(sed -n 's/^\([A-Z_][A-Z0-9_]*\)=.*/\1/p' "$state/compose.env")
for key in "${compose_keys[@]}" COMPOSE_FILE COMPOSE_PROJECT_NAME COMPOSE_PROFILES COMPOSE_ENV_FILES COMPOSE_DISABLE_ENV_FILE STATE_DIR COORDINATOR_IMAGE; do
  if [[ $(declare -p "$key" 2>/dev/null || true) == 'declare -x '* ]]; then
    fail "Unset exported Compose override before upgrading: $key"
  fi
done
dc config --quiet
previous_tag=$(sed -n 's/^COORDINATOR_IMAGE=//p' "$state/compose.env")
[[ $previous_tag =~ ^farsail/coordinator:[a-z0-9-]+$ ]] || fail 'Invalid current coordinator image selection'
configured_image=$(dc config --format json | jq -er '.services.coordinator.image')
[[ $configured_image == "$previous_tag" ]] || fail 'Compose coordinator image differs from stored deployment selection'
configured_state=$(sed -n 's/^STATE_DIR=//p' "$state/compose.env")
[[ -n $configured_state && $(realpath "$configured_state") == "$state" ]] || fail 'STATE_DIR must identify the existing canonical deployment state'
stored_meta="$state/coordinator-release.json"
receipt="$state/coordinator-online-receipt.json"
current_revision=null
if [[ -f $stored_meta ]]; then
  [[ ! -L $stored_meta ]] || fail 'Stored release metadata must be a regular file'
  current_revision=$(jq -er '.source_revision // "null"' "$stored_meta")
  [[ $current_revision == null || $current_revision =~ ^[0-9a-f]{40}$ ]] || fail 'Stored source revision is invalid'
fi
new_receipt=
if [[ -n $selection ]]; then
  for name in upgrade-coordinator.sh update-online.sh install-online-updater.sh; do verify_file "$name"; done
  jq -e '.online_update_protocol == 1 and .platform == "linux/amd64" and (.source_revision | type == "string" and test("^[0-9a-f]{40}$"))' "$release/coordinator-release.json" >/dev/null || fail 'Online-compatible release metadata required'
  if [[ -f $stored_meta ]]; then
    [[ $(jq -er '.image' "$stored_meta") == "$previous_tag" ]] || fail 'Stored release metadata does not identify the selected coordinator image'
  fi
  jq -e '
    .repository == "wanghao9103/farsail" and
    (.asset_id | type == "number" and floor == . and . > 0 and . <= 9007199254740991) and
    (.created_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
    (.sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
    (.tag | type == "string" and test("^coordinator-[a-z0-9_-]+$")) and
    (.name | type == "string" and test("^FarSail_coordinator_[A-Za-z0-9_-]+_linux_amd64\\.tar\\.gz$")) and (.url | type == "string") and
    (.base_source_revision == null or (.base_source_revision | type == "string" and test("^[0-9a-f]{40}$"))) and
    (.source_comparison == "ahead" or .source_comparison == "identical" or .source_comparison == "legacy")
  ' "$selection" >/dev/null || fail 'Invalid official online selection'
  selected_tag=$(jq -r .tag "$selection")
  selected_name=$(jq -r .name "$selection")
  [[ $(jq -r .url "$selection") == "https://github.com/wanghao9103/farsail/releases/download/$selected_tag/$selected_name" ]] || fail 'Online asset URL must match the official repository, release tag and asset name'
  created_at=$(jq -r .created_at "$selection")
  [[ $(date -u -d "$created_at" +%Y-%m-%dT%H:%M:%SZ) == "$created_at" ]] || fail 'Invalid online asset timestamp'
  [[ $(jq -r '.base_source_revision // "null"' "$selection") == "$current_revision" ]] || fail 'Installed source changed after the online comparison; retry update'
  candidate_revision=$(jq -r .source_revision "$release/coordinator-release.json")
  comparison=$(jq -r .source_comparison "$selection")
  if [[ $current_revision == null ]]; then
    [[ $comparison == legacy && ( $previous_tag == farsail/coordinator:ubuntu-login-20261009 || $previous_tag == farsail/coordinator:verification-code-20261009 ) ]] || fail 'Unknown legacy image; manually apply one verified source-recording package before using latest updates'
    printf 'Known legacy bootstrap: source ancestry before this installation cannot be proven.\n'
  elif [[ $candidate_revision == "$current_revision" ]]; then
    [[ $comparison == identical ]] || fail 'Online source comparison does not match the installed baseline'
  else
    [[ $comparison == ahead ]] || fail 'Only a verified descendant of the installed source may be upgraded'
  fi
  new_receipt=$(mktemp "$state/.coordinator-online-receipt.XXXXXX")
  temporary_files+=("$new_receipt")
  jq -n --slurpfile selection "$selection" --slurpfile meta "$release/coordinator-release.json" '
    $selection[0] as $s | $meta[0] as $m |
    {online_update_protocol:1,repository:$s.repository,asset_id:$s.asset_id,created_at:$s.created_at,sha256:$s.sha256,
     tag:$s.tag,name:$s.name,url:$s.url,source_revision:$m.source_revision,image:$m.image,config_digest:$m.config_digest}
  ' > "$new_receipt"
  if [[ -e $receipt ]]; then
    [[ -f $receipt && ! -L $receipt ]] || fail 'Online receipt must be a regular file'
    jq -e --slurpfile candidate "$new_receipt" '
      . as $old | $candidate[0] as $new |
      $old.online_update_protocol == 1 and $old.repository == $new.repository and
      ($old.asset_id | type == "number" and floor == . and . > 0) and
      ($old.created_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
      ($old.sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      ($old.source_revision | type == "string" and test("^[0-9a-f]{40}$")) and
      ($old.image | type == "string" and test("^farsail/coordinator:[a-z0-9-]+$")) and
      ($old.config_digest | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
      ($old.tag | type == "string" and test("^coordinator-[a-z0-9_-]+$")) and
      ($old.name | type == "string" and test("^FarSail_coordinator_[A-Za-z0-9_-]+_linux_amd64\\.tar\\.gz$")) and
      $old.url == ("https://github.com/wanghao9103/farsail/releases/download/" + $old.tag + "/" + $old.name) and
      $new.created_at >= $old.created_at and
      ($new.created_at != $old.created_at or $new.asset_id >= $old.asset_id) and
      ($old.asset_id != $new.asset_id or $old == $new)
    ' "$receipt" >/dev/null || fail 'Online receipt refuses older assets, changed immutable asset identity, or a different repository'
  fi
fi
atomic_copy() {
  local source=$1 destination=$2 temporary
  temporary=$(mktemp "${destination}.tmp.XXXXXX") || return 1
  temporary_files+=("$temporary")
  cat "$source" > "$temporary" || return 1
  mv -f -- "$temporary" "$destination" || return 1
}
image_config_digest() {
  local reference=$1 exported loaded
  exported=$(docker save "$reference" | tar -xOf - manifest.json) || return 1
  loaded=$(jq -er 'if length == 1 then .[0].Config else error("Expected one image") end' <<< "$exported") || return 1
  printf 'sha256:%s\n' "$(basename "$loaded" .json)"
}
healthy_selected_container() {
  local container image
  container=$(dc ps -q coordinator) || return 1
  [[ -n $container && $container != *$'\n'* ]] || return 1
  [[ $(docker inspect "$container" --format '{{.State.Health.Status}}') == healthy ]] || return 1
  image=$(docker inspect "$container" --format '{{.Image}}') || return 1
  [[ $(image_config_digest "$image") == "$digest" ]]
}
install_updater() {
  if [[ -f $release/install-online-updater.sh ]]; then
    verify_file install-online-updater.sh
    if ! bash "$release/install-online-updater.sh" "$release" "$root"; then
      fail 'Coordinator is healthy, but persistent online updater installation failed; coordinator state has not been rolled back'
    fi
  fi
}
restore_records() {
  local failed=false
  if $had_meta; then atomic_copy "$backup/coordinator-release.json" "$stored_meta" || failed=true;
  else rm -f -- "$stored_meta" || failed=true; fi
  if $had_receipt; then atomic_copy "$backup/coordinator-online-receipt.json" "$receipt" || failed=true;
  else rm -f -- "$receipt" || failed=true; fi
  ! $failed
}
if [[ $previous_tag == "$tag" ]]; then
  [[ -n $selection ]] || fail 'Use a new immutable image tag; current coordinator is already selected'
  jq -e --slurpfile candidate "$release/coordinator-release.json" '.source_revision == $candidate[0].source_revision and .image == $candidate[0].image and .config_digest == $candidate[0].config_digest and .platform == $candidate[0].platform' "$stored_meta" >/dev/null || fail 'Same image tag has inconsistent stored release metadata'
  [[ $(image_config_digest "$tag") == "$digest" ]] || fail 'Same image tag has changed content'
  healthy_selected_container || fail 'Current coordinator is not healthy with the selected image; refusing a latest no-op'
  # Adopting a repackaged updater for the same verified image needs no database
  # backup/restart, but both public metadata and the high-water receipt must
  # recover together if either atomic publication fails.
  backup=$(mktemp -d "$state/.coordinator-noop.XXXXXX")
  temporary_directories+=("$backup")
  cp "$stored_meta" "$backup/coordinator-release.json"
  had_meta=true; had_receipt=false
  if [[ -e $receipt ]]; then cp "$receipt" "$backup/coordinator-online-receipt.json"; had_receipt=true; fi
  rollback_noop_records() {
    trap - ERR
    if ! restore_records; then
      preserved_noop_backup=$backup
      printf 'Record recovery failed; private recovery snapshots retained: %s\n' "$backup" >&2
    fi
    printf 'Latest no-op record publication failed; coordinator was not restarted.\n' >&2
    exit 1
  }
  trap rollback_noop_records ERR
  atomic_copy "$release/coordinator-release.json" "$stored_meta"
  atomic_copy "$new_receipt" "$receipt"
  trap - ERR
  install_updater
  printf 'Coordinator already latest and healthy: %s\n' "$tag"
  exit 0
fi
docker image inspect "$previous_tag" >/dev/null || fail 'Previous image unavailable for rollback'
docker load --input "$release/coordinator-images.tar.gz"
[[ $(docker image inspect "$tag" --format '{{.Os}}/{{.Architecture}}') == linux/amd64 ]] || fail 'Linux amd64 image required'
# Config hashes work with both classic and containerd Docker storage.
[[ $(image_config_digest "$tag") == "$digest" ]] || fail 'Loaded image config digest mismatch'
backup="$state/backups/coordinator-$(date -u +%Y%m%dT%H%M%SZ)-$$"
mkdir -p "$backup"
cp "$state/compose.env" "$backup/compose.env"
had_meta=false; had_receipt=false
if [[ -e $stored_meta ]]; then cp "$stored_meta" "$backup/coordinator-release.json"; had_meta=true; fi
if [[ -e $receipt ]]; then cp "$receipt" "$backup/coordinator-online-receipt.json"; had_receipt=true; fi
dc exec -T db pg_dump -U farsail -d farsail -Fc > "$backup/database.dump"
[[ -s $backup/database.dump ]] || fail 'Empty database backup'
dc exec -T db pg_restore --list < "$backup/database.dump" > /dev/null
printf 'Private backup: %s\n' "$backup"
rollback() {
  trap - ERR
  recovery_failed=false
  atomic_copy "$backup/compose.env" "$state/compose.env" || recovery_failed=true
  restore_records || recovery_failed=true
  printf 'Upgrade failed; restoring previous coordinator image. Database migration is retained.\n' >&2
  if $recovery_failed || ! dc up -d --pull never --no-deps --force-recreate --wait --wait-timeout "$wait_seconds" coordinator; then
    printf 'Rollback failed; inspect coordinator logs. Backup: %s\n' "$backup" >&2
  fi
  exit 1
}
trap rollback ERR
sed '/^COORDINATOR_IMAGE=/d' "$state/compose.env" > "$state/compose.env.new"
printf 'COORDINATOR_IMAGE=%s\n' "$tag" >> "$state/compose.env.new"
mv "$state/compose.env.new" "$state/compose.env"
dc up -d --pull never --no-deps --force-recreate --wait --wait-timeout "$wait_seconds" coordinator
healthy_selected_container || fail 'Coordinator health or running image identity did not match the verified release'
atomic_copy "$release/coordinator-release.json" "$stored_meta"
[[ -z $new_receipt ]] || atomic_copy "$new_receipt" "$receipt"
trap - ERR
install_updater
printf 'Coordinator upgraded and healthy: %s\n' "$tag"
printf 'Existing users/devices, gateway, relay, certificates and database volume retained.\n'
