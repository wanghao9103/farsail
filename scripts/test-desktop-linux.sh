#!/usr/bin/env bash
# Run from an Ubuntu desktop or CI with Rust, Node, GTK/WebKitGTK and gnome-keyring installed.
set -euo pipefail
cd "$(dirname "$0")/.."
npm ci
npm run build
cargo fmt --all -- --check
cargo test -p farsail-client -p farsail-media -p farsail-windows -p farsail-desktop --lib
cargo clippy -p farsail-client -p farsail-media -p farsail-windows -p farsail-desktop --all-targets -- -D warnings
cargo build -p farsail-desktop
mkdir -p .local
task_session=$(mktemp -d "$PWD/.local/ubuntu-smoke.XXXXXX")
mkdir -p "$task_session/runtime" "$task_session/data"
chmod 700 "$task_session/runtime"
export XDG_RUNTIME_DIR="$task_session/runtime" XDG_DATA_HOME="$task_session/data"
export FARSAIL_TEST_PROFILE_DIR="$task_session/profile"
export FARSAIL_IPC_SMOKE_PATH="$task_session/report.json" FARSAIL_UBUNTU_SMOKE=1
export FARSAIL_TEST_BINARY="$(realpath "${CARGO_TARGET_DIR:-target}/debug/farsail-desktop")"
xvfb-run -a dbus-run-session -- bash -euo pipefail <<'NATIVE'
  printf "%s" "farsail-test-only" | gnome-keyring-daemon --unlock --components=secrets
  cargo test -p farsail-client native_keyring_roundtrip_and_profile_isolation --lib -- --ignored
  (cd apps/desktop && exec ../../node_modules/.bin/vite) >"$XDG_RUNTIME_DIR/vite.log" 2>&1 &
  vite=$!
  app=""
  trap 'kill "$vite" ${app:+"$app"} 2>/dev/null || true' EXIT
  for _ in {1..60}; do
    if curl -fsS http://127.0.0.1:1420/ >/dev/null 2>&1; then break; fi
    sleep 1
  done
  "$FARSAIL_TEST_BINARY" >"$XDG_RUNTIME_DIR/app.log" 2>&1 &
  app=$!
  for _ in {1..60}; do
    if [[ -f "$FARSAIL_IPC_SMOKE_PATH" ]]; then break; fi
    sleep 1
  done
  if [[ ! -f "$FARSAIL_IPC_SMOKE_PATH" ]]; then cat "$XDG_RUNTIME_DIR/app.log"; exit 1; fi
  node -e 'const r=JSON.parse(require("fs").readFileSync(process.env.FARSAIL_IPC_SMOKE_PATH)); console.log(r); if(!r.ok) process.exit(1)'
NATIVE
