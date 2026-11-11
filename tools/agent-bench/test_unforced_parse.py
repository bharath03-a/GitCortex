"""Metric extraction from recorded (synthetic) Claude stream-json transcripts."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from typing import Any

from unforced_parse import parse_transcript

GCX = "mcp__gcx__gcx"


def usage(fresh: int, cache_read: int, out: int) -> dict[str, int]:
    return {
        "input_tokens": fresh,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": cache_read,
        "output_tokens": out,
    }


def init(connected: bool = True, tools: tuple[str, ...] = ("Read", "Bash", GCX)) -> dict[str, Any]:
    return {
        "type": "system",
        "subtype": "init",
        "model": "claude-sonnet-5-5",
        "tools": list(tools),
        "mcp_servers": [{"name": "gcx", "status": "connected" if connected else "failed"}],
    }


def assistant(message_id: str, u: dict[str, int], *blocks: dict[str, Any]) -> dict[str, Any]:
    return {"type": "assistant", "message": {"id": message_id, "usage": u, "content": list(blocks)}}


def tool_use(tool_id: str, name: str, **tool_input: Any) -> dict[str, Any]:
    return {"type": "tool_use", "id": tool_id, "name": name, "input": tool_input}


def tool_result(tool_id: str, content: Any, is_error: bool = False) -> dict[str, Any]:
    block = {"type": "tool_result", "tool_use_id": tool_id, "content": content, "is_error": is_error}
    return {"type": "user", "message": {"content": [block]}}


def result(answer: str, cost: float = 0.1, subtype: str = "success", **extra: Any) -> dict[str, Any]:
    return {
        "type": "result",
        "subtype": subtype,
        "is_error": subtype != "success",
        "result": answer,
        "total_cost_usd": cost,
        "num_turns": 4,
        "duration_ms": 9000,
        "usage": {
            "input_tokens": 10,
            "cache_creation_input_tokens": 100,
            "cache_read_input_tokens": 1000,
            "output_tokens": 50,
        },
        "modelUsage": {"claude-sonnet-5-5": {}},
        **extra,
    }


def stream(*events: dict[str, Any]) -> str:
    return "\n".join(json.dumps(event) for event in events)


ADOPTED = stream(
    init(),
    assistant("m1", usage(100, 0, 10), tool_use("t1", "Grep", pattern="AddCommand")),
    tool_result("t1", "command.go:10"),
    assistant("m2", usage(50, 100, 5), tool_use("t2", "ToolSearch", query="select:mcp__gcx__gcx")),
    tool_result("t2", "ok"),
    assistant("m3", usage(20, 200, 8), tool_use("t3", GCX, action="find_callers", params={})),
    tool_result("t3", json.dumps({"status": "ok", "index": {"state": "ready"}, "items": []})),
    assistant("m4", usage(5, 300, 40), {"type": "text", "text": "done"}),
    result("Callers are in command.go and completions.go."),
)


class AdoptionTests(unittest.TestCase):
    def test_counts_adoption_and_first_position(self) -> None:
        metrics = parse_transcript(ADOPTED)
        self.assertTrue(metrics.adopted)
        self.assertEqual(metrics.gcx_calls, 1)
        self.assertEqual(metrics.tool_calls, 2)
        self.assertEqual(metrics.tool_search_calls, 1)
        self.assertEqual(metrics.first_gcx_position, 2)
        self.assertEqual(metrics.gcx_actions, ("find_callers",))
        self.assertEqual(metrics.gcx_states, ("ready",))
        self.assertIsNone(metrics.not_ready)

    def test_tokens_before_first_gcx_cover_messages_through_the_call(self) -> None:
        metrics = parse_transcript(ADOPTED)
        self.assertEqual(metrics.tokens_before_first_gcx_total, (100 + 10) + (50 + 100 + 5) + (20 + 200 + 8))
        self.assertEqual(metrics.tokens_before_first_gcx_uncached, (100 + 10) + (50 + 5) + (20 + 8))

    def test_final_usage_cost_turns_and_connection(self) -> None:
        metrics = parse_transcript(ADOPTED)
        self.assertEqual(metrics.run_status, "ok")
        self.assertEqual(metrics.total_tokens, 10 + 100 + 1000 + 50)
        self.assertEqual(metrics.uncached_tokens, 10 + 100 + 50)
        self.assertEqual(metrics.cost_usd, 0.1)
        self.assertEqual(metrics.turns, 4)
        self.assertEqual(metrics.duration_ms, 9000)
        self.assertEqual(metrics.model, "claude-sonnet-5-5")
        self.assertTrue(metrics.gcx_connected)
        self.assertTrue(metrics.gcx_tool_listed)
        self.assertEqual(metrics.gcx_error_rate, 0.0)

    def test_model_is_the_session_model_not_an_auxiliary_one(self) -> None:
        aux = result("a", modelUsage={"claude-haiku-5-5": {"costUSD": 0.001}, "claude-sonnet-5-5": {"costUSD": 0.05}})
        metrics = parse_transcript(stream(init(), aux))
        self.assertEqual(metrics.model, "claude-sonnet-5-5")
        self.assertEqual(metrics.aux_models, ("claude-haiku-5-5",))

    def test_repeated_streaming_events_for_one_message_count_usage_once(self) -> None:
        duplicated = stream(
            init(),
            assistant("m1", usage(100, 0, 5), {"type": "thinking", "thinking": ""}),
            assistant("m1", usage(100, 0, 5), tool_use("t1", GCX, action="lookup_symbol")),
            tool_result("t1", "{}"),
            result("x"),
        )
        metrics = parse_transcript(duplicated)
        self.assertEqual(metrics.tokens_before_first_gcx_total, 105)


class NoAdoptionTests(unittest.TestCase):
    RAW = stream(
        init(),
        assistant("m1", usage(10, 0, 1), tool_use("a", "Read", file_path="command.go")),
        tool_result("a", "package cobra"),
        assistant("m2", usage(10, 0, 1), tool_use("b", "Read", file_path="command.go")),
        tool_result("b", "package cobra"),
        assistant("m3", usage(10, 0, 1), tool_use("c", "Bash", command="grep -rn AddCommand .")),
        tool_result("c", "x"),
        assistant("m4", usage(10, 0, 1), tool_use("d", "Bash", command="grep -rn AddCommand .")),
        tool_result("d", "x"),
        result("answer"),
    )

    def test_no_gcx_calls(self) -> None:
        metrics = parse_transcript(self.RAW)
        self.assertFalse(metrics.adopted)
        self.assertIsNone(metrics.first_gcx_position)
        self.assertIsNone(metrics.tokens_before_first_gcx_total)
        self.assertIsNone(metrics.gcx_error_rate)

    def test_repeat_exploration_counts_rereads_and_repeated_commands(self) -> None:
        metrics = parse_transcript(self.RAW)
        self.assertEqual(metrics.repeat_reads, 1)
        self.assertEqual(metrics.repeat_commands, 1)


class ErrorRateTests(unittest.TestCase):
    def test_error_results_counted_against_gcx_calls(self) -> None:
        raw = stream(
            init(),
            assistant("m1", usage(1, 0, 1), tool_use("a", GCX, action="bogus")),
            tool_result("a", "invalid params", is_error=True),
            assistant("m2", usage(1, 0, 1), tool_use("b", GCX, action="find_callers")),
            tool_result("b", "{}"),
            result("answer"),
        )
        metrics = parse_transcript(raw)
        self.assertEqual(metrics.gcx_calls, 2)
        self.assertEqual(metrics.gcx_errors, 1)
        self.assertEqual(metrics.gcx_error_rate, 0.5)


class NotReadyTests(unittest.TestCase):
    NOT_READY = json.dumps({"status": "index_not_ready", "index": {"state": "missing"}})

    def _run(self, fall_back: bool, answer: str) -> dict[str, Any] | None:
        events = [
            init(),
            assistant("m1", usage(1, 0, 1), tool_use("a", GCX, action="find_callers")),
            tool_result("a", self.NOT_READY),
        ]
        if fall_back:
            events += [
                assistant("m2", usage(1, 0, 1), tool_use("b", "Bash", command="grep -rn foo .")),
                tool_result("b", "x"),
            ]
        events.append(result(answer))
        return parse_transcript(stream(*events)).not_ready

    def test_fallback_with_disclosure_is_correct(self) -> None:
        handling = self._run(True, "The graph index was not ready, so I fell back to grep.")
        assert handling is not None
        self.assertEqual(handling["first_state"], "missing")
        self.assertTrue(handling["fell_back"])
        self.assertTrue(handling["disclosed"])
        self.assertTrue(handling["correct"])

    def test_fallback_without_disclosure_is_not_correct(self) -> None:
        handling = self._run(True, "Callers are in command.go.")
        assert handling is not None
        self.assertTrue(handling["fell_back"])
        self.assertFalse(handling["disclosed"])
        self.assertFalse(handling["correct"])

    def test_trusting_not_ready_result_is_not_correct(self) -> None:
        handling = self._run(False, "There are no callers.")
        assert handling is not None
        self.assertFalse(handling["fell_back"])
        self.assertFalse(handling["correct"])

    def test_ready_state_has_no_not_ready_record(self) -> None:
        self.assertIsNone(parse_transcript(ADOPTED).not_ready)

    def test_state_is_read_from_non_json_text(self) -> None:
        raw = stream(
            init(),
            assistant("m1", usage(1, 0, 1), tool_use("a", GCX, action="x")),
            tool_result("a", 'result: {"index": {"state": "stale"}, "items": []} trailing'),
            result("stale graph, fell back"),
        )
        handling = parse_transcript(raw).not_ready
        assert handling is not None
        self.assertEqual(handling["first_state"], "stale")


class RunStatusTests(unittest.TestCase):
    def test_budget_exceeded_is_distinguished(self) -> None:
        metrics = parse_transcript(stream(init(), result("partial", 0.41, "error_max_budget_usd")))
        self.assertEqual(metrics.run_status, "budget_exceeded")
        self.assertEqual(metrics.cost_usd, 0.41)

    def test_missing_result_event_is_no_result_with_unknown_cost(self) -> None:
        metrics = parse_transcript(stream(init()))
        self.assertEqual(metrics.run_status, "no_result")
        self.assertIsNone(metrics.cost_usd)
        self.assertEqual(metrics.total_tokens, 0)

    def test_empty_answer_is_error(self) -> None:
        self.assertEqual(parse_transcript(stream(init(), result(""))).run_status, "error")

    def test_connection_failure_recorded(self) -> None:
        metrics = parse_transcript(stream(init(connected=False, tools=("Read",)), result("a")))
        self.assertFalse(metrics.gcx_connected)
        self.assertFalse(metrics.gcx_tool_listed)

    def test_garbage_lines_are_ignored(self) -> None:
        metrics = parse_transcript("Warning: no stdin\n{not json\n" + stream(init(), result("a")))
        self.assertEqual(metrics.run_status, "ok")


class RealTranscriptTests(unittest.TestCase):
    """A genuine Claude Code 2.1.322 transcript (paths scrubbed) from the V1 smoke."""

    def test_real_gcx_bare_run_without_adoption(self) -> None:
        path = Path(__file__).parent / "fixtures" / "real-claude-gcx-bare-no-adoption.jsonl"
        metrics = parse_transcript(path.read_text(encoding="utf-8"))
        self.assertEqual(metrics.run_status, "ok")
        self.assertTrue(metrics.gcx_connected)
        self.assertTrue(metrics.gcx_tool_listed)
        self.assertFalse(metrics.adopted)
        self.assertEqual(metrics.tool_calls, 1)
        self.assertEqual(metrics.model, "claude-sonnet-5-5")
        self.assertEqual(metrics.aux_models, ("claude-haiku-5-5",))
        self.assertEqual(metrics.total_tokens, 24183)
        self.assertEqual(metrics.uncached_tokens, 4952)
        self.assertAlmostEqual(metrics.cost_usd or 0, 0.0311, places=4)


if __name__ == "__main__":
    unittest.main()
