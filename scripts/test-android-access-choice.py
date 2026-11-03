#!/usr/bin/env python3
"""Owned emulator only: save allow choices before authorization and process death."""
import http.server,json,os,pathlib,re,shlex,subprocess,threading,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
OUT=ROOT/'.local/android-choice-verification'/f'attempt-{time.time_ns()}'
OUT.mkdir(parents=True,exist_ok=True)
serial=os.environ.get('ANDROID_SERIAL','');assert re.fullmatch(r'emulator-[0-9]+',serial)
ADB=[str(pathlib.Path.home()/'.cache/codex-android/sdk/platform-tools/adb'),'-s',serial]
def adb(*args,check=True):return subprocess.run([*ADB,*args],capture_output=True,check=check,timeout=90)
assert adb('shell','getprop','ro.kernel.qemu').stdout.strip()==b'1'
assert adb('emu','avd','name').stdout.splitlines()[0]==b'farsail-memory-api35'
class Coordinator(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def answer(self):
        n=int(self.headers.get('Content-Length','0'));body=json.loads(self.rfile.read(n)) if n else {};path=self.path.split('?')[0]
        if path in ['/v1/auth/login','/v1/auth/refresh']:v=dict(access_token='synthetic-access',refresh_token='synthetic-refresh',session_id='10000000-0000-4000-8000-000000000001',access_expires_in=900)
        elif path=='/v1/me':v=dict(id='20000000-0000-4000-8000-000000000001',email='choice@example.invalid',email_verified=True,role='user')
        elif path=='/v1/devices/challenge':v=dict(challenge_id='30000000-0000-4000-8000-000000000001',message='synthetic-binding-challenge')
        elif path=='/v1/devices/bind':assert body['platform']=='android';v=dict(id='40000000-0000-4000-8000-000000000001',device_token='synthetic-device')
        elif path.endswith('/40000000-0000-4000-8000-000000000001'):v=dict(id='40000000-0000-4000-8000-000000000001',enabled=True,bound=True,name='Owned choice fixture',platform='android')
        elif path=='/v1/devices/heartbeat':v=dict(generation=1,expires_in=60)
        elif path in ['/v1/devices','/v1/remote/pending','/v1/remote/sessions']:v=[]
        else:v={}
        raw=json.dumps(v).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
    do_GET=answer
    do_POST=answer
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Coordinator);port=server.server_port
thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
old={k:adb('shell','settings','get','secure',k).stdout.decode().strip() for k in ['enabled_accessibility_services','accessibility_enabled']}
stage=None
def instrument(method,path):
    return subprocess.Popen([*ADB,'shell','am','instrument','-w','-r','-e','class',f'app.farsail.android.AccessChoiceTest#{method}','-e','choiceDriver','true','-e','fixtureUrl',f'http://127.0.0.1:{port}','app.farsail.android.test/androidx.test.runner.AndroidJUnitRunner'],stdout=path.open('wb'),stderr=subprocess.STDOUT)
def state():
    r=adb('shell','content','query','--uri','content://app.farsail.android.memory-fixture/state',check=False)
    return dict(re.findall(r'(\w+)=(true|false|[0-9]+|[a-z_]+)',r.stdout.decode())) if r.returncode==0 else {}
def wait(test):
    end=time.monotonic()+35
    while time.monotonic()<end:
        x=state()
        if test(x):return x
        time.sleep(.2)
    raise AssertionError(f'Normal process did not restore the choice: {x}')
try:
    adb('reverse',f'tcp:{port}',f'tcp:{port}')
    adb('install','-r',str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk'))
    adb('install','-r',str(ROOT/'apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk'))
    adb('shell','pm','clear','app.farsail.android')
    stage=instrument('controlChoiceSurvivesPendingSystemAuthorization',OUT/'pending.log')
    until=time.monotonic()+65;ready=None
    while time.monotonic()<until and stage.poll() is None:
        r=adb('shell','run-as','app.farsail.android','cat','files/choice-pending.json',check=False)
        if r.returncode==0:ready=json.loads(r.stdout);break
        time.sleep(.2)
    assert ready,'Allow control was not saved before authorization; inspect pending.log'
    adb('install','-r',str(ROOT/'apps/android/app/build/outputs/apk/debug/app-debug.apk'));stage.wait(timeout=15)
    adb('shell','am','start','-n','app.farsail.android/.MainActivity')
    pending=wait(lambda x:x.get('ready')=='true' and x.get('wantedControl')=='true' and x.get('control')=='false')
    assert pending['pid']!=str(ready['pid']) and pending['service']=='false'
    adb('shell','settings','put','secure','enabled_accessibility_services','app.farsail.android/app.farsail.android.ControlService')
    adb('shell','settings','put','secure','accessibility_enabled','1')
    restored=wait(lambda x:x.get('control')=='true' and x.get('wantedControl')=='true' and x.get('service')=='true')
    (OUT/'normal-process.json').write_text(json.dumps(dict(beforePermission=pending,afterPermission=restored),indent=2)+'\n')
    for method in ['grantedProjectionWaitsForRestoredAccountState','stopRejectsDeferredProjectionResult']:
        log=OUT/f'{method}.log';stage=instrument(method,log);assert stage.wait(timeout=120)==0
        assert 'OK (1 test)' in log.read_text(),f'{method} failed; inspect {log}'
    result=dict(verification='passed',controlIntentBeforePermission=True,actualProcessReplacement=True,permissionConnectionRestoresControl=True,sharingIntentBeforeConsent=True,deferredProjectionAfterReady=True,stopRejectsLateResult=True,productionTouched=False)
    (OUT/'verification.json').write_text(json.dumps(result,indent=2)+'\n');(OUT.parent/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
    print('PASS pre-authorization choices, actual process replacement, service restoration, delayed projection result and explicit Stop')
finally:
    if stage and stage.poll() is None:adb('shell','am','force-stop','app.farsail.android',check=False);stage.wait(timeout=15)
    for k,v in old.items():
        if v=='null':adb('shell','settings','delete','secure',k,check=False)
        else:adb('shell','settings','put','secure',k,shlex.quote(v),check=False)
    adb('reverse','--remove',f'tcp:{port}',check=False);server.shutdown();server.server_close();thread.join(timeout=5)
    (OUT.parent/'latest.json').write_text(json.dumps(dict(directory=str(OUT),verification='passed' if (OUT/'verification.json').exists() else 'failed'))+'\n')
