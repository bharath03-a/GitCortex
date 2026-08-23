"""Tests for codegraph_compare.py's exactly-once Bash-call check, the
per-client baseline lookup, and the gcx/suite staleness gate."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from codegraph_compare import (
    count_bash_tool_calls,
    check_baseline_freshness,
    codegraph_command,
    BASELINE_LABEL_BY_CLIENT,
)


def bash_event(command: str) -> str:
    return json.dumps(
        {
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "tool_use", "name": "Bash", "input": {"command": command}}
                ]
            },
        }
    )


class CountBashToolCallsTest(unittest.TestCase):
    def test_single_real_call_counts_once(self) -> None:
        stream = "\n".join([bash_event("codegraph query filter")])
        codegraph_calls, other_bash_calls = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 1)
        self.assertEqual(other_bash_calls, 0)

    def test_two_genuinely_different_calls_still_flagged(self) -> None:
        """A real retry against instructions — a different command — must
        still count as more than one call."""
        stream = "\n".join(
            [
                bash_event("codegraph query filter"),
                bash_event("codegraph callers filter"),
            ]
        )
        codegraph_calls, _ = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 2)

    def test_two_adjacent_identical_calls_dedup_to_one(self) -> None:
        """The 10/10-reproducible harness artifact: a local PreToolUse gate
        hook logs the identical first Bash call twice in a row. That's not
        the model retrying — must not be flagged."""
        stream = "\n".join(
            [
                bash_event("codegraph query filter"),
                bash_event("codegraph query filter"),
            ]
        )
        codegraph_calls, _ = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 1)

    def test_non_adjacent_identical_calls_still_flagged(self) -> None:
        """Same command repeated but not back-to-back is a genuine retry,
        not the adjacent-duplicate artifact — must still be flagged."""
        stream = "\n".join(
            [
                bash_event("codegraph query filter"),
                bash_event("ls"),
                bash_event("codegraph query filter"),
            ]
        )
        codegraph_calls, other_bash_calls = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 2)
        self.assertEqual(other_bash_calls, 1)

    def test_no_codegraph_call_counts_zero(self) -> None:
        stream = "\n".join([bash_event("ls")])
        codegraph_calls, other_bash_calls = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 0)
        self.assertEqual(other_bash_calls, 1)

    def test_adjacent_duplicate_other_bash_calls_also_dedup(self) -> None:
        stream = "\n".join([bash_event("ls"), bash_event("ls")])
        _, other_bash_calls = count_bash_tool_calls(stream)
        self.assertEqual(other_bash_calls, 1)

    def test_ignores_non_json_and_non_assistant_lines(self) -> None:
        stream = "\n".join(
            [
                "not json",
                json.dumps({"type": "result"}),
                bash_event("codegraph query filter"),
            ]
        )
        codegraph_calls, _ = count_bash_tool_calls(stream)
        self.assertEqual(codegraph_calls, 1)


class SimpleTask:
    def __init__(self, action: str, query: str) -> None:
        self.action = action
        self.query = query


class CodegraphCommandTest(unittest.TestCase):
    def test_search_action_maps_to_query(self) -> None:
        self.assertEqual(codegraph_command(SimpleTask("search", "Notify")), ["codegraph", "query", "Notify"])

    def test_callers_action_maps_to_callers(self) -> None:
        self.assertEqual(codegraph_command(SimpleTask("callers", "QuerySet.filter")), ["codegraph", "callers", "filter"])


class BaselineLabelByClientTest(unittest.TestCase):
    def test_claude_and_codex_have_distinct_baseline_labels(self) -> None:
        self.assertEqual(BASELINE_LABEL_BY_CLIENT["claude"], "big-repos-claude-fixed")
        self.assertEqual(BASELINE_LABEL_BY_CLIENT["codex"], "big-repos-codex")
        self.assertNotEqual(BASELINE_LABEL_BY_CLIENT["claude"], BASELINE_LABEL_BY_CLIENT["codex"])


def meta_line(gcx_sha256: str, suite_sha256: str) -> str:
    return json.dumps({"type": "meta", "gcx_sha256": gcx_sha256, "suite_sha256": suite_sha256})


class CheckBaselineFreshnessTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmpdir = tempfile.TemporaryDirectory()
        self.gcx_path = Path(self.tmpdir.name) / "gcx"
        self.gcx_path.write_bytes(b"fake binary contents")

    def tearDown(self) -> None:
        self.tmpdir.cleanup()

    def _real_gcx_sha(self) -> str:
        import hashlib

        return hashlib.sha256(self.gcx_path.read_bytes()).hexdigest()

    def _real_suite_sha(self) -> str:
        import hashlib

        from codegraph_compare import SUITE_PATH

        return hashlib.sha256(SUITE_PATH.read_bytes()).hexdigest()

    def test_matching_hashes_pass_silently(self) -> None:
        baseline = Path(self.tmpdir.name) / "baseline.jsonl"
        baseline.write_text(meta_line(self._real_gcx_sha(), self._real_suite_sha()) + "\n")
        check_baseline_freshness(baseline, self.gcx_path)  # must not raise

    def test_mismatched_gcx_hash_raises(self) -> None:
        baseline = Path(self.tmpdir.name) / "baseline.jsonl"
        baseline.write_text(meta_line("stale-hash", self._real_suite_sha()) + "\n")
        with self.assertRaises(SystemExit):
            check_baseline_freshness(baseline, self.gcx_path)

    def test_mismatched_suite_hash_raises(self) -> None:
        baseline = Path(self.tmpdir.name) / "baseline.jsonl"
        baseline.write_text(meta_line(self._real_gcx_sha(), "stale-suite-hash") + "\n")
        with self.assertRaises(SystemExit):
            check_baseline_freshness(baseline, self.gcx_path)

    def test_missing_meta_line_raises(self) -> None:
        baseline = Path(self.tmpdir.name) / "baseline.jsonl"
        baseline.write_text(json.dumps({"type": "sample"}) + "\n")
        with self.assertRaises(SystemExit):
            check_baseline_freshness(baseline, self.gcx_path)

    def test_missing_gcx_binary_raises(self) -> None:
        baseline = Path(self.tmpdir.name) / "baseline.jsonl"
        baseline.write_text(meta_line(self._real_gcx_sha(), self._real_suite_sha()) + "\n")
        missing_gcx = Path(self.tmpdir.name) / "does-not-exist"
        with self.assertRaises(SystemExit):
            check_baseline_freshness(baseline, missing_gcx)


if __name__ == "__main__":
    unittest.main()
