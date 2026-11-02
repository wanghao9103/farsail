#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-$HOME/.cache/codex-android/sdk}}"
: "${JAVA_HOME:=$HOME/.cache/codex-android/jdk}"
: "${ANDROID_NDK_HOME:=$ANDROID_HOME/ndk/27.2.12479018}"
export ANDROID_HOME JAVA_HOME
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
for pair in 'aarch64-linux-android arm64-v8a' 'x86_64-linux-android x86_64'; do
  read -r target abi <<< "$pair"
  cargo rustc --release --lib -p farsail-android --target "$target" --locked -- -C link-arg=-Wl,-z,max-page-size=16384
  mkdir -p "apps/android/app/src/main/jniLibs/$abi"
  cp "${CARGO_TARGET_DIR:-target}/$target/release/libfarsail_android.so" "apps/android/app/src/main/jniLibs/$abi/"
done
cd apps/android
./gradlew --no-daemon :app:assembleDebug :app:testDebugUnitTest :app:lintDebug "$@"
