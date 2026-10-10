"""Verified updater-only installation; temporary fixtures, no Docker or network."""
import hashlib
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("install-online-updater.sh").resolve()
TOOLS = ("realpath", "sha256sum", "awk", "flock", "mktemp", "mkdir", "install", "ln", "cp", "mv", "rm", "chmod")


class InstallOnlineUpdaterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="farsail-updater-install-")
        self.folder = Path(self.temp.name)
        self.root = self.folder / "deployment space ' quote $ literal"
        self.state = self.folder / "private state space ' quote"
        (self.root / "deploy/production").mkdir(parents=True)
        self.state.mkdir()
        (self.root / "deploy/production/compose.yaml").write_text("fixture deployment\n")
        self.private = {
            "compose.env": b"synthetic deployment selection\n",
            "coordinator.env": b"synthetic config fixture\n",
            "settings.json": b'{"fixture":"preserve"}\n',
        }
        for name, content in self.private.items():
            (self.state / name).write_bytes(content)
            (self.state / name).chmod(0o600)
        self.release = self.folder / "updater only bundle"
        self.release.mkdir()
        shutil.copyfile(SCRIPT, self.release / SCRIPT.name)
        self.updater = self.release / "update-online.sh"
        self.updater.write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\n'
            'printf "%s\\n" "$FARSAIL_STATE_DIR" "$@" > "$2/updater-called"\n'
        )
        self.write_checksums()
        self.env = {**os.environ, "FARSAIL_STATE_DIR": str(self.state)}

    def tearDown(self):
        self.temp.cleanup()

    def write_checksums(self):
        self.digests = {
            name: hashlib.sha256((self.release / name).read_bytes()).hexdigest()
            for name in ("update-online.sh", SCRIPT.name)
        }
        (self.release / "SHA256SUMS").write_text(
            "".join(f"{digest}  {name}\n" for name, digest in self.digests.items())
        )

    def run_installer(self, env=None, script=None):
        return subprocess.run(
            ["/bin/bash", str(script or self.release / SCRIPT.name), str(self.release), str(self.root)],
            env=env or self.env, text=True, capture_output=True, timeout=10,
        )

    def assert_private_preserved(self):
        for name, content in self.private.items():
            path = self.state / name
            self.assertEqual(path.read_bytes(), content)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertFalse((self.root / ".local").exists())
        self.assertFalse(list(self.root.glob(".updater-install.*")))
        self.assertFalse(list((self.state / "tools").glob(".updater-install.*")))

    def assert_installed(self):
        entry = self.root / "farsail-update"
        updater = self.state / "tools/update-online.sh"
        self.assertTrue(updater.is_symlink())
        self.assertEqual(updater.read_bytes(), self.updater.read_bytes())
        self.assertEqual(stat.S_IMODE(updater.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(entry.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((self.state / "tools").stat().st_mode), 0o700)
        return entry

    def test_updater_only_bundle_installs_fixed_custom_paths_and_check_wrapper(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        entry = self.assert_installed()
        for args in ([], ["--check"]):
            called = subprocess.run(
                [str(entry), *args], env={**self.env, "FARSAIL_STATE_DIR": "/ignored-caller-state"},
                text=True, capture_output=True, timeout=10,
            )
            self.assertEqual(called.returncode, 0, called.stderr)
            self.assertEqual(
                (self.root / "updater-called").read_text().splitlines(),
                [str(self.state), "--latest", str(self.root), *args],
            )
        rejected = subprocess.run([str(entry), "/another-root"], capture_output=True, timeout=10)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertEqual(self.run_installer(script=SCRIPT).returncode, 0)
        self.assertEqual(len(list((self.state / "tools/online-updaters").iterdir())), 1)
        self.assert_private_preserved()

    def test_missing_or_ambiguous_checksum_and_changed_installer_refuse_before_install(self):
        original = (self.release / "SHA256SUMS").read_text()
        for contents in (
            f"{self.digests['update-online.sh']}  update-online.sh\n",
            original + f"{self.digests['update-online.sh']}  update-online.sh\n",
            original.replace(self.digests["update-online.sh"], "0" * 64),
        ):
            (self.release / "SHA256SUMS").write_text(contents)
            result = self.run_installer()
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.root / "farsail-update").exists())
            self.assertFalse((self.state / "tools").exists())
        (self.release / "SHA256SUMS").write_text(original)
        changed_runner = self.folder / "different-installer.sh"
        changed_runner.write_bytes(SCRIPT.read_bytes() + b"\n# different running installer\n")
        different = self.run_installer(script=changed_runner)
        self.assertNotEqual(different.returncode, 0)
        self.assertIn("Running installer does not match", different.stderr)
        with (self.release / SCRIPT.name).open("a") as installer:
            installer.write("\n# changed after checksum\n")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Checksum mismatch: install-online-updater.sh", result.stderr)
        self.assert_private_preserved()

    def test_missing_tool_and_missing_deployment_or_relative_state_refuse(self):
        tool_path = self.folder / "restricted tools"
        tool_path.mkdir()
        for tool in TOOLS:
            if tool != "flock":
                (tool_path / tool).symlink_to(shutil.which(tool))
        result = self.run_installer({**self.env, "PATH": str(tool_path)})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing flock", result.stderr)
        relative = self.run_installer({**self.env, "FARSAIL_STATE_DIR": "relative-state"})
        self.assertNotEqual(relative.returncode, 0)
        self.assertIn("absolute", relative.stderr)
        (self.state / "compose.env").rename(self.state / "saved-compose.env")
        missing = self.run_installer()
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("Existing deployment", missing.stderr)
        (self.state / "saved-compose.env").rename(self.state / "compose.env")
        self.assertFalse((self.state / "tools").exists())
        self.assert_private_preserved()

    def test_failed_entry_replace_restores_original_updater_link_and_wrapper(self):
        initial = self.run_installer()
        self.assertEqual(initial.returncode, 0, initial.stderr)
        entry = self.assert_installed()
        previous_entry = entry.read_bytes()
        updater_link = self.state / "tools/update-online.sh"
        previous_target = os.readlink(updater_link)
        previous_updater = updater_link.read_bytes()
        self.updater.write_text(self.updater.read_text() + "\n# new updater fixture\n")
        self.write_checksums()
        mocks = self.folder / "mock tools"
        mocks.mkdir()
        real_mv = shutil.which("mv")
        mock_mv = mocks / "mv"
        mock_mv.write_text(
            '#!/bin/bash\nset -euo pipefail\n'
            'if [[ ${@: -1} == "$FARSAIL_TEST_REPLACE_TARGET" && ! -e $FARSAIL_TEST_FAILURE_MARKER ]]; then\n'
            '  : > "$FARSAIL_TEST_FAILURE_MARKER"; exit 70\nfi\n'
            f'exec "{real_mv}" "$@"\n'
        )
        mock_mv.chmod(0o700)
        failure_marker = self.folder / "replace-failed"
        failed = self.run_installer({
            **self.env, "PATH": f"{mocks}:{self.env['PATH']}",
            "FARSAIL_TEST_REPLACE_TARGET": str(entry),
            "FARSAIL_TEST_FAILURE_MARKER": str(failure_marker),
        })
        self.assertNotEqual(failed.returncode, 0)
        self.assertTrue(failure_marker.exists())
        self.assertEqual(entry.read_bytes(), previous_entry)
        self.assertEqual(os.readlink(updater_link), previous_target)
        self.assertEqual(updater_link.read_bytes(), previous_updater)
        self.assertEqual(self.run_installer().returncode, 0)
        self.assertNotEqual(os.readlink(updater_link), previous_target)
        self.assert_installed()
        self.assert_private_preserved()

    def test_independent_tool_lock_and_modified_immutable_version_refuse(self):
        import fcntl
        with Path(f"{self.state}.updater-tools.lock").open("w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            blocked = self.run_installer()
            self.assertNotEqual(blocked.returncode, 0)
            self.assertIn("Another updater installation", blocked.stderr)
        # Holding the coordinator lock must not deadlock tool-only installation.
        with Path(f"{self.state}.operation.lock").open("w") as operation:
            fcntl.flock(operation, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.run_installer()
            self.assertEqual(result.returncode, 0, result.stderr)
        entry = self.assert_installed()
        original_entry = entry.read_bytes()
        installed = self.state / "tools/update-online.sh"
        installed.write_text("tampered updater fixture\n")
        refused = self.run_installer()
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("Existing updater version checksum mismatch", refused.stderr)
        self.assertEqual(entry.read_bytes(), original_entry)
        self.assert_private_preserved()


if __name__ == "__main__":
    unittest.main()
