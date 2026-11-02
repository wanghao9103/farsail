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
for tool in curl sha256sum tar mktemp awk date stat flock; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
if $latest; then command -v jq >/dev/null || fail 'Missing jq'; fi
# System temporary storage also supports deployments with a custom state directory
# and no .local under their root. umask/mktemp keep fetched scripts private.
work=$(mktemp -d "${TMPDIR:-/tmp}/farsail-online-update.XXXXXX")
download_pid=''
progress_pid=''
cleanup() {
  local result=$?
  trap - EXIT
  for pid in "$download_pid" "$progress_pid"; do
    [[ -z $pid ]] || { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }
  done
  rm -rf -- "$work"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fetch() {
  local address=$1 destination=$2 limit=$3 timeout=$4
  local -a display=(--silent)
  # Keep small metadata requests quiet; show curl's live byte/percentage,
  # speed and elapsed-time meter for the potentially long package transfer.
  [[ ${5:-} != progress ]] || display=()
  if ! curl -q --fail "${display[@]}" --show-error --location --proto '=https' --proto-redir '=https' \
    --retry 2 --retry-max-time "$((timeout*3+10))" --connect-timeout 20 --max-time "$timeout" \
    --max-filesize "$limit" --output "$destination" "$address"; then
    fail 'Download failed. Check the network and retry; deployment has not been changed.'
  fi
  [[ $(wc -c < "$destination") -le $limit ]] || fail 'Downloaded file exceeds the allowed size'
}
fetch_api() {
  local address=$1 destination=$2
  if ! curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --retry 2 --retry-max-time 200 --connect-timeout 20 --max-time 60 \
    --max-filesize 4194304 -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2022-11-28' --output "$destination" "$address"; then
    fail 'GitHub API request failed (network, service response or rate limit). Retry later; no package was executed and deployment was not changed.'
  fi
  [[ $(wc -c < "$destination") -le 4194304 ]] || fail 'GitHub response exceeds the allowed size'
}

# Package bytes persist across retries and invocations; metadata stays temporary.
# SHA256 identifies the exact immutable content. Never append another version.
# Count received bytes, including a validated response still being written. The
# overlap adjustment avoids double counting while a response is being committed.
received_bytes() {
  local target=$1 part bytes=0 size pending base overlap
  if [[ ! -d $target ]]; then [[ ! -f $target ]] || bytes=$(wc -c < "$target"); printf '%s' "$bytes"; return; fi
  for part in "$target"/*.part; do
    [[ -f $part ]] || continue
    size=$(wc -c < "$part"); bytes=$((bytes+size))
    if [[ -f ${part%.part}.response && -f ${part%.part}.base ]]; then
      pending=$(stat -c %s -- "${part%.part}.response" 2>/dev/null) || pending=0
      base=$(cat -- "${part%.part}.base" 2>/dev/null) || base=invalid
      if [[ $base =~ ^[0-9]+$ ]]; then
        overlap=$((size-base)); ((overlap>=0)) || overlap=0
        ((pending<=overlap)) || bytes=$((bytes+pending-overlap))
      fi
    fi
  done
  printf '%s' "$bytes"
}
# Only authenticated, matching HTTP 206 bytes may extend a piece. A pending
# response and its base survive SIGINT. Replaying after a mid-append interruption
# copies only the suffix not already committed to the piece.
consume_piece_response() {
  local prefix=$1 first=$2 last=$3 total=$4 part="$1.part" response="$1.response" headers="$1.headers" base start current count status range overlap
  [[ -f $headers && -f $prefix.base ]] || return 1
  status=$(awk '/^HTTP\// {s=$2} END {print s}' "$headers")
  if [[ $status == 200 ]]; then : > "${prefix%/*}/../no-range"; return 2; fi
  [[ -f $response ]] || return 1
  base=$(< "$prefix.base"); [[ $base =~ ^[0-9]+$ ]] || return 1
  start=$((first+base)); current=$(wc -c < "$part"); count=$(wc -c < "$response")
  range=$(awk '/^HTTP\// {r=""} tolower($0) ~ /^content-range:/ {r=tolower($0);sub(/^[^:]*:[[:space:]]*/,"",r);sub(/\r$/,"",r)} END {print r}' "$headers")
  [[ $status == 206 && $range == "bytes $start-$last/$total" ]] || return 1
  ((start>=first && start<=last && current>=base && current<=last-first+1 && count<=last-start+1)) || return 1
  overlap=$((current-base)); ((overlap<=count)) || return 1
  if ((count>overlap)); then tail -c "+$((overlap+1))" -- "$response" >> "$part" || return 1; fi
  rm -f -- "$response" "$headers" "$prefix.base" || return 1
}
download_piece() {
  local prefix=$1 first=$2 last=$3 total=$4 address=$5 part="$1.part" offset start attempt code http recovered=0 chunk_curl=''
  trap 'exit 143' TERM
  trap 'exit 130' INT
  trap 'if [[ -n $chunk_curl ]]; then kill "$chunk_curl" 2>/dev/null || true; wait "$chunk_curl" 2>/dev/null || true; fi' EXIT
  for suffix in part response headers base error; do
    local entry="$prefix.$suffix"
    [[ ! -L $entry && ( ! -e $entry || -f $entry ) ]] || exit 1
    [[ ! -e $entry || ( -O $entry && $(stat -c %h -- "$entry") == 1 ) ]] || exit 1
  done
  [[ -f $part ]] || : > "$part"
  if [[ -f $prefix.response ]]; then
    if consume_piece_response "$prefix" "$first" "$last" "$total"; then :; else
      recovered=$?; ((recovered!=2)) || exit 2
      rm -f -- "$prefix.response" "$prefix.headers" "$prefix.base"
    fi
  fi
  for attempt in 1 2 3; do
    offset=$(wc -c < "$part")
    ((offset<=last-first+1)) || exit 1
    ((offset!=last-first+1)) || return 0
    start=$((first+offset)); printf '%s\n' "$offset" > "$prefix.base"
    curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
      --retry 0 --retry-max-time 3600 --connect-timeout 20 --max-time 3600 \
      --speed-limit 1024 --speed-time 120 --max-filesize "$((last-start+1))" \
      --range "$start-$last" --dump-header "$prefix.headers" \
      --output "$prefix.response" "$address" 2> "$prefix.error" &
    chunk_curl=$!
    if wait "$chunk_curl"; then code=0; else code=$?; fi
    chunk_curl=''
    http=$(awk '/^HTTP\// {s=$2} END {print s}' "$prefix.headers" 2>/dev/null || true)
    if consume_piece_response "$prefix" "$first" "$last" "$total"; then
      offset=$(wc -c < "$part")
      ((offset!=last-first+1)) || return 0
    else
      recovered=$?; ((recovered!=2)) || exit 2
      rm -f -- "$prefix.response" "$prefix.headers" "$prefix.base"
    fi
    # Integrity/header failures are never appended. Network failures retain only
    # the prefix whose Content-Range was checked, then retry that remaining range.
    case $code in 0|5|6|7|16|18|28|52|55|56|92) ;; 22) case $http in 408|429|500|502|503|504) ;; *) return 1 ;; esac ;; *) return 1 ;; esac
    if ((attempt<3)); then printf '分片 %s 中断，已保留 %s 字节，正在重试。\n' "${prefix##*/}" "$(wc -c < "$part")" >&2; sleep "$attempt"; fi
  done
  return 1
}
run_piece_workers() {
  local directory=$1 total=$2 address=$3 piece_size=2097152 count index first last id pid failed=0
  local -a workers=()
  trap 'exit 143' TERM
  trap 'exit 130' INT
  trap 'for pid in "${workers[@]}"; do kill "$pid" 2>/dev/null || true; done; for pid in "${workers[@]}"; do wait "$pid" 2>/dev/null || true; done' EXIT
  count=$(((total+piece_size-1)/piece_size))
  for ((index=0;index<count;index++)); do
    printf -v id '%05d' "$index"; first=$((index*piece_size)); last=$((first+piece_size-1)); ((last<total)) || last=$((total-1))
    download_piece "$directory/$id" "$first" "$last" "$total" "$address" & workers+=("$!")
    if ((${#workers[@]}==4 || index==count-1)); then
      for pid in "${workers[@]}"; do if wait "$pid"; then :; else failed=1; break; fi; done
      ((failed==0)) || return 1
      workers=()
    fi
  done
}
fetch_pieces() {
  local address=$1 digest=$2 total=$3 directory=$4 start initial elapsed average index id count part code=0
  segments_ok=false
  [[ ! -L $directory && ( ! -e $directory || -d $directory ) ]] || fail 'Unsafe fragment cache directory'
  mkdir -p -- "$directory"; chmod 700 -- "$directory"
  count=$(((total+2097151)/2097152))
  printf '分片下载：4 路并发，每片约 2 MB，共 %s 片；已有片段会继续使用。\n' "$count" >&2
  start=$(date +%s); initial=$(received_bytes "$directory")
  run_piece_workers "$directory" "$total" "$address" & download_pid=$!
  show_progress "$directory" "$total" "$initial" "$download_pid" "$start" & progress_pid=$!
  if wait "$download_pid"; then code=0; else code=$?; fi
  download_pid=''; kill "$progress_pid" 2>/dev/null || true; wait "$progress_pid" 2>/dev/null || true; progress_pid=''
  [[ ! -t 2 ]] || printf '\n' >&2
  if ((code!=0)); then return; fi
  : > "${directory%/*}/package.assembling"
  for ((index=0;index<count;index++)); do
    printf -v id '%05d' "$index"; part="$directory/$id.part"
    cat -- "$part" >> "${directory%/*}/package.assembling"
  done
  [[ $(wc -c < "${directory%/*}/package.assembling") == "$total" ]] || fail 'Fragment total size mismatch; deployment has not been changed'
  if [[ $(sha256sum < "${directory%/*}/package.assembling" | cut -d' ' -f1) != "$digest" ]]; then
    rm -f -- "${directory%/*}/package.assembling"; rm -rf -- "$directory"
    fail 'Package SHA256 mismatch; corrupt fragments removed; deployment has not been changed'
  fi
  mv -- "${directory%/*}/package.assembling" "${directory%/*}/package.tar.gz"
  package="${directory%/*}/package.tar.gz"; rm -rf -- "$directory"
  elapsed=$(( $(date +%s)-start )); ((elapsed>0)) || elapsed=1
  average=$(((total-initial)/elapsed)); ((average>=0)) || average=0
  printf '%s\n' "$(format_progress "$total" "$total" "$average" "$elapsed")" >&2
  segments_ok=true
}

format_progress() {
  awk -v done="$1" -v total="$2" -v speed="$3" -v elapsed="$4" '
    function bytes(n) {if(n>=1000000000)return sprintf("%.2f GB",n/1000000000); if(n>=1000000)return sprintf("%.1f MB",n/1000000); if(n>=1000)return sprintf("%.1f KB",n/1000); return sprintf("%.0f B",n)}
    function clock(n) {n=int(n); if(n>=3600)return sprintf("%02d:%02d:%02d",n/3600,(n%3600)/60,n%60);return sprintf("%02d:%02d",n/60,n%60)}
    BEGIN {eta="计算中";if(total>0 && done>=total)eta="00:00";else if(total>done && speed>0)eta=clock(int((total-done)/speed+0.999));
      if(total>0) printf "下载 %5.1f%%  %s / %s",(done>total?100:100*done/total),bytes(done),bytes(total);else printf "已下载 %s",bytes(done);
      printf "  速度 %s/s  剩余 %s  已用 %s",bytes(speed),eta,clock(elapsed)}'
}
show_progress() {
  # The display must not keep the cache lock alive after cancellation.
  trap - EXIT INT TERM
  exec 7>&-
  local file=$1 total=$2 initial=$3 pid=$4 start=$5 now previous=$3 previous_at=$5 last_emit=0 rate=0 current delta line
  while kill -0 "$pid" 2>/dev/null; do
    now=$(date +%s); current=$(received_bytes "$file")
    delta=$((now-previous_at))
    if ((delta>0)); then
      if ((current>=previous)); then rate=$(((rate*2+(current-previous)/delta)/3)); else rate=0; fi
      previous=$current; previous_at=$now
    fi
    if [[ -t 2 ]] || ((now-last_emit>=5)); then
      line=$(format_progress "$current" "$total" "$rate" "$((now-start))")
      if [[ -t 2 ]]; then printf '\r\033[K%s' "$line" >&2; else printf '%s\n' "$line" >&2; fi
      last_emit=$now
    fi
    sleep 1
  done
}
fetch_package() {
  local address=$1 digest=$2 total=$3 base="$state/updater-cache" directory file cached offset code http attempt restarted=false start count cache_key initial elapsed average
  cache_key=$(printf '%s\n%s\n' "$address" "$digest" | sha256sum | cut -d' ' -f1)
  for directory in "$base" "$base/$cache_key"; do
    [[ ! -L $directory && ( ! -e $directory || -d $directory ) ]] || fail 'Unsafe updater cache directory'
    mkdir -p -- "$directory"
    [[ -O $directory ]] || fail 'Use the original deployment user to access the updater cache'
    chmod 700 -- "$directory"
  done
  cached="$base/$cache_key/package.tar.gz"; file="$base/$cache_key/package.part"
  for directory in "$file" "$cached" "$base/$cache_key/download.lock" "$base/$cache_key/no-range" "$base/$cache_key/package.assembling"; do
    [[ ! -L $directory && ( ! -e $directory || -f $directory ) ]] || fail 'Unsafe updater cache file'
    if [[ -e $directory ]]; then
      [[ -O $directory && $(stat -c %h -- "$directory") == 1 ]] || fail 'Updater cache file ownership or link count is unsafe'
    fi
  done
  exec 7> "$base/$cache_key/download.lock"
  flock -n 7 || fail 'Another process is downloading this coordinator package; retry after it finishes'
  if [[ -f $cached ]]; then
    count=$(wc -c < "$cached")
    if ((count<=package_limit)) && [[ $(sha256sum < "$cached" | cut -d' ' -f1) == "$digest" ]]; then
      printf '%s\n' '复用已下载并校验的升级包，无需重新下载。' >&2
      package=$cached; return
    fi
    rm -f -- "$cached"
  fi
  if [[ -f $file ]] && (( $(wc -c < "$file") > package_limit )); then rm -f -- "$file"; fi
  # A transfer can finish just before SIGINT or connection failure. Reuse it only
  # after verifying the complete digest, without trusting size/HTTP status alone.
  if [[ -f $file && $(sha256sum < "$file" | cut -d' ' -f1) == "$digest" ]]; then
    mv -- "$file" "$cached"; package=$cached; return
  fi
  if ((total>0)) && [[ ! -f $base/$cache_key/no-range ]]; then
    fetch_pieces "$address" "$digest" "$total" "$base/$cache_key/pieces"
    if $segments_ok; then return; fi
    if [[ -f $base/$cache_key/no-range ]]; then
      printf '%s\n' '服务器不支持分片，改用可续传的单路下载。' >&2
    else
      printf '分片进度已保留：%s；重跑同一命令将继续下载。\n' "$base/$cache_key/pieces" >&2
      fail 'Download failed. Retry the same command to resume; deployment has not been changed.'
    fi
  fi
  start=$(date +%s); initial=$(received_bytes "$file")
  for attempt in 1 2 3; do
    offset=0; [[ ! -f $file ]] || offset=$(wc -c < "$file")
    if ((offset>0)); then printf '继续下载，已保留 %s 字节（尝试 %s/3）。\n' "$offset" "$attempt" >&2
    else printf '开始下载（尝试 %s/3）。\n' "$attempt" >&2; fi
    # One hour per attempt permits slow but progressing links. A stalled link
    # retries sooner. Each new curl starts from the actual newly written offset.
    curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
      --retry 0 --retry-max-time 3600 --connect-timeout 20 --max-time 3600 \
      --speed-limit 1024 --speed-time 120 --max-filesize "$package_limit" \
      --continue-at "$offset" --write-out '%{http_code}' --output "$file" "$address" \
      > "$work/package-http-code" 2> "$work/package-curl-error" &
    download_pid=$!
    show_progress "$file" "$total" "$offset" "$download_pid" "$start" &
    progress_pid=$!
    if wait "$download_pid"; then code=0; else code=$?; fi
    download_pid=''
    kill "$progress_pid" 2>/dev/null || true; wait "$progress_pid" 2>/dev/null || true; progress_pid=''
    [[ ! -t 2 ]] || printf '\n' >&2
    count=0; [[ ! -f $file ]] || count=$(wc -c < "$file")
    if ((count>package_limit)); then rm -f -- "$file"; fail 'Downloaded file exceeds the allowed size'; fi
    if ((code==0)); then
      [[ $(sha256sum < "$file" | cut -d' ' -f1) == "$digest" ]] || { rm -f -- "$file"; fail 'Package SHA256 mismatch; corrupt cache removed; deployment has not been changed'; }
      elapsed=$(( $(date +%s)-start )); ((elapsed>0)) || elapsed=1
      average=$(((count-initial)/elapsed)); ((average>=0)) || average=0
      printf '%s\n' "$(format_progress "$count" "$count" "$average" "$elapsed")" >&2
      mv -- "$file" "$cached"; package=$cached; return
    fi
    cat "$work/package-curl-error" >&2
    http=$(< "$work/package-http-code")
    if { ((code==33)) || [[ $http == 416 ]]; } && ((offset>0)) && ! $restarted; then
      printf '%s\n' '服务器不支持当前断点，清除分片后安全重新下载。' >&2
      rm -f -- "$file"; restarted=true
    else
      case $code in
        5|6|7|16|18|28|52|55|56|92) ;;
        22) case $http in 408|429|500|502|503|504) ;; *) break ;; esac ;;
        *) break ;;
      esac
    fi
    if ((attempt<3)); then printf '下载中断，%s 秒后自动重试；已保留的进度不会丢失。\n' "$attempt" >&2; sleep "$attempt"; fi
  done
  printf '已保留 %s 字节，缓存位置：%s\n' "$count" "$file" >&2
  fail 'Download failed. Retry the same command to resume; deployment has not been changed.'
}

