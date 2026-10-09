#!/usr/bin/env bash
# Download a pinned coordinator package over HTTPS, then run its verified upgrade.
set -euo pipefail
umask 077
url=${1:?Usage: update-online.sh HTTPS_PACKAGE_URL SHA256 [DEPLOY_ROOT]}
expected=${2:?Expected package SHA256 required}
root=$(realpath "${3:-/home/data/farsail}")
fail() { printf '%s\n' "$*" >&2; exit 1; }
[[ $url =~ ^https://[^[:space:]]+$ ]] || fail 'An HTTPS package URL is required'
[[ $expected =~ ^[0-9a-f]{64}$ ]] || fail 'Use the exact release package SHA256'
state=${FARSAIL_STATE_DIR:-$root/.local/production}
[[ -f $state/compose.env && -f $root/deploy/production/compose.yaml ]] || fail 'Existing deployment configuration not found'
for tool in curl sha256sum tar mktemp; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
work=$(mktemp -d "$root/.local/online-update.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
curl --fail --show-error --location --proto '=https' --proto-redir '=https' \
  --retry 3 --connect-timeout 20 --max-time 600 --max-filesize 268435456 \
  --output "$work/package.tar.gz" "$url"
actual=$(sha256sum "$work/package.tar.gz" | cut -d' ' -f1)
[[ $actual == "$expected" ]] || fail 'Package SHA256 mismatch; deployment has not been changed'
mkdir "$work/release"
tar -xzf "$work/package.tar.gz" -C "$work/release"
[[ -f $work/release/upgrade-coordinator.sh ]] || fail 'Coordinator upgrade script missing from verified package'
(cd "$work/release" && sha256sum -c SHA256SUMS)
bash "$work/release/upgrade-coordinator.sh" "$work/release" "$root"
