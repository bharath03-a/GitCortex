"""Tests for graphify_compare.py's Codex & Claude runners, command builder, and freshness gate."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest import mock

import graphify_compare as gc


@dataclass
class FakeTask:
    id: str = "cobra-impact-add-command"
    repo: str = "cobra"
    action: str = "impact"
    query: str = "Command.AddCommand"
    required: tuple[str, ...] = ("command.go", "completions.go")
    forbidden: tuple[str, ...] = ()


def codex_stream(*events: dict) -> str:
    return "\n".join(json.dumps(e) for e in events) + "\n"


class GraphifyCommandTest(unittest.TestCase):
    def test_impact_maps_to_affected(self) -> None:
        self.assertEqual(gc.graphify_command(FakeTask(action="impact", query="Command.AddCommand")), ["graphify", "affected", "AddCommand"])

    def test_architecture_and_refactor_map_to_explain(self) -> None:
        self.assertEqual(gc.graphify_command(FakeTask(action="architecture", query="searcher::Searcher")), ["graphify", "explain", "Searcher"])
        self.assertEqual(gc.graphify_command(FakeTask(action="refactor", query="Session.send")), ["graphify", "explain", "send"])


class BuildGraphifyCodexCommandTest(unittest.TestCase):
    def test_codex_command_enforces_isolation_and_exact_graphify_invocation(self) -> None:
        repo_dir = Path("/tmp/repos/cobra")
        cmd = gc.build_graphify_codex_command(FakeTask(), repo_dir, model="gpt-5.4-mini", reasoning="low")
        self.assertEqual(cmd[:2], ["codex", "exec"])
        for flag in ("--json", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--dangerously-bypass-approvals-and-sandbox"):
            self.assertIn(flag, cmd)
        self.assertEqual(cmd[cmd.index("-m") + 1], "gpt-5.4-mini")
        self.assertEqual(cmd[cmd.index("-c") + 1], 'model_reasoning_effort="low"')
        self.assertEqual(cmd[cmd.index("-C") + 1], str(repo_dir))
        self.assertIn("graphify affected AddCommand", cmd[-1])


class RunGraphifyArmCodexTest(unittest.TestCase):
    def _run_with_stdout(self, stdout: str, returncode: int = 0):
        with tempfile.TemporaryDirectory() as tmp:
            log_path = Path(tmp) / "logs" / "task.jsonl"
            proc = subprocess.CompletedProcess([], returncode, stdout, "")
            with mock.patch.object(gc.subprocess, "run", return_value=proc):
                return gc.run_graphify_arm_codex(FakeTask(), Path(tmp), log_path)

    def test_single_graphify_call_passes(self) -> None:
        stdout = codex_stream(
            {"type": "thread.started", "thread_id": "th-1"},
            {"type": "turn.started"},
            {
                "type": "item.completed",
                "item": {
                    "type": "command_execution",
                    "command": "/bin/zsh -lc 'graphify affected AddCommand'",
                    "exit_code": 0,
                    "status": "completed",
                },
            },
            {"type": "item.completed", "item": {"type": "agent_message", "text": "Touches command.go and completions.go."}},
            {"type": "turn.completed", "usage": {"input_tokens": 1000, "cached_input_tokens": 200, "output_tokens": 50}},
        )
        res = self._run_with_stdout(stdout)
        self.assertFalse(res.error)
        self.assertEqual(res.gcx_calls, 1)
        self.assertEqual(res.total_tokens, 1050)
        self.assertEqual(res.quality_score, 1.0)

    def test_zero_calls_is_flagged_as_error(self) -> None:
        stdout = codex_stream(
            {"type": "item.completed", "item": {"type": "agent_message", "text": "command.go and completions.go"}},
            {"type": "turn.completed", "usage": {"input_tokens": 500, "cached_input_tokens": 0, "output_tokens": 20}},
        )
        res = self._run_with_stdout(stdout)
        self.assertTrue(res.error)
        self.assertTrue(any("never invoked" in m for m in res.error_messages))

    def test_two_calls_is_flagged_as_error(self) -> None:
        stdout = codex_stream(
            {"type": "item.completed", "item": {"type": "command_execution", "command": "graphify affected AddCommand", "exit_code": 0}},
            {"type": "item.completed", "item": {"type": "command_execution", "command": "graphify explain AddCommand", "exit_code": 0}},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "command.go and completions.go"}},
            {"type": "turn.completed", "usage": {"input_tokens": 500, "cached_input_tokens": 0, "output_tokens": 20}},
        )
        res = self._run_with_stdout(stdout)
        self.assertTrue(res.error)
        self.assertTrue(any("expected 1" in m for m in res.error_messages))

    def test_nonzero_exit_code_is_flagged_as_error(self) -> None:
        stdout = codex_stream(
            {"type": "item.completed", "item": {"type": "command_execution", "command": "graphify affected AddCommand", "exit_code": 0}},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "command.go and completions.go"}},
            {"type": "turn.completed", "usage": {"input_tokens": 500, "cached_input_tokens": 0, "output_tokens": 20}},
        )
        res = self._run_with_stdout(stdout, returncode=1)
        self.assertTrue(res.error)
        self.assertTrue(any("codex exited 1" in m for m in res.error_messages))


class BaselineDiscoveryAndFreshnessTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmpdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tmpdir.name)
        self.gcx_path = self.root / "gcx"
        self.gcx_path.write_bytes(b"fake gcx binary")

    def tearDown(self) -> None:
        self.tmpdir.cleanup()

    def test_client_labels_and_find_baseline_source(self) -> None:
        self.assertEqual(gc.BASELINE_LABEL_BY_CLIENT["claude"], "claude-v2-full")
        self.assertEqual(gc.BASELINE_LABEL_BY_CLIENT["codex"], "codex-v2-full")

        incomplete = self.root / "codex-v2-full-20260101T000000Z.agent.jsonl"
        incomplete.write_text(json.dumps({"type": "meta"}) + "\n", encoding="utf-8")
        complete = self.root / "codex-v2-full-20260102T000000Z.agent.jsonl"
        complete.write_text(
            json.dumps({"type": "meta"}) + "\n" + json.dumps({"type": "summary"}) + "\n",
            encoding="utf-8",
        )
        self.assertEqual(gc.find_baseline_source("codex", results_dir=self.root), complete)

    def test_check_baseline_freshness_matches_and_rejects_stale(self) -> None:
        gcx_sha = hashlib.sha256(self.gcx_path.read_bytes()).hexdigest()
        suite_sha = hashlib.sha256(gc.SUITE_PATH.read_bytes()).hexdigest()

        good = self.root / "good.jsonl"
        good.write_text(json.dumps({"type": "meta", "gcx_sha256": gcx_sha, "suite_sha256": suite_sha}) + "\n")
        gc.check_baseline_freshness(good, self.gcx_path)

        stale_gcx = self.root / "stale_gcx.jsonl"
        stale_gcx.write_text(json.dumps({"type": "meta", "gcx_sha256": "stale", "suite_sha256": suite_sha}) + "\n")
        with self.assertRaises(SystemExit):
            gc.check_baseline_freshness(stale_gcx, self.gcx_path)

        stale_suite = self.root / "stale_suite.jsonl"
        stale_suite.write_text(json.dumps({"type": "meta", "gcx_sha256": gcx_sha, "suite_sha256": "stale"}) + "\n")
        with self.assertRaises(SystemExit):
            gc.check_baseline_freshness(stale_suite, self.gcx_path)


class ParseArgsTest(unittest.TestCase):
    def test_parse_args_defaults_and_codex_flags(self) -> None:
        default_args = gc.parse_args([])
        self.assertEqual(default_args.client, "claude")
        self.assertIsNone(default_args.baseline_source)

        codex_args = gc.parse_args([
            "--client", "codex",
            "--baseline-source", "/tmp/base.jsonl",
            "--gcx", "/opt/gcx",
            "--out", "/tmp/out.jsonl",
            "--model", "gpt-5.4-mini",
            "--reasoning", "medium",
            "--repos-dir", "/tmp/repos",
        ])
        self.assertEqual(codex_args.client, "codex")
        self.assertEqual(codex_args.baseline_source, Path("/tmp/base.jsonl"))
        self.assertEqual(codex_args.gcx, Path("/opt/gcx"))
        self.assertEqual(codex_args.out, Path("/tmp/out.jsonl"))
        self.assertEqual(codex_args.model, "gpt-5.4-mini")
        self.assertEqual(codex_args.reasoning, "medium")
        self.assertEqual(codex_args.repos_dir, Path("/tmp/repos"))


if __name__ == "__main__":
    unittest.main()
