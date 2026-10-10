"""Local trusted HTTPS fixture; no production or Docker mutations."""
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import ssl
import subprocess
import tarfile
import tempfile
import threading

SCRIPT = Path(__file__).with_name("update-online.sh").resolve()
with tempfile.TemporaryDirectory(prefix="farsail-online-test-") as temp:
    folder = Path(temp)
    root = folder / "deployment"
    (root / ".local/production").mkdir(parents=True)
    (root / "deploy/production").mkdir(parents=True)
    (root / ".local/production/compose.env").write_text("fixture only\n")
    (root / "deploy/production/compose.yaml").write_text("fixture only\n")
    release = folder / "release"
    release.mkdir()
    marker = root / "upgrade-called"
    upgrader = release / "upgrade-coordinator.sh"
    upgrader.write_text('set -eu\nprintf "%s\\n" "$2" > "$2/upgrade-called"\n')
    checksum = hashlib.sha256(upgrader.read_bytes()).hexdigest()
    (release / "SHA256SUMS").write_text(f"{checksum}  upgrade-coordinator.sh\n")
    archive = folder / "package.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        bundle.add(release, arcname=".")
    expected = hashlib.sha256(archive.read_bytes()).hexdigest()
    cert, key = folder / "cert.pem", folder / "key.pem"
    subprocess.run([
        "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
        "-keyout", str(key), "-out", str(cert), "-days", "1",
        "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
    ], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(folder), **kwargs)

        def handle(self):
            try:
                super().handle()
            except (ConnectionResetError, BrokenPipeError):
                # Expected when the client refuses a certificate or redirect.
                pass

        def do_GET(self):
            if self.path == "/redirect":
                self.send_response(302)
                self.send_header("Location", "http://localhost/package.tar.gz")
                self.end_headers()
            else:
                super().do_GET()

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(cert, key)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"https://localhost:{server.server_port}/package.tar.gz"
    env = {**os.environ, "CURL_CA_BUNDLE": str(cert)}
    env.pop("FARSAIL_STATE_DIR", None)
    env["NO_PROXY"] = "localhost,127.0.0.1"

    def run(address, digest):
        return subprocess.run(
            ["bash", str(SCRIPT), address, digest, str(root)],
            env=env, text=True, capture_output=True, timeout=30,
        )

    try:
        valid = run(url, expected)
        assert valid.returncode == 0, valid.stderr
        assert marker.read_text().strip() == str(root)
        marker.unlink()
        wrong = run(url, "0" * 64)
        assert wrong.returncode != 0 and "SHA256 mismatch" in wrong.stderr
        assert not marker.exists()
        insecure = run(url.replace("https://", "http://"), expected)
        assert insecure.returncode != 0 and "HTTPS" in insecure.stderr
        assert not marker.exists()
        downgrade = run(url.replace("/package.tar.gz", "/redirect"), expected)
        assert downgrade.returncode != 0 and "http" in downgrade.stderr.lower()
        assert not marker.exists()
        untrusted_env = {k: v for k, v in env.items() if k != "CURL_CA_BUNDLE"}
        untrusted = subprocess.run(
            ["bash", str(SCRIPT), url, expected, str(root)],
            env=untrusted_env, text=True, capture_output=True, timeout=30,
        )
        assert untrusted.returncode != 0 and "certificate" in untrusted.stderr.lower()
        assert not marker.exists()
        assert not list((root / ".local").glob("online-update.*"))
        print("PASS trusted HTTPS download, exact SHA256, upgrade dispatch, mismatch/HTTP/downgrade/untrusted TLS refusal and cleanup; synthetic upgrader only.")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
