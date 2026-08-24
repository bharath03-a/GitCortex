"""Tests for the agy permissions allow-list swap (replaces --dangerously-skip-permissions)."""

from __future__ import annotations

import json
import unittest

import agent_run


class AgyPermissionsConfigTests(unittest.TestCase):
    def setUp(self) -> None:
        self._original_settings_path = agent_run.AGY_SETTINGS
        self.addCleanup(setattr, agent_run, "AGY_SETTINGS", self._original_settings_path)

    def _use_real_tmp_path(self, tmp_path):
        agent_run.AGY_SETTINGS = tmp_path

    def test_grants_mcp_and_pwd_when_enabled(self) -> None:
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "settings.json"
            path.write_text(json.dumps({"model": "test-model"}), encoding="utf-8")
            self._use_real_tmp_path(path)

            with agent_run.agy_permissions_config(enabled=True):
                written = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(written["model"], "test-model")
                self.assertIn("mcp(gitcortex/gcx)", written["permissions"]["allow"])
                self.assertIn("command(pwd)", written["permissions"]["allow"])

            restored = json.loads(path.read_text(encoding="utf-8"))
            self.assertNotIn("permissions", restored)
            self.assertEqual(restored["model"], "test-model")

    def test_grants_only_pwd_when_disabled(self) -> None:
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "settings.json"
            path.write_text(json.dumps({}), encoding="utf-8")
            self._use_real_tmp_path(path)

            with agent_run.agy_permissions_config(enabled=False):
                written = json.loads(path.read_text(encoding="utf-8"))
                self.assertNotIn("mcp(gitcortex/gcx)", written["permissions"]["allow"])
                self.assertIn("command(pwd)", written["permissions"]["allow"])

    def test_creates_and_removes_file_when_none_existed(self) -> None:
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "nested" / "settings.json"
            self._use_real_tmp_path(path)

            with agent_run.agy_permissions_config(enabled=True):
                self.assertTrue(path.exists())

            self.assertFalse(path.exists())


if __name__ == "__main__":
    unittest.main()
