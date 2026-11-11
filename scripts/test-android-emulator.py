#!/usr/bin/env python3
"""Run real JNI/projection/accessibility + encrypted QUIC against an owned emulator.
Requires built debug/test APKs and `cargo build -p farsail-android --example device_probe`.
No production coordinator or credentials are used. Refuses physical devices.
"""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get("ANDROID_VERIFICATION_OUT", str(ROOT / ".local/android-verification")))
OUT.mkdir(parents=True, exist_ok=True)


serial = os.environ.get("ANDROID_SERIAL")
if not serial:
    devices = subprocess.check_output(["adb", "devices"], text=True).splitlines()
    emulators = [line.split()[0] for line in devices if line.startswith("emulator-") and "\tdevice" in line]
    assert len(emulators) == 1, "Start exactly one dedicated emulator, or set ANDROID_SERIAL"
    serial = emulators[0]
ADB = ["adb", "-s", serial]

def adb(*args, check=True):
    return subprocess.run([*ADB, *args], capture_output=True, check=check, timeout=120)


assert adb("shell", "getprop", "ro.kernel.qemu").stdout.strip() == b"1", "Use an emulator, never a physical phone"
probe = ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug/examples/device_probe"
key = subprocess.check_output([str(probe), "--key"], text=True).strip()
assert re.fullmatch(r"[a-f0-9]{64}", key)
adb("install", "-r", str(ROOT / "apps/android/app/build/outputs/apk/debug/app-debug.apk"))
adb("install", "-r", str(ROOT / "apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"))
# The dedicated emulator package contains synthetic state from this test only.
adb("shell", "pm", "clear", "app.farsail.android")
redir = None
ready_forward = None
backend = os.environ.get("ANDROID_CAPTURE_BACKEND", "projection")
assert backend in ("projection", "accessibility")
connection_wake = backend == "projection" and os.environ.get("ANDROID_CONNECTION_WAKE") == "1"
wake_idle_seconds = int(os.environ.get("ANDROID_WAKE_IDLE_SECONDS", "0"))
assert 0 <= wake_idle_seconds <= 300
assert wake_idle_seconds == 0 or connection_wake, "Idle wake check needs projection connection wake"
guest = os.environ.get("ANDROID_PROBE_IN_GUEST") == "1"
if guest:
    guest_probe = ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "x86_64-linux-android/release/examples/device_probe"
    assert guest_probe.is_file(), "Build the Android x86_64 device_probe example first"
    adb("push", str(guest_probe), "/data/local/tmp/farsail-device-probe")
    adb("shell", "chmod", "700", "/data/local/tmp/farsail-device-probe")
with (OUT / "instrumentation.log").open("wb") as log:
    test = subprocess.Popen([*ADB, "shell", "am", "instrument", "-w", "-e", "class", "app.farsail.android.NativeSmokeTest#keystoreJniProjectionBackgroundAndStop", "-e", "realPeer", "true", "-e", "sourceKey", key, "-e", "captureBackend", backend, "-e", "wakePeer", str(backend == "accessibility").lower(), "-e", "connectionWake", str(connection_wake).lower(), "-e", "wakeIdleSeconds", str(wake_idle_seconds),
                             "app.farsail.android.test/androidx.test.runner.AndroidJUnitRunner"], stdout=log, stderr=subprocess.STDOUT)
    try:
        end = time.monotonic() + 90 + wake_idle_seconds
        endpoint = None
        idle_waited = False
        while time.monotonic() < end and test.poll() is None:
            result = adb("shell", "run-as", "app.farsail.android", "cat", "files/p2p-ready.json", check=False)
            if result.returncode == 0:
                endpoint = json.loads(result.stdout)
                break
            if wake_idle_seconds and not idle_waited:
                idling = adb("shell", "run-as", "app.farsail.android", "cat", "files/p2p-idling.json", check=False)
                if idling.returncode == 0:
                    idle_waited = True
                    print(f"Leaving owned emulator asleep for {wake_idle_seconds}s without ADB polling", flush=True)
                    time.sleep(wake_idle_seconds)
            time.sleep(0.3)
        assert endpoint, "Android test did not publish readiness; inspect instrumentation.log"
        (OUT / "endpoint.json").write_text(json.dumps(endpoint, indent=2))
        addresses = endpoint.get("addrs", [])
        ip_addresses = [a.get("Ip", a.get("ip", "")) for a in addresses if isinstance(a, dict)]
        ipv4 = next(a for a in ip_addresses if a and ":" in a and not a.startswith("["))
        guest_port = int(ipv4.rsplit(":", 1)[1])
        if guest:
            command = [*ADB, "shell", "/data/local/tmp/farsail-device-probe", endpoint["id"], str(guest_port), str(endpoint["tapX"]), str(endpoint["tapY"])]
        else:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as available:
                available.bind(("127.0.0.1", 0))
                host_port = available.getsockname()[1]
            result = adb("emu", "redir", "add", f"udp:{host_port}:{guest_port}")
            assert b"OK" in result.stdout, result.stdout
            redir = host_port
            command = [str(probe), endpoint["id"], str(host_port), str(endpoint["tapX"]), str(endpoint["tapY"])]
        if backend == "accessibility" or connection_wake:
            command.append("--connection-wake")
            ready_port = endpoint["inputReadyPort"]
            if not guest:
                ready_forward = int(adb("forward", "tcp:0", f"tcp:{ready_port}").stdout)
                ready_port = ready_forward
            command.append(str(ready_port))
        with (OUT / "device-probe.jsonl").open("wb") as report:
            remote = subprocess.Popen(command, stdout=report, stderr=subprocess.STDOUT)
            time.sleep(3)
            (OUT / "remote-input.png").write_bytes(adb("exec-out", "screencap", "-p").stdout)
            assert remote.wait(timeout=25) == 0, "Desktop probe failed; inspect device-probe.jsonl"
        assert test.wait(timeout=60) == 0
        result = (OUT / "instrumentation.log").read_text()
        assert "OK (1 test)" in result, result
        frames = [json.loads(line) for line in (OUT / "device-probe.jsonl").read_text().splitlines() if line.startswith("{")]
        assert any(f.get("first_frame") for f in frames)
        assert any(f.get("input_sent") and f.get("still_open") for f in frames)
        if backend == "accessibility":
            wake = json.loads(adb("exec-out", "run-as", "app.farsail.android", "cat", "files/p2p-wake.json").stdout)
            assert wake.get("approvedConnectionWokeScreen") and wake.get("secureStillLocked"), wake
            (OUT / "wake.json").write_text(json.dumps(wake, indent=2))
        if connection_wake:
            wake = json.loads(adb("exec-out", "run-as", "app.farsail.android", "cat", "files/p2p-wake.json").stdout)
            assert wake.get("connectionWokeScreen") and wake.get("insecureKeyguardDismissed"), wake
            (OUT / "wake.json").write_text(json.dumps(wake, indent=2))
        print(f"PASS: Android Keystore/JNI, {backend}, background/task removal, authenticated QUIC frames and owned text/click input, local stop and notification cleanup")
    finally:
        if ready_forward:
            adb("forward", "--remove", f"tcp:{ready_forward}", check=False)
        if redir:
            adb("emu", "redir", "del", f"udp:{redir}", check=False)
        if test.poll() is None:
            # Let test finally blocks stop projection and revoke synthetic state.
            try:
                test.wait(timeout=55)
            except subprocess.TimeoutExpired:
                test.terminate()
