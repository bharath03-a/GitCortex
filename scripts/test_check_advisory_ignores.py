"""Tests for advisory ignore parity and documentation."""
import contextlib
import io
from pathlib import Path
import tempfile
import unittest

try:
    from scripts.check_advisory_ignores import main
except ModuleNotFoundError:
    from check_advisory_ignores import main


class AdvisoryIgnoreTests(unittest.TestCase):
    def check(self, deny, audit):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ("deny.toml", "audit.toml")]
            for path, entries in zip(paths, (deny, audit)):
                path.write_text('[advisories]\nignore = [\n' + entries + '\n]\n')
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                result = main(*paths)
            return result, output.getvalue()

    def test_identical_lists_pass(self):
        entry = '"RUSTSEC-2024-0001", # documented reason'
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_id_only_in_audit_fails(self):
        entry = '"RUSTSEC-2024-0001", # reason'
        result, output = self.check(entry, entry + '\n"RUSTSEC-2024-0002", # reason')
        self.assertEqual(result, 1)
        self.assertIn("RUSTSEC-2024-0002", output)

    def test_missing_reason_fails(self):
        result, output = self.check('"RUSTSEC-2024-0001",', '"RUSTSEC-2024-0001", # reason')
        self.assertEqual(result, 1)
        self.assertIn("missing reason", output)

    def test_malformed_id_fails(self):
        result, output = self.check('"RUSTSEC-1", # reason', '"RUSTSEC-1", # reason')
        self.assertEqual(result, 1)
        self.assertIn("malformed", output)

    def test_missing_reason_on_audit(self):
        self.assertEqual(self.check('"RUSTSEC-2024-0001", # reason',
                                    '"RUSTSEC-2024-0001",')[0], 1)

    def test_deny_only(self):
        self.assertEqual(self.check('"RUSTSEC-2024-0001", # reason', '')[0], 1)

    def test_invalid_toml(self):
        self.assertEqual(self.check('not valid toml', '')[0], 1)

    def test_missing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.toml"
            self.assertEqual(main(missing, missing), 1)

    def test_duplicate(self):
        entry = '"RUSTSEC-2024-0001", # reason'
        result, output = self.check(entry + '\n' + entry, entry)
        self.assertEqual(result, 1)
        self.assertIn("duplicate", output)

    def test_no_trailing_comma(self):
        entry = '"RUSTSEC-2024-0001" # reason'
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_single_quotes(self):
        entry = "'RUSTSEC-2024-0001', # reason"
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_commented_id(self):
        entry = '# "RUSTSEC-2024-0001",'
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_reason_punctuation(self):
        entry = '"RUSTSEC-2024-0001", # reason contains # and ]'
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_multiple_ids_need_separate_reasons(self):
        entry = '"RUSTSEC-2024-0001", "RUSTSEC-2024-0002", # reason'
        result, output = self.check(entry, entry)
        self.assertEqual(result, 1)
        self.assertIn("multiple IDs", output)

    def test_object_reason(self):
        entry = '{ id = "RUSTSEC-2024-0001", reason = "because # ]" }'
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_multiple_objects_have_individual_reasons(self):
        entry = ('{ id = "RUSTSEC-2024-0001", reason = "first" }, '
                 '{ id = "RUSTSEC-2024-0002", reason = "second" }')
        self.assertEqual(self.check(entry, entry)[0], 0)

    def test_object_missing_reason(self):
        entry = '{ id = "RUSTSEC-2024-0001" }'
        self.assertEqual(self.check(entry, entry)[0], 1)

    def test_one_line_array(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "deny.toml"
            path.write_text('[advisories]\nignore = ["RUSTSEC-2024-0001"] # reason\n')
            self.assertEqual(main(path, path), 0)

    def test_real_repo_files(self):
        root = Path(__file__).resolve().parent.parent
        self.assertEqual(main(root / "deny.toml", root / ".cargo/audit.toml"), 0)
