#!/usr/bin/env bash
# Build host only. All runtime images included; server never contacts Docker Hub.
set -euo pipefail
docker() { MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' command docker "$@"; }
if [[ $(uname -s) == MINGW* ]]; then jq() { command jq -b "$@"; }; fi
root=$(cd "$(dirname "$0")/../.." && pwd)
revision=${1:?revision required}; out=${2:?output directory required}
mkdir -p "$out"
jq -n --arg revision "$revision" '{revision:$revision,platform:"linux/amd64",images:{}}' > "$out/manifest.json"
tags=()
while IFS=$'\t' read -r key source; do
  if [[ $key != COORDINATOR_IMAGE && $key != RELAY_IMAGE ]]; then docker pull --platform linux/amd64 "$source"; fi
  info=$(docker image inspect "$source")
  [[ $(jq -r '.[0]|.Os+"/"+.Architecture' <<< "$info") == linux/amd64 ]]
  id=$(jq -r '.[0].Id' <<< "$info")
  tag=$source
  if [[ $key != COORDINATOR_IMAGE && $key != RELAY_IMAGE ]]; then
    name=$(tr '[:upper:]' '[:lower:]' <<< "${key%_IMAGE}")
    tag="farsail/offline-$name:${id#sha256:}"
  fi
  docker tag "$source" "$tag"; tags+=("$tag")
  jq --arg key "$key" --arg source "$source" --arg tag "$tag" --arg id "$id" '.images[$key]={source:$source,tag:$tag,id:$id}' "$out/manifest.json" > "$out/manifest.new"
  mv "$out/manifest.new" "$out/manifest.json"
done < <(jq -r --arg rev "$revision" '.+{COORDINATOR_IMAGE:("farsail/coordinator:"+$rev),RELAY_IMAGE:("farsail/relay:"+$rev)}|to_entries[]|[.key,.value]|@tsv' "$root/deploy/production/images.json")
docker save "${tags[@]}" | gzip -n -3 > "$out/farsail-linux-amd64-images.tar.gz"
(cd "$out" && sha256sum manifest.json farsail-linux-amd64-images.tar.gz > SHA256SUMS)
printf 'Six-image offline archive ready: %s\n' "$out"
