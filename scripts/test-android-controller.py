#!/usr/bin/env python3
"""Owned emulator only: real mobile viewer UI/JNI/QUIC, synthetic peers, no OS injection."""
import json
import os
from pathlib import Path
import re
import selectors
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.local/android-controller-verification' / f'attempt-{time.time_ns()}'
OUT.mkdir(parents=True, exist_ok=True)
serial = os.environ.get('ANDROID_SERIAL', '')
assert re.fullmatch(r'emulator-[0-9]+', serial), 'Explicit owned emulator required'
binary = Path.home()/'.cache/codex-android/sdk/platform-tools/adb'
ADB = [str(binary), '-s', serial]
def adb(*args, check=True):
    return subprocess.run([*ADB, *args], check=check, capture_output=True, timeout=90)
assert adb('shell', 'getprop', 'ro.kernel.qemu').stdout.strip() == b'1', 'Never run on a physical phone'
assert adb('emu', 'avd', 'name').stdout.splitlines()[0] == b'farsail-memory-api35', 'Use the dedicated FarSail test AVD'
fixture = None
port = None
test = None
try:
    err = (OUT/'fixture.log').open('wb')
    fixture = subprocess.Popen([str(ROOT/'target/debug/examples/controller_fixture')], stdout=subprocess.PIPE, stderr=err)
    selector = selectors.DefaultSelector(); selector.register(fixture.stdout, selectors.EVENT_READ)
    assert selector.select(30), 'Synthetic peer failed to start'
    line = fixture.stdout.readline(); selector.close()
    port = json.loads(line)['fixturePort']; assert isinstance(port,int) and 0 < port < 65536
    adb('reverse',f'tcp:{port}',f'tcp:{port}')
    adb('shell','wm','dismiss-keyguard')
    adb('install','-r',str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk'))
    adb('install','-r',str(ROOT/'apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk'))
    adb('shell','pm','clear','app.farsail.android')
    with (OUT/'instrumentation.log').open('wb') as log:
        test = subprocess.Popen([*ADB,'shell','am','instrument','-w','-r','-e','class','app.farsail.android.ControllerSmokeTest#desktopPhoneViewAndCancellationThroughRealMobileUi','-e','controllerDriver','true','-e','fixtureUrl',f'http://127.0.0.1:{port}','app.farsail.android.test/androidx.test.runner.AndroidJUnitRunner'],stdout=log,stderr=subprocess.STDOUT)
        assert test.wait(timeout=240)==0, 'Instrumentation process failed'
    result = (OUT/'instrumentation.log').read_text()
    assert 'OK (1 test)' in result, result
    for name in ['controller-result.json','controller-desktop.png','controller-phone.png']:
        (OUT/name).write_bytes(adb('exec-out','run-as','app.farsail.android','cat',f'files/{name}').stdout)
    with urllib.request.urlopen(f'http://127.0.0.1:{port}/fixture/report', timeout=5) as response:
        peers = json.load(response)
    (OUT/'synthetic-peers.json').write_text(json.dumps(peers,indent=2)+'\n')
    accepted = json.loads((OUT/'controller-result.json').read_text())
    assert all(accepted[x] for x in ['desktopControl','phoneControl','staticFrameControl','viewDeniedInput','rotatedTouch','backgroundClosed','pendingCancelled'])
    assert all(row['acks'] == 1 for row in peers['requests'] if row['phone'] and row['approved']), 'Phone fixture must stay on its single unchanged frame'
    assert not accepted['localSharingStarted']
    assert all(not row['open'] for row in peers['requests'])
    summary = dict(verification='passed',realMobileUi=True,realJni=True,authenticatedQuic=True,syntheticPeers=True,hostOsInjection=False,productionTouched=False,acceptance=accepted,evidence=str(OUT.relative_to(ROOT)))
    (OUT/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
    (OUT.parent/'verification.json').write_text(json.dumps(summary,indent=2)+'\n')
    print('PASS: mobile desktop/phone control, static phone frame beyond six seconds, actual touch/drag/wheel/text, view-only denial, local rotated touch, background close and pending cancellation')
finally:
    if test is not None and test.poll() is None:
        test.terminate()
    for name in ['controller-failure.png', 'controller-failure.xml']:
        result = adb('exec-out','run-as','app.farsail.android','cat',f'files/{name}',check=False)
        if result.returncode == 0:
            (OUT/name).write_bytes(result.stdout)
    if port:
        adb('reverse','--remove',f'tcp:{port}',check=False)
    if fixture is not None:
        fixture.terminate()
        try:
            fixture.wait(timeout=8)
        except subprocess.TimeoutExpired:
            fixture.kill(); fixture.wait()
    (OUT.parent/'latest.json').write_text(json.dumps({'attempt':str(OUT.relative_to(ROOT))})+'\n')
