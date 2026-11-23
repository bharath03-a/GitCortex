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


class CodexArmIsolationTests(unittest.TestCase):
    def test_run_codex_arm_passes_ignore_user_config(self) -> None:
        import subprocess
        import tempfile
        from dataclasses import dataclass
        from pathlib import Path
        from unittest import mock

        @dataclass
        class StubTask:
            id: str = "cobra-impact-add-command"
            repo: str = "cobra"
            action: str = "impact"
            query: str = "AddCommand"
            required: tuple[str, ...] = ("command.go",)
            forbidden: tuple[str, ...] = ()

        seen: list[list[str]] = []

        def fake_run(cmd, **kwargs):
            seen.append(cmd)
            stdout = "\n".join([
                json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": "command.go"}}),
                json.dumps({"type": "turn.completed", "usage": {"input_tokens": 100, "cached_input_tokens": 0, "output_tokens": 10}}),
            ])
            return subprocess.CompletedProcess(cmd, 0, stdout, "")

        with tempfile.TemporaryDirectory() as tmp:
            with mock.patch.object(agent_run.subprocess, "run", side_effect=fake_run):
                res = agent_run.run_codex_arm(
                    StubTask(), Path(tmp), "gpt-5.4-mini", "low", ["/opt/gcx", "query", "impact", "AddCommand"],
                    "/opt/gcx", "baseline", Path(tmp) / "log.jsonl",
                )
        self.assertFalse(res.error)
        self.assertEqual(len(seen), 1)
        for flag in ("--json", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--dangerously-bypass-approvals-and-sandbox"):
            self.assertIn(flag, seen[0])


if __name__ == "__main__":
    unittest.main()