if $latest; then
  printf '%s\n' 'Checking coordinator updates…' >&2
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

printf '%s\n' 'Downloading coordinator package…' >&2
total=0
if $latest; then total=$(jq -r .size "$work/candidate.json"); fi
fetch_package "$url" "$expected" "$total"
printf '%s\n' 'Verifying coordinator package…' >&2
actual=$(sha256sum "$package" | cut -d' ' -f1)
[[ $actual == "$expected" ]] || fail 'Package SHA256 mismatch; deployment has not been changed'
# Validate before extracting: no absolute/traversal names, links, devices, nested
# paths, duplicate entries or unbounded expansion. Current coordinator packages
# are flat; the manual mode also accepts the harmless ./ root directory.
LC_ALL=C tar -tzf "$package" > "$work/names"
LC_ALL=C tar -tvzf "$package" > "$work/types"
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
tar --no-same-owner --no-same-permissions -xzf "$package" -C "$work/release"
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
  printf '%s\n' 'Applying coordinator update…' >&2
  bash "$work/release/upgrade-coordinator.sh" "$work/release" "$root" --online-selection "$work/selection.json"
else
  printf '%s\n' 'Applying coordinator update…' >&2
  bash "$work/release/upgrade-coordinator.sh" "$work/release" "$root"
fi
