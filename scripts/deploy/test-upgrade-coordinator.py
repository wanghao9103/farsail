#!/usr/bin/env python3
"""Upgrade contracts in private temporary fixtures; never accesses Docker/PG."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("upgrade-coordinator.sh").resolve()
OLD_REV, NEW_REV = "1" * 40, "2" * 40
OLD_DIGEST, NEW_DIGEST = "sha256:" + "3" * 64, "sha256:" + "4" * 64
OLD_TAG, NEW_TAG = "farsail/coordinator:old", "farsail/coordinator:new"

FAKE_DOCKER = r'''#!/usr/bin/env python3
import io,json,os,sys,tarfile
from pathlib import Path
p=Path(os.environ['FAKE_DOCKER_STATE']); s=json.loads(p.read_text()); a=sys.argv[1:]
s['calls'].append(a)
def done(code=0,text=''):
    p.write_text(json.dumps(s)); sys.stdout.write(text); sys.exit(code)
def selected(env):
    return next(x.split('=',1)[1] for x in Path(env).read_text().splitlines() if x.startswith('COORDINATOR_IMAGE='))
if a[0]=='compose':
    env=a[a.index('--env-file')+1]; cmd=a[a.index('-f')+2:]
    if cmd[0]=='config':
        done(text=json.dumps({'services':{'coordinator':{'image':selected(env)}}}) if '--format' in cmd else '')
    if cmd[0]=='ps': done(text='fixture-coordinator\n')
    if cmd[0]=='exec':
        if 'pg_dump' in cmd: done(text='fixture-private-dump\n')
        if 'pg_restore' in cmd:
            done(0 if sys.stdin.read()=='fixture-private-dump\n' else 1)
        done(1)
    if cmd[0]=='up':
        tag=selected(env)
        if tag==s['load_tag'] and s.get('fail_up_once'):
            s['fail_up_once']=False; done(1)
        s['container_image']=s['images'][tag]
        s['health']='unhealthy' if tag==s['load_tag'] and s.get('unhealthy_new') else 'healthy'
        done()
    done(1)
if a[:2]==['image','inspect']:
    done(0 if a[2] in s['images'] else 1,text='linux/amd64\n' if '--format' in a else '')
if a[0]=='load':
    s['images'][s['load_tag']]=s['load_digest']; done()
if a[0]=='inspect':
    done(text=(s['health'] if 'Health' in a[-1] else s['container_image'])+'\n')
if a[0]=='save':
    ref=a[1]; digest=s['images'].get(ref,ref if ref in s['images'].values() else '')
    if not digest: done(1)
    payload=json.dumps([{'Config':digest.removeprefix('sha256:')+'.json','RepoTags':[ref]}]).encode()
    archive=io.BytesIO()
    with tarfile.open(fileobj=archive,mode='w') as t:
        member=tarfile.TarInfo('manifest.json');member.size=len(payload);t.addfile(member,io.BytesIO(payload))
    p.write_text(json.dumps(s));sys.stdout.buffer.write(archive.getvalue());sys.exit(0)
done(1)
'''

FAKE_MV = r'''#!/usr/bin/env python3
import os,subprocess,sys
from pathlib import Path
suffix=os.environ.get('FAKE_FAIL_MV_SUFFIX',''); marker=Path(os.environ['FAKE_MV_MARKER'])
if suffix and sys.argv[-1].endswith(suffix) and not marker.exists():
    marker.write_text('injected once');sys.exit(1)
restore_suffix=os.environ.get('FAKE_FAIL_MV_RESTORE_SUFFIX','')
if restore_suffix and sys.argv[-1].endswith(restore_suffix) and marker.exists() and marker.read_text()=='injected once':
    marker.write_text('recovery also failed');sys.exit(1)
sys.exit(subprocess.run(['/usr/bin/mv',*sys.argv[1:]]).returncode)
'''


class UpgradeContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="farsail-upgrade-contract-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "deployment"
        self.state = self.root / "custom-private-state"
        self.release = self.base / "release"
        self.bin = self.base / "bin"
        for folder in (self.state, self.release, self.bin, self.root / "deploy/production"):
            folder.mkdir(parents=True, mode=0o700)
        (self.root / "deploy/production/compose.yaml").write_text("fixture only\n")
        self.compose = self.state / "compose.env"
        self.meta = self.state / "coordinator-release.json"
        self.receipt = self.state / "coordinator-online-receipt.json"
        self.selection = self.base / "selection.json"
        self.docker_state = self.base / "docker-state.json"
        self.compose.write_text(f"STATE_DIR={self.state}\nCOMPOSE_PROJECT_NAME=fixture-only\nCOORDINATOR_IMAGE={OLD_TAG}\n")
        self.old_meta = {"image": OLD_TAG, "config_digest": OLD_DIGEST, "source_revision": OLD_REV, "platform": "linux/amd64", "retained": "old bytes"}
        self.meta.write_text(json.dumps(self.old_meta, indent=2) + "\n")
        self.new_meta = {"online_update_protocol": 1, "image": NEW_TAG, "config_digest": NEW_DIGEST, "source_revision": NEW_REV, "package_source_revision": "9" * 40, "platform": "linux/amd64"}
        self.selected = {"repository": "wanghao9103/farsail", "asset_id": 200, "created_at": "2026-10-10T06:00:00Z", "sha256": "a" * 64, "tag": "coordinator-preview", "name": "FarSail_coordinator_tools_refresh_linux_amd64.tar.gz", "url": "https://github.com/wanghao9103/farsail/releases/download/coordinator-preview/FarSail_coordinator_tools_refresh_linux_amd64.tar.gz", "base_source_revision": OLD_REV, "source_comparison": "ahead"}
        self.fake = {"calls": [], "images": {OLD_TAG: OLD_DIGEST}, "load_tag": NEW_TAG, "load_digest": NEW_DIGEST, "health": "healthy", "container_image": OLD_DIGEST}
        (self.bin / "docker").write_text(FAKE_DOCKER)
        (self.bin / "mv").write_text(FAKE_MV)
        for p in self.bin.iterdir():
            p.chmod(0o700)
        self.env = {**os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"], "FARSAIL_STATE_DIR": str(self.state), "FAKE_DOCKER_STATE": str(self.docker_state), "FAKE_MV_MARKER": str(self.base / "mv-failed")}
        for key in ("COMPOSE_FILE", "COMPOSE_PROJECT_NAME", "COMPOSE_PROFILES", "COMPOSE_ENV_FILES", "COMPOSE_DISABLE_ENV_FILE", "STATE_DIR", "COORDINATOR_IMAGE"):
            self.env.pop(key, None)
        shutil.copyfile(SCRIPT, self.release / "upgrade-coordinator.sh")
        (self.release / "update-online.sh").write_text("# fixture updater\n")
        (self.release / "install-online-updater.sh").write_text('set -eu\nprintf installed > "$2/updater-tools-installed"\n')
        (self.release / "coordinator-images.tar.gz").write_bytes(b"fixture Docker image archive")
        for p in self.state.iterdir():
            p.chmod(0o600)

    def write_inputs(self):
        (self.release / "coordinator-release.json").write_text(json.dumps(self.new_meta) + "\n")
        self.selection.write_text(json.dumps(self.selected) + "\n")
        self.docker_state.write_text(json.dumps(self.fake))
        checks = []
        for p in self.release.iterdir():
            if p.name != "SHA256SUMS":
                checks.append(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n")
        (self.release / "SHA256SUMS").write_text("".join(checks))

    def run_upgrade(self, *, online=True, env=None, expected=0, retained_snapshot=False):
        self.write_inputs()
        args = ["bash", str(SCRIPT), str(self.release), str(self.root)]
        if online:
            args += ["--online-selection", str(self.selection)]
        result = subprocess.run(args, env=env or self.env, text=True, capture_output=True, timeout=20)
        self.fake = json.loads(self.docker_state.read_text())
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        self.assertFalse(list(self.state.glob("*.tmp.*")))
        if not retained_snapshot:
            self.assertFalse(list(self.state.glob(".coordinator-noop.*")))
        self.assertFalse(list(self.state.glob(".coordinator-online-receipt.*")))
        return result

    def rejected_without_mutation(self):
        original = self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes() if self.receipt.exists() else None
        result = self.run_upgrade(expected=1)
        self.assertEqual((self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes() if self.receipt.exists() else None), original)
        self.assertFalse(any(a[0] == "load" or "up" in a or "pg_dump" in a for a in self.fake["calls"]))
        return result

    def current_receipt(self, **changes):
        value = {k: self.selected[k] for k in ("repository", "asset_id", "created_at", "sha256", "tag", "name", "url")}
        value.update(online_update_protocol=1, source_revision=OLD_REV, image=OLD_TAG, config_digest=OLD_DIGEST)
        value.update(changes)
        self.receipt.write_text(json.dumps(value, indent=2) + "\n")
        return value

    def same_image(self):
        self.compose.write_text(self.compose.read_text().replace(OLD_TAG, NEW_TAG))
        value = {**self.new_meta, "retained": "legacy full identity without updater protocol"}
        value.pop("online_update_protocol")
        self.meta.write_text(json.dumps(value, indent=2) + "\n")
        self.selected.update(base_source_revision=NEW_REV, source_comparison="identical")
        self.fake["images"][NEW_TAG] = NEW_DIGEST
        self.fake["container_image"] = NEW_DIGEST

    def test_online_upgrade_private_backup_identity_and_scoped_recreate(self):
        self.current_receipt(asset_id=100, created_at="2026-10-09T06:00:00Z")
        result = self.run_upgrade()
        self.assertIn("upgraded and healthy", result.stdout)
        self.assertEqual(json.loads(self.meta.read_text()), self.new_meta)
        receipt = json.loads(self.receipt.read_text())
        self.assertEqual(receipt["asset_id"], 200)
        self.assertEqual(receipt["source_revision"], NEW_REV)
        backups = list((self.state / "backups").iterdir())
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / "database.dump").read_bytes(), b"fixture-private-dump\n")
        for p in backups[0].iterdir():
            self.assertEqual(p.stat().st_mode & 0o077, 0)
        ups = [a for a in self.fake["calls"] if "up" in a]
        self.assertEqual(len(ups), 1)
        self.assertEqual(ups[0][-1], "coordinator")
        self.assertIn("--no-deps", ups[0])
        self.assertIn("--pull", ups[0])
        self.assertEqual((self.root / "updater-tools-installed").read_text(), "installed")

    def test_manual_verified_package_remains_compatible(self):
        self.new_meta.pop("online_update_protocol")
        self.new_meta.pop("source_revision")
        self.run_upgrade(online=False)
        self.assertFalse(self.receipt.exists())
        self.assertEqual(json.loads(self.meta.read_text()), self.new_meta)

    def test_release_path_with_spaces_and_backslash_keeps_checksum_identity(self):
        renamed = self.base / "release with \\ literal backslash"
        self.release.rename(renamed)
        self.release = renamed
        self.run_upgrade()
        self.assertEqual(json.loads(self.meta.read_text())["source_revision"], NEW_REV)

    def test_exported_compose_controls_are_rejected_without_values(self):
        for key in ("STATE_DIR", "COORDINATOR_IMAGE", "COMPOSE_PROJECT_NAME"):
            env = {**self.env, key: "sensitive-override-value"}
            result = self.run_upgrade(env=env, expected=1)
            self.assertIn(key, result.stderr)
            self.assertNotIn("sensitive-override-value", result.stdout + result.stderr)
            self.assertFalse(any(a[0] == "load" for a in self.fake["calls"]))

    def test_same_shared_lock_blocks_upgrade_before_docker(self):
        with open(str(self.state) + ".operation.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.run_upgrade(expected=1)
        self.assertIn("operation is active", result.stderr)
        self.assertEqual(self.fake["calls"], [])

    def test_same_healthy_image_adopts_protocol_without_database_or_restart(self):
        self.same_image()
        result = self.run_upgrade()
        self.assertIn("already latest and healthy", result.stdout)
        self.assertEqual(json.loads(self.meta.read_text()), self.new_meta)
        self.assertEqual(json.loads(self.receipt.read_text())["source_revision"], NEW_REV)
        self.assertFalse((self.state / "backups").exists())
        self.assertFalse(any(a[0] == "load" or "up" in a or "pg_dump" in a for a in self.fake["calls"]))

    def test_same_tag_unhealthy_running_identity_or_mutated_tag_refused(self):
        self.same_image()
        self.fake["health"] = "unhealthy"
        self.rejected_without_mutation()
        self.fake["health"] = "healthy"
        self.fake["container_image"] = OLD_DIGEST
        self.rejected_without_mutation()
        self.fake["container_image"] = NEW_DIGEST
        self.fake["images"][NEW_TAG] = OLD_DIGEST
        self.rejected_without_mutation()

    def test_stored_metadata_and_locked_source_snapshot_must_match(self):
        value = {**self.old_meta, "image": "farsail/coordinator:another"}
        self.meta.write_text(json.dumps(value))
        self.rejected_without_mutation()
        self.meta.write_text(json.dumps({**self.old_meta, "source_revision": "5" * 40}))
        self.assertIn("changed after", self.rejected_without_mutation().stderr)

    def test_receipt_refuses_downgrade_tie_id_mutation_and_cross_repository(self):
        cases = [
            {"created_at": "2026-10-11T06:00:00Z", "asset_id": 100},
            {"asset_id": 201},
            {"sha256": "b" * 64},
            {"created_at": "2026-10-09T06:00:00Z"},
            {"repository": "elsewhere/farsail", "asset_id": 100},
        ]
        for changes in cases:
            self.current_receipt(**changes)
            with self.subTest(changes=changes):
                self.assertIn("receipt refuses", self.rejected_without_mutation().stderr)

    def test_same_release_newer_asset_can_refresh_verified_same_image(self):
        self.same_image()
        self.current_receipt(asset_id=100, source_revision=NEW_REV, image=NEW_TAG, config_digest=NEW_DIGEST)
        self.run_upgrade()
        self.assertEqual(json.loads(self.receipt.read_text())["asset_id"], 200)

    def test_legacy_bootstrap_is_limited_to_the_two_published_images(self):
        self.old_meta.pop("source_revision")
        self.meta.write_text(json.dumps(self.old_meta))
        self.selected.update(base_source_revision=None, source_comparison="legacy")
        self.assertIn("Unknown legacy", self.rejected_without_mutation().stderr)
        legacy = "farsail/coordinator:verification-code-20261009"
        self.compose.write_text(self.compose.read_text().replace(OLD_TAG, legacy))
        self.meta.write_text(json.dumps({**self.old_meta, "image": legacy}))
        self.fake["images"][legacy] = OLD_DIGEST
        self.assertIn("cannot be proven", self.run_upgrade().stdout)

    def test_non_descendant_or_unofficial_or_missing_protocol_refused(self):
        self.selected["source_comparison"] = "identical"
        self.rejected_without_mutation()
        self.selected["source_comparison"] = "ahead"
        self.selected["url"] = self.selected["url"].replace("wanghao9103", "other")
        self.rejected_without_mutation()
        self.selected["url"] = self.selected["url"].replace("other", "wanghao9103")
        self.new_meta.pop("online_update_protocol")
        self.rejected_without_mutation()

    def test_failed_start_restores_image_metadata_receipt_and_keeps_database_dump(self):
        self.current_receipt(asset_id=100, created_at="2026-10-09T06:00:00Z")
        original = self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes()
        self.fake["fail_up_once"] = True
        result = self.run_upgrade(expected=1)
        self.assertIn("migration is retained", result.stderr)
        self.assertEqual((self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes()), original)
        self.assertEqual(self.fake["container_image"], OLD_DIGEST)
        self.assertEqual(len([a for a in self.fake["calls"] if "up" in a]), 2)
        self.assertFalse((self.root / "updater-tools-installed").exists())

    def test_atomic_metadata_or_receipt_failure_restores_both_records(self):
        for filename in ("coordinator-release.json", "coordinator-online-receipt.json"):
            self.current_receipt(asset_id=100, created_at="2026-10-09T06:00:00Z")
            original = self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes()
            env = {**self.env, "FAKE_FAIL_MV_SUFFIX": filename}
            (self.base / "mv-failed").unlink(missing_ok=True)
            self.run_upgrade(env=env, expected=1)
            self.assertEqual((self.compose.read_bytes(), self.meta.read_bytes(), self.receipt.read_bytes()), original)

    def test_same_image_record_failure_recovers_without_restart_or_dump(self):
        self.same_image()
        for existing_receipt in (False, True):
            if existing_receipt:
                self.current_receipt(asset_id=100, source_revision=NEW_REV, image=NEW_TAG, config_digest=NEW_DIGEST)
            original = self.meta.read_bytes(), self.receipt.read_bytes() if self.receipt.exists() else None
            (self.base / "mv-failed").unlink(missing_ok=True)
            env = {**self.env, "FAKE_FAIL_MV_SUFFIX": "coordinator-online-receipt.json"}
            self.run_upgrade(env=env, expected=1)
            self.assertEqual((self.meta.read_bytes(), self.receipt.read_bytes() if self.receipt.exists() else None), original)
            self.assertFalse(any("up" in a or "pg_dump" in a for a in self.fake["calls"]))

    def test_persistent_tool_failure_is_separate_from_healthy_coordinator_commit(self):
        (self.release / "install-online-updater.sh").write_text("exit 1\n")
        result = self.run_upgrade(expected=1)
        self.assertIn("Coordinator is healthy", result.stderr)
        self.assertEqual(json.loads(self.meta.read_text()), self.new_meta)
        self.assertEqual(json.loads(self.receipt.read_text())["source_revision"], NEW_REV)
        self.assertEqual(self.fake["container_image"], NEW_DIGEST)
        self.assertEqual(len([a for a in self.fake["calls"] if "up" in a]), 1)

    def test_failed_noop_publication_and_recovery_retains_private_original_snapshots(self):
        self.same_image()
        original = self.meta.read_bytes()
        env = {**self.env, "FAKE_FAIL_MV_SUFFIX": "coordinator-online-receipt.json", "FAKE_FAIL_MV_RESTORE_SUFFIX": "coordinator-release.json"}
        result = self.run_upgrade(env=env, expected=1, retained_snapshot=True)
        snapshots = list(self.state.glob(".coordinator-noop.*"))
        self.assertEqual(len(snapshots), 1)
        self.assertIn(str(snapshots[0]), result.stderr)
        self.assertEqual(snapshots[0].stat().st_mode & 0o777, 0o700)
        original_meta = snapshots[0] / "coordinator-release.json"
        self.assertEqual(original_meta.read_bytes(), original)
        self.assertEqual(original_meta.stat().st_mode & 0o077, 0)
        self.assertFalse(any("up" in a or "pg_dump" in a for a in self.fake["calls"]))

    def test_state_directory_and_lock_must_match_compose_configuration(self):
        other_state = self.base / "another-private-state"
        other_state.mkdir(mode=0o700)
        self.compose.write_text(self.compose.read_text().replace(str(self.state), str(other_state)))
        self.assertIn("STATE_DIR", self.rejected_without_mutation().stderr)

    def test_failed_upgrade_removes_records_that_did_not_preexist(self):
        self.meta.unlink()
        self.fake["fail_up_once"] = True
        self.run_upgrade(online=False, expected=1)
        self.assertFalse(self.meta.exists())
        self.assertFalse(self.receipt.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
