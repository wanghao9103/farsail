#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-$HOME/.cache/codex-android/sdk}}"
: "${JAVA_HOME:=$HOME/.cache/codex-android/jdk}"
: "${ANDROID_NDK_HOME:=$ANDROID_HOME/ndk/27.2.12479018}"
export ANDROID_HOME JAVA_HOME ANDROID_NDK_HOME
export CMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"
ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
if [[ ! -x "$ndk_bin/aarch64-linux-android26-clang" ]]; then
  echo 'Install Android NDK 27.2.12479018, or set ANDROID_NDK_HOME.' >&2; exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ndk_bin/aarch64-linux-android26-clang"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$ndk_bin/x86_64-linux-android26-clang"
export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
export CC_x86_64_linux_android="$CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER"
export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
export AR_x86_64_linux_android="$ndk_bin/llvm-ar"
cd "$repo_dir"
if [[ "${1:-}" == '--codec-tests' ]]; then
  report="${TMPDIR:-/tmp}/farsail-codec-artifacts-$$.jsonl"
  trap 'rm -f "$report"' EXIT
  cargo test --release --locked -p farsail-media --target aarch64-linux-android --lib --no-run --message-format=json > "$report"
  python3 - "$report" <<'PY'
import json, pathlib, shutil, sys
artifacts = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
tests = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('target', {}).get('name') == 'farsail_media' and a.get('profile', {}).get('test') and a.get('executable')]
assert len(tests) == 1, 'Expected one ARM media test executable'
output = pathlib.Path('.local/android-codec-tests')
output.mkdir(parents=True, exist_ok=True)
shutil.copy2(tests[0], output / 'farsail-media-tests')
PY
  exit 0
fi
for pair in 'aarch64-linux-android arm64-v8a' 'x86_64-linux-android x86_64'; do
  read -r target abi <<< "$pair"
  cargo rustc --release --lib -p farsail-android --target "$target" --locked -- -C link-arg=-Wl,-z,max-page-size=16384
  mkdir -p "apps/android/app/src/main/jniLibs/$abi"
  cp "${CARGO_TARGET_DIR:-target}/$target/release/libfarsail_android.so" "apps/android/app/src/main/jniLibs/$abi/"
done
python3 - <<'PY'
import os, pathlib, shutil, tomllib
packages = tomllib.loads(pathlib.Path('Cargo.lock').read_text())['package']
registry = pathlib.Path(os.environ.get('CARGO_HOME', str(pathlib.Path.home() / '.cargo'))) / 'registry/src'
output = pathlib.Path('apps/android/app/src/main/assets/third-party/native-jpeg')
output.mkdir(parents=True, exist_ok=True)
for name in ('turbojpeg', 'turbojpeg-sys'):
    version = next(p['version'] for p in packages if p['name'] == name)
    roots = list(registry.glob(f'*/{name}-{version}'))
    assert roots, f'Missing verified crate source: {name}'
    for source in roots[0].glob('*LICENSE*'):
        if source.is_file(): shutil.copy2(source, output / f'{name}-{source.name}')
    if name == 'turbojpeg-sys':
        for filename in ('LICENSE.md', 'README.ijg'):
            source = roots[0] / 'libjpeg-turbo' / filename
            assert source.is_file(), f'Missing native JPEG license: {filename}'
            shutil.copy2(source, output / f'libjpeg-turbo-{filename}')
(output / 'NOTICE.txt').write_text('This software is based in part on the work of the Independent JPEG Group.\nNative JPEG codec: libjpeg-turbo; Rust bindings: turbojpeg / turbojpeg-sys.\nSee the accompanying license notices.\n')
PY
cd apps/android
./gradlew --no-daemon :app:assembleDebug :app:testDebugUnitTest :app:lintDebug "$@"
