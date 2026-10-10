#!/usr/bin/env bash
# Install only the verified updater tools into an existing deployment.
set -Eeuo pipefail
umask 077
fail() { printf '%s\n' "$*" >&2; exit 1; }
[[ $# -ge 1 && $# -le 2 ]] || fail 'Usage: install-online-updater.sh RELEASE_DIR [DEPLOY_ROOT]'
for tool in realpath sha256sum awk flock mktemp mkdir install ln cp mv rm chmod; do
  command -v "$tool" >/dev/null || fail "Missing $tool"
done
release=$(realpath -- "$1")
root=$(realpath -- "${2:-/home/data/farsail}")
state_path=${FARSAIL_STATE_DIR:-$root/.local/production}
[[ $state_path == /* ]] || fail 'FARSAIL_STATE_DIR must be an absolute existing path'
state=$(realpath -- "$state_path")
[[ -f $state/compose.env && -f $root/deploy/production/compose.yaml ]] || fail 'Existing deployment configuration not found'
[[ -f $release/SHA256SUMS && ! -L $release/SHA256SUMS ]] || fail 'Verified bundle SHA256SUMS missing'

verify_file() {
  local name=$1 expected actual
  [[ -f $release/$name && ! -L $release/$name ]] || fail "Verified bundle file missing: $name"
  expected=$(awk -v name="$name" '
    $2 == name { if (NF != 2) exit 1; count++; digest=$1 }
    END { if (count != 1) exit 1; print digest }
  ' "$release/SHA256SUMS") || fail "Missing or ambiguous checksum: $name"
  [[ $expected =~ ^[0-9a-f]{64}$ ]] || fail "Invalid checksum: $name"
  actual=$(sha256sum < "$release/$name")
  [[ ${actual%% *} == "$expected" ]] || fail "Checksum mismatch: $name"
  printf '%s' "$expected"
}
updater_sha=$(verify_file update-online.sh)
installer_sha=$(verify_file install-online-updater.sh)
self=$(realpath -- "${BASH_SOURCE[0]}")
self_sha=$(sha256sum < "$self")
[[ ${self_sha%% *} == "$installer_sha" ]] || fail 'Running installer does not match the verified bundle'

# This is separate from the coordinator operation lock: the upgrader may call
# this installer after becoming healthy while still holding its own lock.
exec 8> "${state}.updater-tools.lock"
flock -n 8 || fail 'Another updater installation is active'
tools="$state/tools"
versions="$tools/online-updaters"
for directory in "$tools" "$versions"; do
  [[ ! -L $directory ]] || fail 'Updater tool directories must not be symlinks'
  [[ ! -e $directory || -d $directory ]] || fail 'Updater tool directory is not a directory'
done
mkdir -p -- "$versions"
updater="$tools/update-online.sh"
entry="$root/farsail-update"
for destination in "$updater" "$entry"; do
  [[ ! -e $destination || -f $destination || -L $destination ]] || fail 'Updater entry is not a regular file or link'
done
tools_work=''
root_work=''
transaction=false
old_updater=false
old_entry=false
cleanup() {
  local status=$? restored=true
  trap - EXIT
  if [[ $transaction == true ]]; then
    if [[ $old_updater == true ]]; then
      mv -Tf -- "$tools_work/previous-updater" "$updater" || restored=false
    else
      rm -f -- "$updater" || restored=false
    fi
    if [[ $old_entry == true ]]; then
      mv -Tf -- "$root_work/previous-entry" "$entry" || restored=false
    else
      rm -f -- "$entry" || restored=false
    fi
    [[ $restored == true ]] || printf 'Updater entry rollback failed; inspect the private installation staging directories.\n' >&2
  fi
  if [[ $restored == true ]]; then
    [[ -z $tools_work ]] || rm -rf -- "$tools_work"
    [[ -z $root_work ]] || rm -rf -- "$root_work"
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
tools_work=$(mktemp -d "$tools/.updater-install.XXXXXX")
root_work=$(mktemp -d "$root/.updater-install.XXXXXX")
version="$versions/$updater_sha"
if [[ -e $version || -L $version ]]; then
  [[ -d $version && ! -L $version && -f $version/update-online.sh && ! -L $version/update-online.sh ]] || fail 'Invalid existing updater version'
  existing_sha=$(sha256sum < "$version/update-online.sh")
  [[ ${existing_sha%% *} == "$updater_sha" ]] || fail 'Existing updater version checksum mismatch'
else
  mkdir -- "$tools_work/version"
  install -m 700 -- "$release/update-online.sh" "$tools_work/version/update-online.sh"
  staged_sha=$(sha256sum < "$tools_work/version/update-online.sh")
  [[ ${staged_sha%% *} == "$updater_sha" ]] || fail 'Updater changed while being staged'
  mv -T -- "$tools_work/version" "$version"
fi
# Keep old immutable versions so an updater already running can finish safely.
ln -s -- "online-updaters/$updater_sha/update-online.sh" "$tools_work/next-updater"
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\n'
  printf 'if [[ $# -gt 1 || ( $# -eq 1 && $1 != --check ) ]]; then\n'
  printf '  printf "Usage: farsail-update [--check]\\n" >&2; exit 1\nfi\n'
  printf 'export FARSAIL_STATE_DIR=%q\n' "$state"
  printf 'exec bash %q --latest %q "$@"\n' "$updater" "$root"
} > "$root_work/next-entry"
chmod 700 "$root_work/next-entry"
if [[ -e $updater || -L $updater ]]; then
  cp -pP -- "$updater" "$tools_work/previous-updater"
  old_updater=true
fi
if [[ -e $entry || -L $entry ]]; then
  cp -pP -- "$entry" "$root_work/previous-entry"
  old_entry=true
fi
transaction=true
mv -Tf -- "$tools_work/next-updater" "$updater"
mv -Tf -- "$root_work/next-entry" "$entry"
transaction=false
printf 'Online updater installed. Check: %q --check\n' "$entry"
printf 'Upgrade: %q\n' "$entry"
