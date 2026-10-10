#!/usr/bin/env bash
# Native Linux Debian packaging; no emulated/cross target is accepted here.
set -euo pipefail
cd "$(dirname "$0")/.."
native_target=$(rustc -vV | sed -n 's/^host: //p')
target=${FARSAIL_LINUX_TARGET:-$native_target}
[[ $target == "$native_target" ]] || { printf 'A native runner matching %s is required\n' "$target" >&2; exit 1; }
case "$target:$(uname -m):$(dpkg --print-architecture)" in
  x86_64-unknown-linux-gnu:x86_64:amd64) architecture=amd64 ;;
  aarch64-unknown-linux-gnu:aarch64:arm64) architecture=arm64 ;;
  *) printf 'Unsupported native Linux client architecture\n' >&2; exit 1 ;;
esac
FARSAIL_PACKAGE_TARGET="$target" node --input-type=module <<'JS'
import fs from 'node:fs';
const report=JSON.parse(fs.readFileSync('.local/linux-native-verification.json'));
if(!/^[a-f0-9]{40}$/.test(process.env.GITHUB_SHA||'')||!report.ok||report.target!==process.env.FARSAIL_PACKAGE_TARGET||report.source_commit!==process.env.GITHUB_SHA)throw Error('Packaging requires the native checks for this exact source and target');
JS
if [[ -n ${TAURI_SIGNING_PRIVATE_KEY:-} ]]; then
  npm run tauri -w @farsail/desktop -- build --target "$target" --bundles deb --config src-tauri/tauri.linux.conf.json -- --locked
else
  npm run tauri -w @farsail/desktop -- build --target "$target" --bundles deb --config src-tauri/tauri.linux.conf.json --config '{"bundle":{"createUpdaterArtifacts":false}}' -- --locked
fi
version=$(node -p 'require("./apps/desktop/src-tauri/tauri.conf.json").version')
package="${CARGO_TARGET_DIR:-target}/$target/release/bundle/deb/FarSail_${version}_${architecture}.deb"
test "$(dpkg-deb -f "$package" Version)" = "$version"
test "$(dpkg-deb -f "$package" Architecture)" = "$architecture"
mkdir -p .local/client-artifacts
extract=$(mktemp -d "$PWD/.local/deb-extracted.XXXXXX")
trap 'rm -rf -- "$extract"' EXIT
dpkg-deb -x "$package" "$extract"
binary="$extract/usr/bin/farsail-desktop"
node scripts/verify-linux-architecture.mjs "$binary" "$target"
ldd "$binary" | tee .local/deb-dependencies.txt
if grep -Fq 'not found' .local/deb-dependencies.txt; then
  printf 'Debian package has missing native dependencies\n' >&2
  exit 1
else
  dependency_check=$?
  [[ $dependency_check == 1 ]] || exit "$dependency_check"
fi
cp "$package" .local/client-artifacts/
if [[ -n ${TAURI_SIGNING_PRIVATE_KEY:-} ]]; then cp "$package.sig" .local/client-artifacts/; fi
FARSAIL_PACKAGE_ARCHITECTURE="$architecture" FARSAIL_PACKAGE_TARGET="$target" FARSAIL_PACKAGE_BINARY="$binary" node --input-type=module <<'JS'
import fs from 'node:fs';import crypto from 'node:crypto';
import {verifyLinuxArchitecture} from './scripts/verify-linux-architecture.mjs';
const version=JSON.parse(fs.readFileSync('apps/desktop/src-tauri/tauri.conf.json')).version;
const architecture=process.env.FARSAIL_PACKAGE_ARCHITECTURE,target=process.env.FARSAIL_PACKAGE_TARGET;
const binary=fs.readFileSync(process.env.FARSAIL_PACKAGE_BINARY);
const elf=verifyLinuxArchitecture(binary,target);
const installer=`FarSail_${version}_${architecture}.deb`,bytes=fs.readFileSync(`.local/client-artifacts/${installer}`);
const os=fs.readFileSync('/etc/os-release','utf8').match(/^PRETTY_NAME=(?:"([^"]+)"|(.+))$/m);
if(!os)throw Error('Missing native Linux build OS identity');
fs.writeFileSync('.local/client-artifacts/release-metadata.json',JSON.stringify({version,source_commit:process.env.GITHUB_SHA,installer,installer_sha256:crypto.createHash('sha256').update(bytes).digest('hex'),installer_bytes:bytes.length,target,deb_architecture:architecture,elf_machine:elf.machine,application_sha256:crypto.createHash('sha256').update(binary).digest('hex'),build_os:os[1]||os[2],native_debug_ipc:'passed',linux_store:'passed',package_dependencies:'passed'},null,2));
JS
