#!/usr/bin/env bash
# Run inside the native smoke's isolated D-Bus/Xvfb session, with Vite and a WM ready.
set -euo pipefail
test -n "${FARSAIL_TEST_BINARY:-}"
test -n "${XDG_RUNTIME_DIR:-}"
tray_dir=$(mktemp -d "$XDG_RUNTIME_DIR/farsail-tray.XXXXXX")
app=''
host=''
trap 'kill ${app:+"$app"} ${host:+"$host"} 2>/dev/null || true' EXIT
unset FARSAIL_UBUNTU_SMOKE FARSAIL_VIEWER_SMOKE FARSAIL_SHARE_SMOKE
export FARSAIL_TRAY_SMOKE=1
wait_report() {
  local expected="$1"
  for _ in {1..60}; do
    if [[ -f "$FARSAIL_IPC_SMOKE_PATH" ]]; then
      if node -e 'const r=JSON.parse(require("fs").readFileSync(process.env.FARSAIL_IPC_SMOKE_PATH));if(r.ok===false){console.error(r);process.exit(2)}process.exit((r.phase===process.argv[1]||(process.argv[1]==="done"&&r.ok))?0:1)' "$expected"; then return; else
        code=$?
        [[ "$code" == 1 ]] || return "$code"
      fi
    fi
    sleep 0.5
  done
  cat "$tray_dir/$mode-app.log"
  printf 'Tray smoke did not reach %s\n' "$expected" >&2
  return 1
}
for mode in no-host host; do
  if [[ "$mode" == host ]]; then
    python3 scripts/tray-probe.py host > "$tray_dir/host.log" 2>&1 &
    host=$!
    for _ in {1..30}; do [[ -f "$XDG_RUNTIME_DIR/tray-host-ready" ]] && break; sleep 0.1; done
    test -f "$XDG_RUNTIME_DIR/tray-host-ready"
  fi
  export FARSAIL_TEST_PROFILE_DIR="$tray_dir/$mode-profile"
  mkdir -p "$FARSAIL_TEST_PROFILE_DIR"
  export FARSAIL_IPC_SMOKE_PATH="$tray_dir/$mode-report.json"
  "$FARSAIL_TEST_BINARY" > "$tray_dir/$mode-app.log" 2>&1 &
  app=$!
  wait_report closed
  timeout 15 "$FARSAIL_TEST_BINARY" > "$tray_dir/$mode-second-launch.log" 2>&1
  if [[ "$mode" == host ]]; then
    wait_report closed-again
    python3 scripts/tray-probe.py open > "$tray_dir/menu.json"
  fi
  wait_report done
  cat "$FARSAIL_IPC_SMOKE_PATH"
  if [[ "$mode" == host ]]; then python3 scripts/tray-probe.py quit; fi
  for _ in {1..30}; do kill -0 "$app" 2>/dev/null || break; sleep 0.2; done
  if kill -0 "$app" 2>/dev/null; then printf 'Explicit Quit did not exit FarSail\n' >&2; exit 1; fi
  wait "$app"
  app=''
done
cp "$tray_dir/host-report.json" "$XDG_RUNTIME_DIR/tray-host-report.json"
cp "$tray_dir/no-host-report.json" "$XDG_RUNTIME_DIR/tray-no-host-report.json"
