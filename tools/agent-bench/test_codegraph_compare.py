"""Tests for codegraph_compare.py's exactly-once Bash-call check."""

from __future__ import annotations

import json
import unittest

from codegraph_compare import count_bash_tool_calls


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


if __name__ == "__main__":
    unittest.main()
