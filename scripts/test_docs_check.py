"""Exercise source-preservation failures against real documents in memory."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("docs_checker", ROOT / "scripts/check_docs.py")
docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs)
SNAPSHOT = ROOT / ".local/docs-028/source"


class SourcePreservationTests(unittest.TestCase):
    def inspect_mutation(self, path, mutate):
        en, zh = Path(path), docs.chinese_path(Path(path))
        original_read = docs.read
        source_file = SNAPSHOT / en
        source_content = original_read(ROOT / en)
        original_is_dir, original_is_file = Path.is_dir, Path.is_file
        values = {ROOT / en: mutate(original_read(ROOT / en), False), ROOT / zh: mutate(original_read(ROOT / zh), True)}
        def read_fixture(filename):
            if filename == source_file:
                return source_content
            return values.get(filename, original_read(filename))
        with patch.object(docs, "inventory", return_value=[en, zh]), patch.object(Path, "is_dir", lambda path: True if path == SNAPSHOT else original_is_dir(path)), patch.object(Path, "is_file", lambda path: True if path == source_file else original_is_file(path)), patch.object(docs, "read", side_effect=read_fixture):
            return docs.check(ROOT, SNAPSHOT)[0]

    def test_both_languages_cannot_change_the_recorded_test_total(self):
        def mutate(content, chinese):
            old, new = ("总计 31 项", "总计 310 项") if chinese else ("31 total", "310 total")
            self.assertIn(old, content)
            return content.replace(old, new)
        errors = self.inspect_mutation("docs/verification/WI-MOUSE-022.md", mutate)
        self.assertTrue(any("original numeric evidence" in error for error in errors), errors)

    def test_both_languages_cannot_drop_a_verification_boundary_section(self):
        def mutate(content, chinese):
            headings = docs.headings(content)
            index = next(i for i, row in enumerate(docs.headings(docs.read(ROOT / "docs/verification/WI-REMOTE-012.md"))) if row[2] == "Limits and compatibility")
            lines = content.splitlines()
            del lines[headings[index][0]]
            return "\n".join(lines)
        errors = self.inspect_mutation("docs/verification/WI-REMOTE-012.md", mutate)
        self.assertTrue(any("original section hierarchy" in error for error in errors), errors)

    def test_missing_snapshot_is_an_error(self):
        errors, count = docs.check(ROOT, SNAPSHOT / "nonexistent-snapshot")
        self.assertEqual(count, 0)
        self.assertTrue(any("Snapshot directory does not exist" in error for error in errors))

    def test_full_real_corpus_is_complete(self):
        errors, count = docs.check(ROOT)
        self.assertEqual(errors, [])
        self.assertGreaterEqual(count, 43)


if __name__ == "__main__":
    unittest.main()
