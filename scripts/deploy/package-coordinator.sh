#!/usr/bin/env bash
# Build-host packaging only; consumes an already built, revision-labelled image.
set -Eeuo pipefail
revision=${1:?Usage: package-coordinator.sh REVISION OUTPUT_DIR}
out=${2:?Output directory required}
[[ $revision =~ ^[0-9a-f]{40}$ && $(git rev-parse HEAD) == "$revision" ]]
image="farsail/coordinator:$revision"
[[ $(docker image inspect "$image" --format '{{.Os}}/{{.Architecture}}') == linux/amd64 ]]
[[ $(docker image inspect "$image" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}') == "$revision" ]]
mkdir -p "$out/release"
release=$(realpath "$out/release")
docker save "$image" | gzip -n -3 > "$release/coordinator-images.tar.gz"
config=$(tar -xOzf "$release/coordinator-images.tar.gz" manifest.json | jq -er --arg image "$image" 'if length == 1 and .[0].RepoTags == [$image] then .[0].Config else error("Expected one coordinator image") end')
digest=$(basename "$config" .json)
[[ $digest =~ ^[0-9a-f]{64}$ ]]
jq -n --arg revision "$revision" --arg image "$image" --arg digest "sha256:$digest" \
  '{online_update_protocol:1,source_revision:$revision,platform:"linux/amd64",image:$image,config_digest:$digest}' > "$release/coordinator-release.json"
cp scripts/deploy/{upgrade-coordinator,update-online,install-online-updater}.sh "$release/"
cat > "$release/README.md" <<'EOF'
# FarSail coordinator update

Removes the account-wide remote-request quota that could return HTTP 429 when
connecting or reconnecting several devices. Login protection, device ownership,
source credentials and target authorization remain required.

Existing deployments with the online updater: run `/home/data/farsail/farsail-update`.
For a verified, extracted offline package:

    bash upgrade-coordinator.sh "$PWD" /home/data/farsail

Use your existing deployment directory. The upgrade backs up the database and
restarts the coordinator. Existing sessions may need to reconnect.
Android and desktop congestion/recovery fixes require separate client updates.
EOF
(cd "$release" && sha256sum coordinator-images.tar.gz coordinator-release.json upgrade-coordinator.sh update-online.sh install-online-updater.sh README.md > SHA256SUMS && sha256sum -c SHA256SUMS)
name="FarSail_coordinator_multiview_${revision:0:7}_linux_amd64.tar.gz"
tar -C "$release" -czf "$out/$name" coordinator-images.tar.gz coordinator-release.json upgrade-coordinator.sh update-online.sh install-online-updater.sh README.md SHA256SUMS
(cd "$out" && sha256sum "$name" > "$name.sha256")
