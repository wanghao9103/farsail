#!/usr/bin/env python3
"""Inspect the privilege boundary in the actual Debian package, without installing it."""
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import xml.etree.ElementTree as ET

package = sys.argv[1]
data = subprocess.check_output(['dpkg-deb', '--fsys-tarfile', package])
with tarfile.open(fileobj=io.BytesIO(data)) as archive:
    entries = {entry.name.removeprefix('./').rstrip('/'): entry for entry in archive}
    for name in ('usr/libexec/farsail-update-helper', 'usr/share/polkit-1/actions/app.farsail.update.policy'):
        entry = entries[name]
        assert entry.isfile() and entry.uid == entry.gid == 0 and not entry.mode & 0o022, name
        parent = name.rsplit('/', 1)[0]
        while parent:
            if parent in entries:
                directory = entries[parent]
                assert directory.isdir() and directory.uid == directory.gid == 0 and not directory.mode & 0o022, parent
            parent = parent.rsplit('/', 1)[0] if '/' in parent else ''
    helper = entries['usr/libexec/farsail-update-helper']
    assert helper.mode & 0o111 and not helper.mode & 0o6000
    policy_bytes = archive.extractfile(entries['usr/share/polkit-1/actions/app.farsail.update.policy']).read()
    assert policy_bytes == Path('apps/desktop/src-tauri/linux/app.farsail.update.policy').read_bytes()
    policy = ET.fromstring(policy_bytes)
    action, = policy.findall('action')
    assert action.attrib['id'] == 'app.farsail.update.install'
    assert action.findtext('defaults/allow_active') == 'auth_admin'
    assert action.findtext('defaults/allow_any') == action.findtext('defaults/allow_inactive') == 'no'
    assert {a.attrib['key']: a.text for a in action.findall('annotate')} == {
        'org.freedesktop.policykit.exec.path': '/usr/libexec/farsail-update-helper',
        'org.freedesktop.policykit.exec.argv1': 'install',
    }
control = subprocess.check_output(['dpkg-deb', '--ctrl-tarfile', package])
with tarfile.open(fileobj=io.BytesIO(control)) as archive:
    for name in ('postinst', 'postrm'):
        entry = next(e for e in archive if e.name.removeprefix('./') == name)
        assert entry.isfile() and entry.uid == entry.gid == 0 and entry.mode & 0o111 and not entry.mode & 0o022
        script = archive.extractfile(entry).read()
        assert script == Path('apps/desktop/src-tauri/linux', name).read_bytes()
        subprocess.run(['/bin/sh', '-n'], input=script, check=True)
print('Debian signed update helper, policy and lifecycle: passed')
