#!/usr/bin/env python3
"""Owned-emulator-only remembered access test across an actual process death.
Uses a temporary local synthetic coordinator; never touches production accounts.
Requires built debug APKs. Run with ANDROID_SERIAL=emulator-....
"""
import http.server
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
HISTORY = ROOT / '.local/android-memory-verification'
OUT = HISTORY / f'attempt-{time.time_ns()}'
OUT.mkdir(parents=True, exist_ok=True)
print(f'Evidence directory: {OUT}', flush=True)
serial = os.environ.get('ANDROID_SERIAL', '')
assert re.fullmatch(r'emulator-[0-9]+', serial), 'Set ANDROID_SERIAL to an owned emulator; physical devices are refused'
ADB = ['adb', '-s', serial]
def adb(*args, check=True):
    return subprocess.run([*ADB, *args], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=check, timeout=35)
assert adb('shell', 'getprop', 'ro.kernel.qemu').stdout.strip() == b'1'
requests = []
class Coordinator(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def answer(self):
        length = int(self.headers.get('Content-Length', '0'))
        body = json.loads(self.rfile.read(length)) if length else {}
        path = self.path.split('?')[0]; requests.append(path)
        if path in ('/v1/auth/login', '/v1/auth/refresh'):
            data = dict(access_token='synthetic-access', refresh_token='synthetic-refresh', session_id='10000000-0000-4000-8000-000000000001', access_expires_in=900)
        elif path == '/v1/me':
            data = dict(id='20000000-0000-4000-8000-000000000001', email='memory-fixture@example.invalid', email_verified=True, role='user')
        elif path == '/v1/devices/challenge':
            data = dict(challenge_id='30000000-0000-4000-8000-000000000001', message='synthetic-binding-challenge')
        elif path == '/v1/devices/bind':
            assert body['platform'] == 'android'
            data = dict(id='40000000-0000-4000-8000-000000000001', device_token='synthetic-device')
        elif path == '/v1/devices/40000000-0000-4000-8000-000000000001':
            data = dict(id='40000000-0000-4000-8000-000000000001', enabled=True, bound=True, name='Owned memory fixture', platform='android')
        elif path == '/v1/devices/heartbeat': data = dict(generation=1, expires_in=60)
        elif path in ('/v1/devices', '/v1/remote/pending', '/v1/remote/sessions'): data = []
        else: data = {}
        raw = json.dumps(data).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json'); self.send_header('Content-Length', str(len(raw))); self.end_headers(); self.wfile.write(raw)
    do_GET = answer
    do_POST = answer
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Coordinator)
port = server.server_port
thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
old_settings = {key: adb('shell', 'settings', 'get', 'secure', key).stdout.decode().strip() for key in ['enabled_accessibility_services', 'accessibility_enabled']}
stage1 = None
try:
    adb('reverse', f'tcp:{port}', f'tcp:{port}')
    adb('install', '-r', str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk'))
    adb('install', '-r', str(ROOT/'apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk'))
    adb('shell', 'pm', 'clear', 'app.farsail.android')
    with (OUT/'stage1.log').open('wb') as log:
        stage1 = subprocess.Popen([*ADB, 'shell', 'am', 'instrument', '-w', '-r', '-e', 'class', 'app.farsail.android.RememberedAccessTest#userEnablesAndWaitsForProcessRestart', '-e', 'memoryDriver', 'true', '-e', 'fixtureUrl', f'http://127.0.0.1:{port}', 'app.farsail.android.test/androidx.test.runner.AndroidJUnitRunner'], stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 85
        ready = None
        while time.monotonic() < deadline and stage1.poll() is None:
            result = adb('shell', 'run-as', 'app.farsail.android', 'cat', 'files/memory-ready.json', check=False)
            if result.returncode == 0:
                ready = json.loads(result.stdout); break
            time.sleep(.3)
        assert ready and ready['sharing'] and ready['control'], 'Stage 1 did not reach a real live share; inspect stage1.log'
        adb('install', '-r', str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk'))
        stage1.wait(timeout=15)
    result = adb('shell', 'pidof', 'app.farsail.android', check=False)
    assert str(ready['pid']).encode() not in result.stdout.split(), 'Old process must actually be dead'
    def state():
        result = adb('shell', 'content', 'query', '--uri', 'content://app.farsail.android.memory-fixture/state', check=False)
        return dict(re.findall(r'(\w+)=(true|false|[0-9]+|[a-z_]+)', result.stdout.decode())) if result.returncode == 0 else {}
    def wait_state(test):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            current = state()
            if test(current): return current
            time.sleep(.2)
        raise AssertionError(f'Normal-process state timed out: {current}')
    def open_app():
        adb('shell', 'am', 'start', '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.LAUNCHER', '-n', 'app.farsail.android/.MainActivity')
    def tap(name):
        x, y = ready['buttons'][name]
        assert isinstance(x, int) and isinstance(y, int) and 0 < x < 3000 and 0 < y < 5000
        adb('shell', 'input', 'tap', str(x), str(y))
    # Points are measured from the real owned UI in phase 1. Avoid a second
    # UiAutomation client suppressing the accessibility service under test.
    background = wait_state(lambda x: x.get('ready') == 'true' and x.get('wantedSharing') == 'true')
    assert background['phase'] == 'off' and background['capture'] == 'false'
    open_app()
    fresh = wait_state(lambda x: x.get('ready') == 'true' and x.get('phase') == 'consent' and x.get('wantedSharing') == 'true')
    assert fresh['pid'] != str(ready['pid']) and fresh['wantedControl'] == 'true'
    tap('cancel')
    wait_state(lambda x: x.get('phase') == 'off' and x.get('wantedSharing') == 'false' and x.get('control') == 'true')
    time.sleep(2)
    assert state()['phase'] == 'off'
    tap('start'); wait_state(lambda x: x.get('phase') == 'consent'); tap('allow')
    wait_state(lambda x: x.get('phase') == 'sharing' and x.get('wantedSharing') == 'true')
    tap('stop')
    wait_state(lambda x: x.get('capture') == 'false' and x.get('wantedSharing') == 'false')
    stopped_pid = state()['pid']
    adb('install', '-r', str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk')); open_app()
    final = wait_state(lambda x: x.get('ready') == 'true' and x.get('control') == 'true' and x.get('wantedSharing') == 'false' and x.get('phase') == 'off')
    time.sleep(2)
    assert state().get('phase') == 'off' and state().get('wantedSharing') == 'false'
    (OUT/'normal-process-states.json').write_text(json.dumps(dict(afterRestart=fresh,afterExplicitStopRestart=final), indent=2)+'\n')
    with (OUT/'guards.log').open('wb') as log:
        guards = subprocess.run([*ADB, 'shell', 'am', 'instrument', '-w', '-r', '-e', 'class', 'app.farsail.android.RememberedAccessTest#scopeAndLateIntentGuards', '-e', 'memoryDriver', 'true', 'app.farsail.android.test/androidx.test.runner.AndroidJUnitRunner'], stdout=log, stderr=subprocess.STDOUT, timeout=150)
    assert guards.returncode == 0 and 'OK (1 test)' in (OUT/'guards.log').read_text(), 'Intent guards failed; inspect guards.log'
    summary = dict(verification='passed', actualProcessDeath=True, restartTrigger='same-certificate APK replacement', liveShareBeforeDeath=True, savedControlRestored=True, foregroundFreshConsent=True, cancelStopsRetry=True, explicitStopRetained=True, staleRestoreDenied=True, foreignDeviceScopeDenied=True, logoutClearsIntent=True, credentialsExcludedFromPreferences=True, productionTouched=False, requests=len(requests))
    (OUT/'verification.json').write_text(json.dumps(summary, indent=2)+'\n')
    (HISTORY/'verification.json').write_text(json.dumps(summary, indent=2)+'\n')
    print('PASS actual process restart, saved control, fresh foreground consent, cancellation, explicit stop, device scope and stale restore guards')
finally:
    (HISTORY/'latest.json').write_text(json.dumps(dict(directory=str(OUT), verification='passed' if (OUT/'verification.json').exists() else 'failed'), indent=2)+'\n')
    if stage1 and stage1.poll() is None:
        adb('shell', 'am', 'force-stop', 'app.farsail.android', check=False)
        stage1.wait(timeout=15)
    adb('reverse', '--remove', f'tcp:{port}', check=False)
    for key, value in old_settings.items():
        if value == 'null': adb('shell', 'settings', 'delete', 'secure', key, check=False)
        else: adb('shell', 'settings', 'put', 'secure', key, shlex.quote(value), check=False)
    server.shutdown(); server.server_close(); thread.join(timeout=5)
