#!/usr/bin/env python3
"""Run the real privileged helper in an unprivileged, disposable user namespace.

Uses a production-signed package, real signature verification and dpkg-deb. Only
package installation/query are replaced; no host package or Polkit state changes.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

helper, package, signature = (Path(p).resolve() for p in sys.argv[1:4])
version = subprocess.check_output(['dpkg-deb', '-f', str(package), 'Version'], text=True).strip()
with tempfile.TemporaryDirectory(prefix='farsail-helper-sandbox-') as temporary:
    root = Path(temporary)
    root.joinpath('passwd').write_text('root:x:0:0:root:/root:/bin/sh\nfarsail-test:x:1000:1000:test:/nonexistent:/bin/false\n')
    root.joinpath('nsswitch.conf').write_text('passwd: files\ngroup: files\n')
    root.joinpath('request').write_bytes((json.dumps({'signature': signature.read_text().strip()})+'\n').encode()+package.read_bytes())
    tampered = bytearray(root.joinpath('request').read_bytes())
    tampered[-1] ^= 1
    root.joinpath('tampered').write_bytes(tampered)
    root.joinpath('version').write_text(version)
    root.joinpath('dpkg').write_text('#!/bin/sh\nset -eu\ntest "$1" = --install\ntest -f "$2"\nprintf installed > /out/installed\n')
    root.joinpath('dpkg-query').write_text('''#!/bin/sh
set -eu
if [ -f /out/installed ]; then
    case "$2" in
        '-f=${Status}'*) printf 'install ok installed\\n';;
    esac
    cat /out/version
else
    printf '0.0.0'
fi
''')
    for name in ('dpkg', 'dpkg-query'):
        root.joinpath(name).chmod(0o755)
    # A new rootfs owned by namespace root avoids trusting any host /etc or /var.
    # No networking operation is exposed; avoid creating an unused network namespace
    # whose loopback setup is prohibited on some hosted runners.
    args = ['bwrap', '--unshare-user', '--uid', '0', '--gid', '0', '--unshare-pid', '--die-with-parent',
            '--ro-bind', '/usr', '/usr', '--symlink', 'usr/bin', '/bin', '--symlink', 'usr/lib', '/lib',
            '--symlink', 'usr/lib64', '/lib64', '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp',
            '--dir', '/var', '--dir', '/var/lib', '--dir', '/etc', '--dir', '/etc/polkit-1', '--dir', '/etc/polkit-1/rules.d',
            '--ro-bind', str(root/'passwd'), '/etc/passwd', '--ro-bind', str(root/'nsswitch.conf'), '/etc/nsswitch.conf',
            '--ro-bind', str(helper), '/helper', '--bind', str(root), '/out',
            '--ro-bind', str(root/'dpkg'), '/usr/bin/dpkg', '--ro-bind', str(root/'dpkg-query'), '/usr/bin/dpkg-query',
            '--tmpfs', '/usr/libexec', '--ro-bind', str(helper), '/usr/libexec/farsail-update-helper',
            '--ro-bind', str(Path('apps/desktop/src-tauri/linux/postinst').resolve()), '/postinst',
            '--ro-bind', str(Path('apps/desktop/src-tauri/linux/postrm').resolve()), '/postrm',
            '--setenv', 'PKEXEC_UID', '1000', '/bin/sh', '-ec', '''
# Rejection must happen before installation or authorization is persisted.
if /helper install < /out/tampered; then exit 1; fi
test ! -e /out/installed
test ! -e /etc/polkit-1/rules.d/49-farsail-update-1000.rules
/helper install < /out/request
test -f /out/installed
cp /etc/polkit-1/rules.d/49-farsail-update-1000.rules /out/generated.rules
# Replay and unexpected arguments must not become another install path.
if /helper install < /out/request; then exit 1; fi
if /helper install extra < /out/request; then exit 1; fi
# Upgrade maintenance preserves the rule; uninstall and explicit revoke remove it.
/helper revoke
test ! -e /etc/polkit-1/rules.d/49-farsail-update-1000.rules
/bin/sh /postinst configure
test -f /etc/polkit-1/rules.d/49-farsail-update-1000.rules
/bin/sh /postrm upgrade
test -f /etc/polkit-1/rules.d/49-farsail-update-1000.rules
/bin/sh /postrm remove
test ! -e /etc/polkit-1/rules.d/49-farsail-update-1000.rules
test ! -e /var/lib/farsail-updater
''']
    subprocess.run(args, check=True)
    rule = root.joinpath('generated.rules').read_text()
    # Execute the actual generated JavaScript for matching/nonmatching subjects.
    script = '''
const vm=require('node:vm'), assert=require('node:assert/strict');
let callback;vm.runInNewContext(JSON.parse(process.argv[1]),{polkit:{addRule:r=>callback=r,Result:{YES:'yes'}}});
const subject={user:'farsail-test',local:true,active:true};
const action={id:'app.farsail.update.install',lookup:()=>'/usr/libexec/farsail-update-helper'};
assert.equal(callback(action,subject),'yes');
for(const change of [{user:'other'},{local:false},{active:false}])assert.equal(callback(action,{...subject,...change}),undefined);
for(const change of [{id:'org.freedesktop.policykit.exec'},{id:'app.farsail.update.revoke'},{lookup:()=>'/usr/bin/dpkg'},{lookup:()=>'/tmp/farsail-update-helper'}])assert.equal(callback({...action,...change},subject),undefined);
'''
    subprocess.run(['node', '-e', script, json.dumps(rule)], check=True)
print('Real signed helper sandbox, tamper/replay/argument rejection, account rule and revoke: passed')
