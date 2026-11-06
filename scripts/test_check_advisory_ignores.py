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

    def test_real_repo_files(self):
        root = Path(__file__).resolve().parent.parent
        self.assertEqual(main(root / "deny.toml", root / ".cargo/audit.toml"), 0)
