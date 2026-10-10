"""Offline coordinator discovery/dispatch checks. No production or real Docker."""
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('update-online.sh').resolve()
API = 'https://api.github.com/repos/wanghao9103/farsail'
DOWNLOAD = 'https://github.com/wanghao9103/farsail/releases/download/'
REVISION = 'd' * 40
OLDER = 'a' * 40
DIGEST = 'sha256:' + 'b' * 64
NAME = 'FarSail_coordinator_files_d21b298_linux_amd64.tar.gz'
TAG = 'coordinator-files-d21b298'

CURL = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args=sys.argv[1:]
assert args[0]=='-q', 'curl config must be disabled'
assert args[args.index('--proto')+1]=='=https'
assert args[args.index('--proto-redir')+1]=='=https'
assert '--max-time' in args and '--retry-max-time' in args
url=args[-1]
fixture=Path(os.environ['LATEST_FIXTURE'])
with (fixture/'requests').open('a') as f:f.write(url+'\\n')
responses=json.loads((fixture/'responses.json').read_text())
entry=responses.get(url)
if entry is None:sys.exit(22)
content=(fixture/entry).read_bytes()
if len(content)>int(args[args.index('--max-filesize')+1]):sys.exit(63)
Path(args[args.index('--output')+1]).write_bytes(content)
'''
DOCKER = '''#!/usr/bin/env python3
import os
from pathlib import Path
Path(os.environ['LATEST_FIXTURE'],'docker-called').write_text('unexpected')
raise SystemExit(98)
'''
UPGRADER = '''#!/usr/bin/env bash
set -euo pipefail
FARSAIL_ONLINE_UPDATE_PROTOCOL=1
[[ $# == 4 && $3 == --online-selection && -f $4 ]]
[[ -f $1/coordinator-release.json && -f $1/coordinator-images.tar.gz ]]
cp "$4" "$2/selection-called.json"
if [[ -f $2/already-current-fixture ]]; then
  printf 'Coordinator already updated and healthy\\n'
else
  printf '%s\\n' "$2" > "$2/upgrade-called"
fi
'''


def sha(content):
    return hashlib.sha256(content).hexdigest()


def archive(files, extra=None):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode='w:gz') as bundle:
        for name, content in files.items():
            header = tarfile.TarInfo(name)
            header.size = len(content)
            bundle.addfile(header, io.BytesIO(content))
        if extra is not None:
            header, content = extra
            bundle.addfile(header, io.BytesIO(content) if content else None)
    return buffer.getvalue()


class LatestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='farsail-latest-test-')
        self.folder = Path(self.temp.name)
        self.root = self.folder / 'deployment with spaces'
        (self.root / '.local/production').mkdir(parents=True)
        (self.root / 'deploy/production').mkdir(parents=True)
        (self.root / '.local/production/compose.env').write_text('COORDINATOR_IMAGE=farsail/coordinator:verification-code-20261009\n')
        (self.root / 'deploy/production/compose.yaml').write_text('fixture only\n')
        self.bin = self.folder / 'bin'
        self.bin.mkdir()
        for name, content in [('curl', CURL), ('docker', DOCKER)]:
            path = self.bin / name
            path.write_text(content)
            path.chmod(0o755)
        self.tmp = self.folder / 'tmp'
        self.tmp.mkdir()
        self.env = {**os.environ, 'PATH': str(self.bin)+os.pathsep+os.environ['PATH'],
                    'LATEST_FIXTURE': str(self.folder), 'TMPDIR': str(self.tmp)}
        self.env.pop('FARSAIL_STATE_DIR', None)
        self.responses = {}
        self.file_count = 0
        self.metadata = {'online_update_protocol': 1, 'source_revision': REVISION,
                         'platform': 'linux/amd64', 'image': 'farsail/coordinator:files-d21b298',
                         'config_digest': DIGEST}
        self.files = {'coordinator-images.tar.gz': b'fixture image; not loaded',
                      'coordinator-release.json': json.dumps(self.metadata).encode(),
                      'upgrade-coordinator.sh': UPGRADER.encode(),
                      'update-online.sh': b'fixture persistent updater\n',
                      'install-online-updater.sh': b'fixture installer\n'}
        self.rebuild()
        self.selected = self.release(TAG, NAME, 202, '2026-10-10T05:00:00Z')
        # A newer client release must not interfere with older server release assets.
        client = self.release('client-v9.0.0', 'FarSail_9.0.0_amd64.deb', 999,
                              '2026-10-11T05:00:00Z')
        self.releases = [client, self.selected]
        self.install_pages()

    def tearDown(self):
        self.temp.cleanup()

    def file(self, url, content):
        target = f'response-{self.file_count}'
        self.file_count += 1
        (self.folder / target).write_bytes(content)
        self.responses[url] = target

    def rebuild(self, extra=None):
        self.files['SHA256SUMS'] = ''.join(f'{sha(body)}  {name}\n'
             for name, body in self.files.items() if name != 'SHA256SUMS').encode()
        self.package = archive(self.files, extra)
        self.package_sha = sha(self.package)

    def release(self, tag, name, identity, timestamp, *, draft=False, checksum=True):
        url = DOWNLOAD + tag + '/' + name
        sidecar = (self.package_sha + '  ' + name + '\n').encode()
        asset = {'name': name, 'id': identity, 'created_at': timestamp,
                 'state': 'uploaded', 'size': len(self.package), 'browser_download_url': url,
                 'digest': 'sha256:' + self.package_sha}
        assets = [asset]
        self.file(url, self.package)
        if checksum:
            assets.append({'name': name+'.sha256', 'id': identity+10000,
                'created_at': timestamp, 'state': 'uploaded', 'size': len(sidecar),
                'browser_download_url': url+'.sha256', 'digest': 'sha256:'+sha(sidecar)})
            self.file(url+'.sha256', sidecar)
        return {'tag_name': tag, 'draft': draft, 'prerelease': True,
                'published_at': '2026-10-09T00:00:00Z', 'assets': assets}

    def install_pages(self, pages=None):
        if pages is None:
            pages = [self.releases]
        for page, entries in enumerate(pages, 1):
            self.file(API+f'/releases?per_page=100&page={page}', json.dumps(entries).encode())

    def replace_package(self, extra=None):
        self.rebuild(extra)
        self.selected = self.release(TAG, NAME, 202, '2026-10-10T05:00:00Z')
        self.releases = [self.selected]
        self.install_pages()

    def run_update(self, *args):
        (self.folder / 'responses.json').write_text(json.dumps(self.responses))
        result = subprocess.run(['bash', str(SCRIPT), *(args or ('--latest', str(self.root)))],
                    env=self.env, text=True, capture_output=True, timeout=20)
        self.assertFalse((self.folder / 'docker-called').exists(),
                         'outer discovery must delegate Docker/no-op decisions')
        self.assertFalse(list(self.tmp.glob('farsail-online-update.*')), 'temporary files leaked')
        return result

    def assert_refused(self, text):
        result = self.run_update()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(text, result.stderr)
        self.assertFalse((self.root / 'upgrade-called').exists())
        self.assertFalse((self.root / 'selection-called.json').exists())

    def requests(self):
        path = self.folder / 'requests'
        return path.read_text().splitlines() if path.exists() else []

    def test_client_release_is_ignored_and_complete_preview_delegates(self):
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'upgrade-called').read_text().strip(), str(self.root))
        selection = json.loads((self.root / 'selection-called.json').read_text())
        self.assertEqual(selection, {'repository': 'wanghao9103/farsail',
             'asset_id': 202, 'created_at': '2026-10-10T05:00:00Z', 'sha256': self.package_sha,
             'tag': TAG, 'name': NAME, 'url': DOWNLOAD+TAG+'/'+NAME,
             'base_source_revision': None, 'source_comparison': 'legacy'})
        self.assertTrue(all('/latest' not in url for url in self.requests()))

    def test_same_release_second_coordinator_asset_is_selected_by_asset_time(self):
        older = self.release(TAG, 'FarSail_coordinator_previous_linux_amd64.tar.gz',
                             101, '2026-10-09T09:00:00Z')
        self.selected['assets'] = older['assets'] + self.selected['assets']
        self.install_pages()
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((self.root/'selection-called.json').read_text())['asset_id'], 202)

    def test_tied_created_time_uses_asset_id(self):
        other = self.release('coordinator-other', 'FarSail_coordinator_other_linux_amd64.tar.gz',
                             201, '2026-10-10T05:00:00Z')
        self.releases.append(other)
        self.install_pages()
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((self.root/'selection-called.json').read_text())['asset_id'], 202)

    def test_newest_missing_sha_never_falls_back_to_older_pair(self):
        other = self.release(TAG, 'FarSail_coordinator_unfinished_linux_amd64.tar.gz',
                             303, '2026-10-11T05:00:00Z', checksum=False)
        self.selected['assets'] += other['assets']
        self.install_pages()
        self.assert_refused('incomplete')

    def test_historical_mail_packages_are_not_automatic_candidates(self):
        self.releases = [self.release('coordinator-ubuntu-login-20261009',
               'FarSail_coordinator_verification-code_20261009.tar.gz', 10, '2026-10-09T09:00:00Z')]
        self.install_pages()
        self.assert_refused('No published coordinator package')

    def test_draft_and_wrong_release_channel_are_ignored(self):
        self.selected['draft'] = True
        other = self.release('client-updates', NAME, 404, '2026-10-11T00:00:00Z')
        self.releases.append(other)
        self.install_pages()
        self.assert_refused('No published coordinator package')

    def test_check_fetches_only_api_and_sidecar(self):
        result = self.run_update('--latest', str(self.root), '--check')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Docker were not changed', result.stdout)
        self.assertFalse((self.root/'selection-called.json').exists())
        self.assertNotIn(DOWNLOAD+TAG+'/'+NAME, self.requests())
        self.assertIn(DOWNLOAD+TAG+'/'+NAME+'.sha256', self.requests())

    def test_check_shorthand(self):
        result = self.run_update('--check', str(self.root))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_custom_state_without_root_local_directory(self):
        custom = self.folder/'private custom state'
        (self.root/'.local/production/compose.env').rename(self.folder/'old-compose.env')
        (self.root/'.local/production').rmdir()
        (self.root/'.local').rmdir()
        custom.mkdir()
        (custom/'compose.env').write_text('fixture\n')
        self.env['FARSAIL_STATE_DIR'] = str(custom)
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root/'.local').exists())

    def test_already_current_is_delegated_to_locked_upgrader(self):
        (self.root/'already-current-fixture').touch()
        (self.root/'.local/production/coordinator-release.json').write_text(json.dumps({'source_revision': REVISION}))
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('already updated', result.stdout)
        self.assertTrue((self.root/'selection-called.json').exists())
        self.assertFalse((self.root/'upgrade-called').exists())
        self.assertFalse(any('/compare/' in url for url in self.requests()))

    def test_paginated_discovery_reaches_server_on_second_page(self):
        filler = [{'tag_name':'client-x','draft':False,'published_at':'2026-10-10T00:00:00Z','assets':[]}]*100
        self.install_pages([filler, [self.selected]])
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(API+'/releases?per_page=100&page=2', self.requests())

    def test_discovery_limit_is_failure_not_partial_latest(self):
        filler = [{'tag_name':'client-x','draft':False,'published_at':'2026-10-10T00:00:00Z','assets':[]}]*100
        self.install_pages([filler, filler, filler])
        self.assert_refused('exceeded 300')
        self.assertEqual(len(self.requests()), 3)

    def test_api_http_failure_or_non_array_is_refused(self):
        self.file(API+'/releases?per_page=100&page=1', b'{"message":"rate limit exceeded"}')
        self.assert_refused('Invalid GitHub releases')

    def test_api_request_failure_reports_safe_retry_without_fallback(self):
        del self.responses[API+'/releases?per_page=100&page=1']
        self.assert_refused('GitHub API request failed')
        self.assertEqual(self.requests(), [API+'/releases?per_page=100&page=1'])

    def test_download_url_must_be_exact_official_repository(self):
        self.selected['assets'][0]['browser_download_url'] = 'https://example.test/'+NAME
        self.install_pages()
        self.assert_refused('URL or GitHub digest')

    def test_checksum_url_must_match_same_tag_and_name(self):
        self.selected['assets'][1]['browser_download_url'] = DOWNLOAD+'coordinator-wrong/'+NAME+'.sha256'
        self.install_pages()
        self.assert_refused('URL or GitHub digest')

    def test_package_size_is_bounded_before_download(self):
        self.selected['assets'][0]['size'] = 268435457
        self.install_pages()
        self.assert_refused('exceeds the size limit')
        self.assertNotIn(DOWNLOAD+TAG+'/'+NAME, self.requests())

    def test_sidecar_size_is_bounded_before_download(self):
        self.selected['assets'][1]['size'] = 4097
        self.install_pages()
        self.assert_refused('exceeds the size limit')

    def test_malformed_asset_identity_is_not_silently_skipped(self):
        self.selected['assets'][0]['created_at'] = '2026-02-31T00:00:00Z'
        self.install_pages()
        self.assert_refused('Invalid coordinator release metadata')

    def test_sidecar_exact_filename_is_required(self):
        content=(self.package_sha+'  wrong.tar.gz\n').encode()
        self.file(DOWNLOAD+TAG+'/'+NAME+'.sha256', content)
        self.selected['assets'][1]['digest']='sha256:'+sha(content)
        self.install_pages()
        self.assert_refused('different package')

    def test_sidecar_multiple_checksum_records_are_refused(self):
        content=(self.package_sha+'  '+NAME+'\n'+'0'*64+'  other.tar.gz\n').encode()
        self.file(DOWNLOAD+TAG+'/'+NAME+'.sha256', content)
        self.selected['assets'][1]['digest']='sha256:'+sha(content)
        self.install_pages()
        self.assert_refused('one SHA256')

    def test_sidecar_must_match_its_github_digest(self):
        self.selected['assets'][1]['digest']='sha256:'+'0'*64
        self.install_pages()
        self.assert_refused('Checksum file does not match')

    def test_sidecar_sha_must_match_package_github_digest(self):
        self.selected['assets'][0]['digest']='sha256:'+'0'*64
        self.install_pages()
        self.assert_refused('disagrees with its GitHub digest')

    def test_null_github_digests_still_require_matching_sidecar_and_bytes(self):
        for asset in self.selected['assets']:asset['digest']=None
        self.install_pages()
        result=self.run_update()
        self.assertEqual(result.returncode,0,result.stderr)

    def test_invalid_github_digest_format_is_refused(self):
        self.selected['assets'][0]['digest']='md5:notsha256'
        self.install_pages()
        self.assert_refused('URL or GitHub digest')

    def test_downloaded_package_must_match_outer_checksum(self):
        self.file(DOWNLOAD+TAG+'/'+NAME, b'corrupted package')
        self.assert_refused('Package SHA256 mismatch')

    def test_missing_protocol_marker_is_refused(self):
        self.files['upgrade-coordinator.sh']=UPGRADER.replace('FARSAIL_ONLINE_UPDATE_PROTOCOL=1','').encode()
        self.replace_package()
        self.assert_refused('does not support automatic-update protocol')

    def test_metadata_protocol_and_complete_source_are_required(self):
        self.metadata['source_revision']='working-tree-local'
        self.files['coordinator-release.json']=json.dumps(self.metadata).encode()
        self.replace_package()
        self.assert_refused('metadata does not satisfy')

    def test_installer_must_have_internal_checksum(self):
        self.files['SHA256SUMS']=b''
        self.rebuild()
        content=self.files['SHA256SUMS'].decode()
        self.files['SHA256SUMS']=''.join(line+'\n' for line in content.splitlines() if not line.endswith('install-online-updater.sh')).encode()
        self.package=archive(self.files);self.package_sha=sha(self.package)
        self.selected=self.release(TAG,NAME,202,'2026-10-10T05:00:00Z')
        self.releases=[self.selected];self.install_pages()
        self.assert_refused('checksum missing for install-online-updater')

    def test_internal_file_corruption_is_refused(self):
        self.files['update-online.sh']=b'changed after checksum'
        self.package=archive(self.files);self.package_sha=sha(self.package)
        self.selected=self.release(TAG,NAME,202,'2026-10-10T05:00:00Z')
        self.releases=[self.selected];self.install_pages()
        self.assert_refused('did NOT match')

    def test_manual_pinned_mode_preserves_safe_relative_regular_files(self):
        manual = {'upgrade-coordinator.sh': b'set -eu\nprintf "%s\\n" "$2" > "$2/upgrade-called"\n',
                  'docs/README.md': b'fixture nested readme\n'}
        manual['SHA256SUMS']=''.join(f'{sha(body)}  {name}\n' for name,body in manual.items()).encode()
        body=archive(manual)
        url='https://example.test/pinned-package.tar.gz'
        self.file(url,body)
        result=self.run_update(url,sha(body),str(self.root))
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.root/'upgrade-called').read_text().strip(),str(self.root))
        self.assertFalse((self.root/'selection-called.json').exists())

    def test_source_compare_ahead_proof_is_passed_with_snapshot(self):
        (self.root/'.local/production/coordinator-release.json').write_text(json.dumps({'source_revision':OLDER}))
        self.file(API+'/compare/'+OLDER+'...'+REVISION,
                  json.dumps({'base_commit':{'sha':OLDER},'status':'ahead','ahead_by':1,'behind_by':0}).encode())
        result=self.run_update()
        self.assertEqual(result.returncode,0,result.stderr)
        selection=json.loads((self.root/'selection-called.json').read_text())
        self.assertEqual(selection['base_source_revision'],OLDER)
        self.assertEqual(selection['source_comparison'],'ahead')

    def test_source_compare_behind_or_divergent_is_refused(self):
        for status,ahead,behind in [('behind',0,1),('diverged',1,1)]:
            with self.subTest(status=status):
                (self.root/'.local/production/coordinator-release.json').write_text(json.dumps({'source_revision':OLDER}))
                self.file(API+'/compare/'+OLDER+'...'+REVISION,
                  json.dumps({'base_commit':{'sha':OLDER},'status':status,'ahead_by':ahead,'behind_by':behind}).encode())
                self.assert_refused('automatic downgrade/divergence')

    def test_source_compare_network_failure_is_not_bypassed(self):
        (self.root/'.local/production/coordinator-release.json').write_text(json.dumps({'source_revision':OLDER}))
        self.assert_refused('GitHub API request failed')
        self.assertIn(API+'/compare/'+OLDER+'...'+REVISION,self.requests())

    def test_source_compare_wrong_base_is_refused(self):
        (self.root/'.local/production/coordinator-release.json').write_text(json.dumps({'source_revision':OLDER}))
        self.file(API+'/compare/'+OLDER+'...'+REVISION,
                  json.dumps({'base_commit':{'sha':'e'*40},'status':'ahead','ahead_by':1,'behind_by':0}).encode())
        self.assert_refused('automatic downgrade/divergence')

    def test_archive_traversal_absolute_links_and_duplicates_are_refused(self):
        for name,kind in [('../escaped','file'),('/absolute','file'),('symlink','link'),('hardlink','hard'),
                          ('upgrade-coordinator.sh','duplicate'),('subdir/file','file')]:
            with self.subTest(name=name):
                header=tarfile.TarInfo(name)
                if kind=='link':header.type=tarfile.SYMTYPE;header.linkname='../escaped';body=b''
                elif kind=='hard':header.type=tarfile.LNKTYPE;header.linkname='upgrade-coordinator.sh';body=b''
                else:body=b'unsafe';header.size=len(body)
                self.replace_package((header,body))
                self.assert_refused('')
                self.assertFalse((self.tmp/'escaped').exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
