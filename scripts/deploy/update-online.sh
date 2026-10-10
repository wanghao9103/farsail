#!/usr/bin/env bash
# Discover a published coordinator-only package, or download an explicitly pinned one.
set -euo pipefail
umask 077
fail() { printf '%s\n' "$*" >&2; exit 1; }
usage() {
  printf '%s\n' 'Usage: update-online.sh --latest [--check] [DEPLOY_ROOT]' \
    '       update-online.sh --check [DEPLOY_ROOT]' \
    '       update-online.sh HTTPS_PACKAGE_URL SHA256 [DEPLOY_ROOT]'
}
readonly repository=wanghao9103/farsail
readonly api_base=https://api.github.com/repos/wanghao9103/farsail
readonly package_limit=268435456
latest=false
check=false
if [[ ${1:-} == --latest || ${1:-} == --check ]]; then
  latest=true
  roots=()
  for arg in "$@"; do
    case $arg in
      --latest) ;;
      --check) check=true ;;
      --*) usage >&2; fail 'Unknown option' ;;
      *) roots+=("$arg") ;;
    esac
  done
  ((${#roots[@]} <= 1)) || fail 'Only one deployment root is allowed'
  root=$(realpath "${roots[0]:-/home/data/farsail}")
else
  (($# >= 2 && $# <= 3)) || { usage >&2; exit 1; }
  url=$1
  expected=$2
  root=$(realpath "${3:-/home/data/farsail}")
  [[ $url =~ ^https://[^[:space:]]+$ ]] || fail 'An HTTPS package URL is required'
  [[ $expected =~ ^[0-9a-f]{64}$ ]] || fail 'Use the exact release package SHA256'
fi
state=${FARSAIL_STATE_DIR:-$root/.local/production}
[[ -f $state/compose.env && -f $root/deploy/production/compose.yaml ]] || fail 'Existing deployment configuration not found'
for tool in curl sha256sum tar mktemp awk; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
if $latest; then command -v jq >/dev/null || fail 'Missing jq'; fi
# System temporary storage also supports deployments with a custom state directory
# and no .local under their root. umask/mktemp keep fetched scripts private.
work=$(mktemp -d "${TMPDIR:-/tmp}/farsail-online-update.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
fetch() {
  local address=$1 destination=$2 limit=$3 timeout=$4
  curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --retry 2 --retry-max-time "$timeout" --connect-timeout 20 --max-time "$timeout" \
    --max-filesize "$limit" --output "$destination" "$address"
  [[ $(wc -c < "$destination") -le $limit ]] || fail 'Downloaded file exceeds the allowed size'
}
fetch_api() {
  local address=$1 destination=$2
  if ! curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --retry 2 --retry-max-time 60 --connect-timeout 20 --max-time 60 \
    --max-filesize 4194304 -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2022-11-28' --output "$destination" "$address"; then
    fail 'GitHub API request failed (network, service response or rate limit). Retry later; no package was executed and deployment was not changed.'
  fi
  [[ $(wc -c < "$destination") -le 4194304 ]] || fail 'GitHub response exceeds the allowed size'
}

if $latest; then
  pages=()
  complete=false
  # Scan all releases, including published previews. Never use /releases/latest:
  # clients share this repository, and GitHub excludes previews from that route.
  for page in 1 2 3; do
    response="$work/releases-$page.json"
    fetch_api "$api_base/releases?per_page=100&page=$page" "$response"
    jq -e 'type == "array" and length <= 100' "$response" >/dev/null || fail 'Invalid GitHub releases response'
    pages+=("$response")
    if [[ $(jq length "$response") -lt 100 ]]; then complete=true; break; fi
  done
  $complete || fail 'Release discovery exceeded 300 entries; latest cannot be determined safely'
  jq -s '
    def utc:
      type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$") and
      (try ((fromdateiso8601 | strftime("%Y-%m-%dT%H:%M:%SZ")) == .) catch false);
    def integer: type == "number" and . == floor and . > 0 and . <= 9007199254740991;
    add | [
      .[] | select(.draft == false and .published_at != null) |
      select(.tag_name | type == "string" and test("^coordinator-[a-z0-9_-]+$")) as $release |
      ($release.assets // [])[] |
      select(.name | type == "string" and test("^FarSail_coordinator_[A-Za-z0-9_-]+_linux_amd64\\.tar\\.gz$")) as $asset |
      {tag:$release.tag_name, name:$asset.name, asset_id:$asset.id,
       created_at:$asset.created_at, state:$asset.state, size:$asset.size,
       url:$asset.browser_download_url, digest:$asset.digest,
       checksums:[($release.assets // [])[] | select(.name == ($asset.name + ".sha256"))]}
    ] |
    if any(.[]; ((.created_at | utc) | not) or ((.asset_id | integer) | not))
      then error("Invalid coordinator asset identity") else . end |
    sort_by([.created_at,.asset_id]) | last
  ' "${pages[@]}" > "$work/candidate.json" || fail 'Invalid coordinator release metadata'
  [[ $(jq -r type "$work/candidate.json") == object ]] || fail 'No published coordinator package is available for automatic updates; historical mail/client packages are not selected'
  jq -e --argjson limit "$package_limit" '
    .state == "uploaded" and (.size | type == "number" and . == floor and . > 0 and . <= $limit) and
    (.checksums | length == 1) and .checksums[0].state == "uploaded" and
    (.checksums[0].size | type == "number" and . == floor and . > 0 and . <= 4096)
  ' "$work/candidate.json" >/dev/null || fail 'Newest coordinator package is incomplete or exceeds the size limit; an older package will not be selected'
  name=$(jq -r .name "$work/candidate.json")
  tag=$(jq -r .tag "$work/candidate.json")
  url="https://github.com/$repository/releases/download/$tag/$name"
  checksum_url="$url.sha256"
  jq -e --arg url "$url" --arg sum "$checksum_url" '
    .url == $url and .checksums[0].browser_download_url == $sum and
    (.digest == null or (.digest | type == "string" and test("^sha256:[0-9a-f]{64}$"))) and
    (.checksums[0].digest == null or (.checksums[0].digest | type == "string" and test("^sha256:[0-9a-f]{64}$")))
  ' "$work/candidate.json" >/dev/null || fail 'Coordinator asset URL or GitHub digest is invalid'
  fetch "$checksum_url" "$work/package.sha256" 4096 60
  checksum_digest=$(jq -r '.checksums[0].digest // ""' "$work/candidate.json")
  [[ -z $checksum_digest || "sha256:$(sha256sum "$work/package.sha256" | cut -d' ' -f1)" == "$checksum_digest" ]] || fail 'Checksum file does not match its GitHub digest'
  sumtext=$(< "$work/package.sha256")
  [[ $sumtext != *$'\n'* && $sumtext =~ ^([0-9a-f]{64})[[:space:]]+\*?([A-Za-z0-9._-]+)$ ]] || fail 'Checksum file must contain one SHA256 and the exact package name'
  expected=${BASH_REMATCH[1]}
  [[ ${BASH_REMATCH[2]} == "$name" ]] || fail 'Checksum file refers to a different package'
  asset_digest=$(jq -r '.digest // ""' "$work/candidate.json")
  [[ -z $asset_digest || $asset_digest == "sha256:$expected" ]] || fail 'Package SHA256 disagrees with its GitHub digest'
  printf 'Published coordinator package: %s (%s)\n' "$name" "$(jq -r .created_at "$work/candidate.json")"
  if $check; then
    printf '%s\n' 'Check complete. Deployment and Docker were not changed. Run --latest without --check to verify and apply this package.'
    exit 0
  fi
fi

fetch "$url" "$work/package.tar.gz" "$package_limit" 600
actual=$(sha256sum "$work/package.tar.gz" | cut -d' ' -f1)
[[ $actual == "$expected" ]] || fail 'Package SHA256 mismatch; deployment has not been changed'
# Validate before extracting: no absolute/traversal names, links, devices, nested
# paths, duplicate entries or unbounded expansion. Current coordinator packages
# are flat; the manual mode also accepts the harmless ./ root directory.
LC_ALL=C tar -tzf "$work/package.tar.gz" > "$work/names"
LC_ALL=C tar -tvzf "$work/package.tar.gz" > "$work/types"
awk -v flat="$latest" '
  {n=$0; sub(/^\.\//,"",n); sub(/\/$/,"",n); if(n=="" || n==".") next;
   if(n !~ /^[A-Za-z0-9_-][A-Za-z0-9_.\/-]*$/ || n ~ /(^|\/)\.\.?(\/|$)/ || length(n)>160 || seen[n]++) exit 1;
   if(flat=="true" && n !~ /^(coordinator-images\.tar\.gz|coordinator-release\.json|SHA256SUMS|upgrade-coordinator\.sh|update-online\.sh|install-online-updater\.sh|README\.md|LOCAL_VERIFICATION\.md)$/) exit 1;
   if(++count>32) exit 1}
  END {if(count==0) exit 1}
' "$work/names" || fail 'Unsafe or duplicate package path; deployment has not been changed'
awk '
  substr($1,1,1)=="d" {next}
  {if(substr($1,1,1)!="-" || $3 !~ /^[0-9]+$/) exit 1;
   total+=$3; if(total>268435456) exit 1}
' "$work/types" || fail 'Package contains links, special files or excessive expanded content'
mkdir "$work/release"
tar --no-same-owner --no-same-permissions -xzf "$work/package.tar.gz" -C "$work/release"
[[ -f $work/release/upgrade-coordinator.sh && -f $work/release/SHA256SUMS ]] || fail 'Coordinator upgrade script or checksums missing from verified package'
awk '
  {if(NF!=2 || length($1)!=64 || $1 !~ /^[0-9a-f]+$/) exit 1;
   n=$2; sub(/^\*/,"",n); sub(/^\.\//,"",n);
   if(n !~ /^[A-Za-z0-9_-][A-Za-z0-9_.\/-]*$/ || n ~ /(^|\/)\.\.?(\/|$)/ || seen[n]++) exit 1}
' "$work/release/SHA256SUMS" || fail 'Invalid internal package checksums'
(cd "$work/release" && sha256sum -c SHA256SUMS)

if $latest; then
  for required in coordinator-images.tar.gz coordinator-release.json upgrade-coordinator.sh update-online.sh install-online-updater.sh; do
    [[ -f $work/release/$required ]] || fail "Automatic update package is missing $required"
    awk -v required="$required" '{n=$2; sub(/^\*/,"",n); sub(/^\.\//,"",n); if(n==required) found=1} END {exit !found}' "$work/release/SHA256SUMS" || fail "Internal checksum missing for $required"
  done
  grep -Eqx '(readonly )?FARSAIL_ONLINE_UPDATE_PROTOCOL=1' "$work/release/upgrade-coordinator.sh" || fail 'Package upgrader does not support automatic-update protocol 1'
  jq -e '
    .online_update_protocol == 1 and .platform == "linux/amd64" and
    (.source_revision | type == "string" and test("^[0-9a-f]{40}$")) and
    (.image | type == "string" and test("^farsail/coordinator:[a-z0-9-]+$")) and
    (.config_digest | type == "string" and test("^sha256:[0-9a-f]{64}$"))
  ' "$work/release/coordinator-release.json" >/dev/null || fail 'Package metadata does not satisfy automatic-update protocol 1'
  candidate_revision=$(jq -r .source_revision "$work/release/coordinator-release.json")
  base_revision=null
  comparison=legacy
  if [[ -f $state/coordinator-release.json ]]; then
    base_revision=$(jq -c '.source_revision // null' "$state/coordinator-release.json") || fail 'Installed coordinator metadata is invalid'
    if [[ $base_revision != null ]]; then
      base_text=$(jq -r '.source_revision' "$state/coordinator-release.json")
      [[ $base_text =~ ^[0-9a-f]{40}$ ]] || fail 'Installed source revision cannot be compared safely; use an explicitly pinned update'
      if [[ $base_text == "$candidate_revision" ]]; then
        comparison=identical
      else
        fetch_api "$api_base/compare/$base_text...$candidate_revision" "$work/comparison.json"
        jq -e --arg base "$base_text" '
          .base_commit.sha == $base and
          ((.status == "ahead" and (.ahead_by | type == "number" and . > 0) and .behind_by == 0) or
           (.status == "identical" and .ahead_by == 0 and .behind_by == 0))
        ' "$work/comparison.json" >/dev/null || fail 'Published coordinator source is not ahead of the installed source; automatic downgrade/divergence is refused'
        comparison=$(jq -r .status "$work/comparison.json")
      fi
    fi
  fi
  jq --arg repository "$repository" --arg sha "$expected" --argjson base "$base_revision" --arg comparison "$comparison" '
    {repository:$repository, asset_id,created_at,sha256:$sha,tag,name,url,
     base_source_revision:$base, source_comparison:$comparison}
  ' "$work/candidate.json" > "$work/selection.json"
  bash "$work/release/upgrade-coordinator.sh" "$work/release" "$root" --online-selection "$work/selection.json"
else
  bash "$work/release/upgrade-coordinator.sh" "$work/release" "$root"
fi
