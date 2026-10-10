"""Synthetic deployment files and injected Compose outcomes; no real email or production writes."""

import fcntl
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('smtp_config', Path(__file__).with_name('configure-smtp.py'))
smtp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smtp)

MODEL = {'services': {
    'db': {'environment': {'POSTGRES_USER': 'farsail', 'POSTGRES_PASSWORD': 'synthetic$$database', 'POSTGRES_DB': 'farsail'}},
    'relay': {'environment': {'IROH_RELAY_HTTP_BEARER_TOKEN': 'synthetic-relay-token'}},
    'gateway': {'ports': [{'target': 8444, 'published': '18443'}]},
}}
BASE = 'FARSAIL_DATABASE_URL=postgres://original\nFARSAIL_BIND=127.0.0.1:8787\nFARSAIL_RELAY_ACCESS_TOKEN=original-relay\nFARSAIL_RELAY_URLS=https://original.invalid:8443/\n'
MAIL = {'FARSAIL_MAIL_MODE': 'smtp-tls', 'FARSAIL_SMTP_PASSWORD': "synthetic$'\\\"authorization"}


class ConfigTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='farsail-smtp-config-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.state = self.root / '.local/production'
        self.state.mkdir(parents=True)
        (self.root / 'deploy/production').mkdir(parents=True)
        (self.root / 'deploy/production/compose.yaml').write_text('fixture only\n')
        (self.state / 'compose.env').write_text('fixture only\n')
        (self.state / 'settings.json').write_text('{"ip":"127.0.0.1","mode":"production"}')
        self.path = self.state / 'coordinator.env'

    def test_repair_missing_base_preserves_mail_and_other_lines(self):
        original = 'FARSAIL_SMTP_PASSWORD=original-synthetic\n# keep this\nUNRELATED=keep\n'
        self.path.write_text(original)
        with patch.object(smtp, 'run', side_effect=[json.dumps(MODEL), '']):
            backup = smtp.configure(self.root, self.state)
        current = self.path.read_text()
        self.assertTrue(current.startswith(original))
        self.assertIn('postgres://farsail:synthetic%24database@db:5432/farsail', current)
        self.assertIn('https://127.0.0.1:18443/', current)
        self.assertEqual(backup.read_text(), original)
        self.assertEqual(backup.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(backup.parent.stat().st_mode & 0o777, 0o700)

    def test_update_preserves_existing_base_and_deduplicates_mail(self):
        self.path.write_text(BASE + 'FARSAIL_MAIL_MODE=smtp-local\nexport FARSAIL_MAIL_MODE=smtp-local\n')
        with patch.object(smtp, 'run', side_effect=[json.dumps(MODEL), '']):
            smtp.configure(self.root, self.state, MAIL)
        current = self.path.read_text()
        self.assertTrue(current.startswith(BASE))
        self.assertEqual(current.count('FARSAIL_MAIL_MODE='), 1)
        self.assertFalse(list(self.state.glob('.smtp-config-*')))

    def test_failed_validation_restores_original_without_recreation(self):
        self.path.write_text(BASE)
        with patch.object(smtp, 'run', side_effect=[json.dumps(MODEL), smtp.ConfigError('synthetic failure')]) as run:
            with self.assertRaisesRegex(smtp.ConfigError, 'original configuration restored'):
                smtp.configure(self.root, self.state, MAIL, apply=True)
        self.assertEqual(self.path.read_text(), BASE)
        self.assertEqual(run.call_count, 2)

    def test_failed_start_restores_original_then_recreates_only_coordinator(self):
        self.path.write_text(BASE)
        with patch.object(smtp, 'run', side_effect=[json.dumps(MODEL), '', smtp.ConfigError('synthetic failure'), '']) as run:
            with self.assertRaisesRegex(smtp.ConfigError, 'original configuration restored'):
                smtp.configure(self.root, self.state, MAIL, apply=True)
        self.assertEqual(self.path.read_text(), BASE)
        self.assertEqual(run.call_count, 4)
        for call in run.call_args_list[2:]:
            self.assertEqual(call.args[0][-1], 'coordinator')
            self.assertIn('--no-deps', call.args[0])
            self.assertIn('--pull', call.args[0])
        self.assertNotIn(MAIL['FARSAIL_SMTP_PASSWORD'], str(run.call_args_list))

    def test_already_complete_config_can_recreate_stale_container(self):
        self.path.write_text(BASE)
        with patch.object(smtp, 'run', side_effect=[json.dumps(MODEL), '', '']) as run:
            backup = smtp.configure(self.root, self.state, apply=True)
        self.assertEqual(self.path.read_text(), BASE)
        self.assertIsNotNone(backup)
        self.assertEqual(run.call_count, 3)

    def test_missing_backup_source_leaves_original_unchanged(self):
        self.path.write_text('FARSAIL_MAIL_MODE=smtp-tls\n')
        with patch.object(smtp, 'run', return_value='{"services":{}}'):
            with self.assertRaisesRegex(smtp.ConfigError, 'Cannot restore defaults'):
                smtp.configure(self.root, self.state, MAIL)
        self.assertEqual(self.path.read_text(), 'FARSAIL_MAIL_MODE=smtp-tls\n')
        self.assertFalse((self.state / 'backups').exists())

    def test_shared_lock_refuses_concurrent_operation(self):
        self.path.write_text(BASE)
        with open(str(self.state) + '.operation.lock', 'a') as lock, patch.object(smtp, 'run') as run:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(smtp.ConfigError, 'Another deployment operation'):
                smtp.configure(self.root, self.state, MAIL)
            run.assert_not_called()
        self.assertEqual(self.path.read_text(), BASE)

    def test_config_injection_refused_before_write(self):
        self.path.write_text(BASE)
        with patch.object(smtp, 'run', return_value=json.dumps(MODEL)):
            with self.assertRaisesRegex(smtp.ConfigError, 'control characters'):
                smtp.configure(self.root, self.state, {'FARSAIL_SMTP_PASSWORD': 'synthetic\nINJECTED=1'})
        self.assertEqual(self.path.read_text(), BASE)


if __name__ == '__main__':
    unittest.main()
