"""Real TLS/HTTP Range fixture for persistent multi-part coordinator downloads.
Synthetic upgrader only; never accesses production or Docker.
"""
import importlib.util
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import threading
import time
import unittest

spec = importlib.util.spec_from_file_location('latest_fixture', Path(__file__).with_name('test-update-latest.py'))
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)
REAL_CURL = shutil.which('curl')


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.case = fixture_module.LatestTests(methodName='test_tied_created_time_uses_asset_id')
        self.case.setUp()
        c = self.case
        c.files['coordinator-images.tar.gz'] = os.urandom(9 * 1024 * 1024)
        c.replace_package()
        self.records = []
        self.mode = 'normal'
        self.active = 0
        self.peak = 0
        self.guard = threading.Lock()
        self.started = threading.Event()
        self.release = threading.Event()
        self.first_drop = True
        cert, key = c.folder/'cert.pem', c.folder/'key.pem'
        subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(key), '-out', str(cert), '-days', '1', '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost'], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                with owner.guard:
                    owner.active += 1
                    owner.peak = max(owner.peak, owner.active)
                    header = self.headers.get('Range')
                    owner.records.append(header)
                try:
                    body = owner.case.package
                    if header and owner.mode != 'no_range':
                        start_text, end_text = header.removeprefix('bytes=').split('-')
                        start, end = int(start_text), int(end_text) if end_text else len(body)-1
                        self.send_response(206)
                        reported = start + 1 if owner.mode == 'bad_range' else start
                        self.send_header('Content-Range', f'bytes {reported}-{end}/{len(body)}')
                        selected = body[start:end+1]
                    else:
                        start = 0
                        selected = body
                        self.send_response(200)
                    if owner.mode == 'corrupt' and start == 0:
                        selected = bytes([selected[0] ^ 1]) + selected[1:]
                    self.send_header('Content-Length', str(len(selected)))
                    self.end_headers()
                    # Make concurrent requests observable, not just CLI flags.
                    time.sleep(.1)
                    with owner.guard:
                        once = owner.mode == 'drop_once' and owner.first_drop
                        if once:
                            owner.first_drop = False
                    drop = once or (owner.mode == 'drop' and start >= 2097152)
                    if owner.mode == 'hold':
                        self.wfile.write(selected[:65536]); self.wfile.flush()
                        owner.started.set(); owner.release.wait(20)
                    elif drop:
                        self.wfile.write(selected[:65536]); self.wfile.flush()
                        self.connection.shutdown(socket.SHUT_RDWR)
                    else:
                        self.wfile.write(selected)
                except (ConnectionResetError, BrokenPipeError, OSError):
                    pass
                finally:
                    with owner.guard:
                        owner.active -= 1

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.daemon_threads = True
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(cert, key)
        self.server.socket = tls.wrap_socket(self.server.socket, server_side=True)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        # Only metadata is stubbed. All package transfers use the real curl and
        # locally trusted HTTPS server, including Range, disconnects and retries.
        wrapper = '''#!/usr/bin/env python3
import os,sys,subprocess,signal
args=sys.argv[1:]
if args[-1].endswith('.tar.gz'):
    process = subprocess.Popen([os.environ['FIXTURE_REAL_CURL'],*args[:-1],os.environ['FIXTURE_TLS_URL']])
    def stop(signum,frame):
        process.terminate(); raise SystemExit(128+signum)
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    try:
        result = process.wait()
    finally:
        if process.poll() is None: process.terminate()
        process.wait()
    raise SystemExit(result)
'''+fixture_module.CURL.split('\n', 1)[1]
        (c.bin/'curl').write_text(wrapper)
        c.env.update({'CURL_CA_BUNDLE': str(cert), 'NO_PROXY': 'localhost,127.0.0.1', 'FIXTURE_REAL_CURL': REAL_CURL, 'FIXTURE_TLS_URL': f'https://localhost:{self.server.server_port}/package'})
        self.cache = c.root/'.local/production/updater-cache'

    def tearDown(self):
        self.release.set()
        self.server.shutdown(); self.server.server_close(); self.thread.join()
        self.case.tearDown()

    def run_update(self):
        return self.case.run_update()

    def test_manual_single_transfer_resumes_after_disconnect(self):
        self.case.files['upgrade-coordinator.sh'] = b'set -eu\nprintf "done\\n" > "$2/upgrade-called"\n'
        self.case.rebuild()
        self.mode = 'drop_once'
        result = self.case.run_update(self.case.env['FIXTURE_TLS_URL'] + '/package.tar.gz', self.case.package_sha, str(self.case.root))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('bytes=65536-', self.records)
        self.assertIn('继续下载', result.stderr)
        self.assertTrue((self.case.root/'upgrade-called').exists())

    def test_parallel_ranges_and_full_sha_before_dispatch(self):
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertGreaterEqual(self.peak, 2)
        self.assertTrue(all(x and x.startswith('bytes=') for x in self.records))
        self.assertIn('4 路并发', result.stderr)
        self.assertIn('100.0%', result.stderr)
        self.assertIn('速度', result.stderr)
        self.assertIn('MB', result.stderr)
        self.assertTrue((self.case.root/'upgrade-called').exists())
        self.assertEqual(len(list(self.cache.glob('*/package.tar.gz'))), 1)

    def test_interrupted_response_automatically_resumes_remaining_range(self):
        self.mode = 'drop_once'
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        starts = [int(x.split('=')[1].split('-')[0]) for x in self.records]
        self.assertTrue(any(x % 2097152 == 65536 for x in starts), starts)

    def test_second_invocation_reuses_completed_pieces_and_partial_prefixes(self):
        self.mode = 'drop'
        first = self.run_update()
        self.assertNotEqual(first.returncode, 0)
        self.assertFalse((self.case.root/'upgrade-called').exists())
        self.assertIn('进度已保留', first.stderr)
        parts = list(self.cache.glob('*/pieces/*.part'))
        self.assertTrue(any(p.stat().st_size == 2097152 for p in parts))
        self.assertTrue(any(0 < p.stat().st_size < 2097152 for p in parts))
        self.mode = 'normal'; self.records.clear()
        second = self.run_update()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertNotIn('bytes=0-2097151', self.records)
        self.assertTrue(any(int(x.split('=')[1].split('-')[0]) % 2097152 > 0 for x in self.records))

    def test_server_ignoring_range_safely_falls_back_to_single_transfer(self):
        self.mode = 'no_range'
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('不支持分片', result.stderr)
        self.assertIn(None, self.records)
        self.assertTrue((self.case.root/'upgrade-called').exists())

    def test_wrong_content_range_never_reaches_upgrader(self):
        self.mode = 'bad_range'
        result = self.run_update()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.case.root/'upgrade-called').exists())
        self.assertFalse(list(self.cache.glob('*/package.tar.gz')))

    def test_corrupt_piece_fails_full_sha_and_clears_poisoned_cache(self):
        self.mode = 'corrupt'
        bad = self.run_update()
        self.assertNotEqual(bad.returncode, 0)
        self.assertIn('SHA256 mismatch', bad.stderr)
        self.assertFalse((self.case.root/'upgrade-called').exists())
        self.assertFalse(list(self.cache.glob('*/pieces')))
        self.mode = 'normal'; self.records.clear()
        valid = self.run_update()
        self.assertEqual(valid.returncode, 0, valid.stderr)
        self.assertIn('bytes=0-2097151', self.records)

    def test_verified_cache_avoids_network_on_retry(self):
        first = self.run_update(); self.assertEqual(first.returncode, 0, first.stderr)
        self.records.clear()
        second = self.run_update()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.records, [])
        self.assertIn('复用已下载并校验', second.stderr)

    def test_cancellation_preserves_uncommitted_response_and_releases_lock(self):
        self.mode = 'hold'
        log = self.case.folder/'cancel.log'
        (self.case.folder/'responses.json').write_text(json.dumps(self.case.responses))
        with log.open('w') as output:
            process = subprocess.Popen(['bash', str(fixture_module.SCRIPT), '--latest', str(self.case.root)], env=self.case.env, stdout=output, stderr=output)
            try:
                self.assertTrue(self.started.wait(10))
                deadline = time.monotonic() + 7
                while "KB/s" not in log.read_text() and time.monotonic() < deadline: time.sleep(.1)
                self.assertIn("KB/s", log.read_text())
                self.assertIn("剩余", log.read_text())
                # Lock is held while all parallel workers write their responses.
                # The first updater legitimately owns a live temporary directory.
                other = subprocess.run(["bash", str(fixture_module.SCRIPT), "--latest", str(self.case.root)], env=self.case.env, text=True, capture_output=True, timeout=10)
                self.assertNotEqual(other.returncode, 0)
                self.assertIn('Another process', other.stderr)
                process.terminate(); process.wait(timeout=10)
            finally:
                if process.poll() is None: process.kill(); process.wait()
                self.release.set()
        self.assertFalse((self.case.root/'upgrade-called').exists())
        self.assertTrue(any(p.stat().st_size > 0 for p in self.cache.glob('*/pieces/*.response')))
        self.mode = 'normal'; self.records.clear()
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(int(x.split('=')[1].split('-')[0]) % 2097152 == 65536 for x in self.records))

    def test_new_digest_does_not_append_old_pieces(self):
        self.mode = 'drop'; self.assertNotEqual(self.run_update().returncode, 0)
        before = set(p.name for p in self.cache.iterdir())
        self.case.files['update-online.sh'] = b'new candidate updater fixture\n'
        self.case.replace_package()
        self.mode = 'normal'; self.records.clear()
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('bytes=0-2097151', self.records)
        self.assertTrue(before < set(p.name for p in self.cache.iterdir()))


if __name__ == '__main__':
    unittest.main(verbosity=2)
