#!/usr/bin/env bash
# Verify completeness without depending on existing daemon layers/tags.
set -euo pipefail
if [[ $(uname -s) == MINGW* ]]; then jq() { command jq -b "$@"; }; fi
dir=${1:?release directory required}
archive="$dir/farsail-linux-amd64-images.tar.gz"
tar -tzf "$archive" > "$dir/archive-files.txt"
tar -xOzf "$archive" manifest.json > "$dir/docker-manifest.json"
[[ $(jq length "$dir/docker-manifest.json") == 6 ]]
while IFS= read -r member; do
  grep -Fx -- "$member" "$dir/archive-files.txt" >/dev/null || { echo "Missing archive member: $member" >&2; exit 1; }
done < <(jq -r '.[]|.Config,.Layers[]' "$dir/docker-manifest.json")
while IFS= read -r tag; do
  jq -e --arg tag "$tag" 'any(.[]; .RepoTags|index($tag))' "$dir/docker-manifest.json" >/dev/null
done < <(jq -r '.images[].tag' "$dir/manifest.json")
printf 'All six fixed tags, image configurations and every referenced layer are present in archive.\n'
