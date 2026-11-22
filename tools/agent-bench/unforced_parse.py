"""Extract unforced-lane metrics from a recorded Claude Code stream-json transcript.

Pure functions over text: no live calls, so every metric can be recomputed from
the retained raw trace.

Token caveat: tool-use events carry streaming usage snapshots, so output tokens
at intermediate messages are undercounted. `tokens_before_first_gcx` is
therefore dominated by input-side tokens and is an approximation; the final
totals come from the `result` event.
"""

from __future__ import annotations

import json
import re
from dataclasses import asdict, dataclass, field
from typing import Any

GCX_PREFIX = "mcp__gcx"
UNCOUNTED_TOOLS = frozenset({"ToolSearch"})
READ_TOOLS = frozenset({"Read"})
TARGET_KEYS = {
    "Read": "file_path",
    "Grep": "pattern",
    "Glob": "pattern",
    "Bash": "command",
}
TARGET_LIMIT = 200

# Heuristic: does the final answer tell the user the graph was not usable?
DISCLOSURE = re.compile(
    r"not\s+(?:yet\s+)?(?:ready|indexed|available)"
    r"|index(?:ing)?\b.{0,40}\b(?:missing|stale|unavailable|unverified|degraded|not ready|in progress)"
    r"|\b(?:stale|unverified|degraded)\b"
    r"|fell back|falling back|fall back"
    r"|graph\b.{0,40}\b(?:unavailable|empty|incomplete)",
    re.IGNORECASE | re.DOTALL,
)
_STATE = re.compile(r'"index"\s*:\s*\{[^{}]*?"state"\s*:\s*"([a-z_]+)"')
_STATUS = re.compile(r'"status"\s*:\s*"([a-z_]+)"')


@dataclass(frozen=True)
class ToolEvent:
    position: int
    name: str
    is_gcx: bool
    target: str | None
    is_error: bool | None
    index_state: str | None
    status: str | None
    tokens_total_through: int
    tokens_uncached_through: int


@dataclass(frozen=True)
class NotReadyHandling:
    first_state: str | None
    first_status: str | None
    position: int
    fell_back: bool
    disclosed: bool

    @property
    def correct(self) -> bool:
        return self.fell_back and self.disclosed


@dataclass(frozen=True)
class TranscriptMetrics:
    run_status: str
    subtype: str | None
    answer: str
    model: str | None
    aux_models: tuple[str, ...]
    cost_usd: float | None
    duration_ms: int | None
    turns: int | None
    input_tokens: int
    cache_creation_tokens: int
    cache_read_tokens: int
    output_tokens: int
    total_tokens: int
    uncached_tokens: int
    tool_calls: int
    tool_search_calls: int
    gcx_calls: int
    gcx_errors: int
    gcx_error_rate: float | None
    adopted: bool
    first_gcx_position: int | None
    tokens_before_first_gcx_total: int | None
    tokens_before_first_gcx_uncached: int | None
    gcx_actions: tuple[str, ...]
    gcx_states: tuple[str, ...]
    gcx_connected: bool | None
    gcx_tool_listed: bool | None
    permission_denials: int
    repeat_reads: int
    repeat_commands: int
    not_ready: dict[str, Any] | None
    events: tuple[ToolEvent, ...] = field(default_factory=tuple)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _usage_sides(usage: dict[str, Any]) -> tuple[int, int]:
    """(total, uncached) tokens for one usage block."""
    fresh = int(usage.get("input_tokens", 0)) + int(usage.get("cache_creation_input_tokens", 0))
    cached = int(usage.get("cache_read_input_tokens", 0))
    out = int(usage.get("output_tokens", 0))
    return fresh + cached + out, fresh + out


def _dominant_model(model_usage: dict[str, Any]) -> str | None:
    if not model_usage:
        return None
    return max(model_usage, key=lambda name: float((model_usage[name] or {}).get("costUSD", 0)))


def _result_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(str(block.get("text", "")) for block in content if isinstance(block, dict))
    return ""


def _target(name: str, tool_input: dict[str, Any]) -> str | None:
    key = TARGET_KEYS.get(name)
    if key and tool_input.get(key) is not None:
        return str(tool_input[key])[:TARGET_LIMIT]
    if name.startswith(GCX_PREFIX) and tool_input.get("action"):
        return str(tool_input["action"])
    return None


def _index_signals(text: str) -> tuple[str | None, str | None]:
    """Pull `index.state` and top-level `status` out of a tool result, JSON first."""
    try:
        parsed = json.loads(text)
    except (json.JSONDecodeError, ValueError):
        parsed = None
    if isinstance(parsed, dict):
        index = parsed.get("index")
        state = index.get("state") if isinstance(index, dict) else None
        status = parsed.get("status")
        return (str(state) if state else None, str(status) if status else None)
    state_match = _STATE.search(text)
    status_match = _STATUS.search(text)
    return (
        state_match.group(1) if state_match else None,
        status_match.group(1) if status_match else None,
    )


def _is_not_ready(state: str | None, status: str | None) -> bool:
    return (state is not None and state != "ready") or status == "index_not_ready"


def _handle_not_ready(events: list[ToolEvent], answer: str) -> NotReadyHandling | None:
    for event in events:
        if event.is_gcx and _is_not_ready(event.index_state, event.status):
            fell_back = any(not later.is_gcx and later.position > event.position for later in events)
            return NotReadyHandling(
                first_state=event.index_state,
                first_status=event.status,
                position=event.position,
                fell_back=fell_back,
                disclosed=bool(DISCLOSURE.search(answer)),
            )
    return None


def _repeats(events: list[ToolEvent]) -> tuple[int, int]:
    seen_reads: set[str] = set()
    seen_commands: set[str] = set()
    repeat_reads = repeat_commands = 0
    for event in events:
        if event.is_gcx or event.target is None:
            continue
        if event.name in READ_TOOLS:
            repeat_reads += event.target in seen_reads
            seen_reads.add(event.target)
        else:
            key = f"{event.name}:{event.target}"
            repeat_commands += key in seen_commands
            seen_commands.add(key)
    return repeat_reads, repeat_commands


def _classify(result: dict[str, Any] | None, answer: str) -> tuple[str, str | None]:
    if result is None:
        return "no_result", None
    subtype = result.get("subtype")
    if subtype == "error_max_budget_usd":
        return "budget_exceeded", subtype
    if subtype in {None, "success"} and not result.get("is_error") and answer.strip():
        return "ok", subtype
    return "error", subtype


def parse_transcript(raw: str, gcx_prefix: str = GCX_PREFIX) -> TranscriptMetrics:
    message_usage: dict[str, tuple[int, int]] = {}
    pending: dict[str, dict[str, Any]] = {}
    ordered: list[str] = []
    result: dict[str, Any] | None = None
    init: dict[str, Any] | None = None
    position = 0
    tool_search_calls = 0

    for line in raw.splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init":
            init = event
        elif kind == "assistant":
            message = event.get("message") or {}
            message_id = str(message.get("id", f"anon-{len(message_usage)}"))
            if isinstance(message.get("usage"), dict):
                message_usage[message_id] = _usage_sides(message["usage"])
            through_total = sum(total for total, _ in message_usage.values())
            through_uncached = sum(uncached for _, uncached in message_usage.values())
            for block in message.get("content") or []:
                if block.get("type") != "tool_use":
                    continue
                name = str(block.get("name", ""))
                if name in UNCOUNTED_TOOLS:
                    tool_search_calls += 1
                    continue
                position += 1
                tool_id = str(block.get("id", f"anon-tool-{position}"))
                pending[tool_id] = {
                    "position": position,
                    "name": name,
                    "is_gcx": name.startswith(gcx_prefix),
                    "target": _target(name, block.get("input") or {}),
                    "is_error": None,
                    "index_state": None,
                    "status": None,
                    "tokens_total_through": through_total,
                    "tokens_uncached_through": through_uncached,
                }
                ordered.append(tool_id)
        elif kind == "user":
            for block in (event.get("message") or {}).get("content") or []:
                if not isinstance(block, dict) or block.get("type") != "tool_result":
                    continue
                entry = pending.get(str(block.get("tool_use_id", "")))
                if entry is None:
                    continue
                entry["is_error"] = bool(block.get("is_error", False))
                if entry["is_gcx"]:
                    state, status = _index_signals(_result_text(block.get("content")))
                    entry["index_state"], entry["status"] = state, status
        elif kind == "result":
            result = event

    events = [ToolEvent(**pending[tool_id]) for tool_id in ordered]
    answer = str((result or {}).get("result") or "")
    usage = (result or {}).get("usage") or {}
    total, uncached = _usage_sides(usage)
    gcx_events = [e for e in events if e.is_gcx]
    gcx_errors = sum(bool(e.is_error) for e in gcx_events)
    first = gcx_events[0] if gcx_events else None
    status, subtype = _classify(result, answer)
    handling = _handle_not_ready(events, answer)
    repeat_reads, repeat_commands = _repeats(events)
    model_usage = (result or {}).get("modelUsage") or {}
    mcp_servers = (init or {}).get("mcp_servers")
    init_tools = (init or {}).get("tools")
    cost = (result or {}).get("total_cost_usd")

    return TranscriptMetrics(
        run_status=status,
        subtype=subtype,
        answer=answer,
        model=(init or {}).get("model") or _dominant_model(model_usage),
        aux_models=tuple(sorted(m for m in model_usage if m != (init or {}).get("model"))),
        cost_usd=float(cost) if cost is not None else None,
        duration_ms=(result or {}).get("duration_ms"),
        turns=(result or {}).get("num_turns"),
        input_tokens=int(usage.get("input_tokens", 0)),
        cache_creation_tokens=int(usage.get("cache_creation_input_tokens", 0)),
        cache_read_tokens=int(usage.get("cache_read_input_tokens", 0)),
        output_tokens=int(usage.get("output_tokens", 0)),
        total_tokens=total,
        uncached_tokens=uncached,
        tool_calls=len(events),
        tool_search_calls=tool_search_calls,
        gcx_calls=len(gcx_events),
        gcx_errors=gcx_errors,
        gcx_error_rate=round(gcx_errors / len(gcx_events), 4) if gcx_events else None,
        adopted=bool(gcx_events),
        first_gcx_position=first.position if first else None,
        tokens_before_first_gcx_total=first.tokens_total_through if first else None,
        tokens_before_first_gcx_uncached=first.tokens_uncached_through if first else None,
        gcx_actions=tuple(e.target or "" for e in gcx_events),
        gcx_states=tuple(e.index_state or "" for e in gcx_events),
        gcx_connected=(
            any(s.get("name") == "gcx" and s.get("status") == "connected" for s in mcp_servers)
            if isinstance(mcp_servers, list)
            else None
        ),
        gcx_tool_listed=(
            any(str(t).startswith(gcx_prefix) for t in init_tools)
            if isinstance(init_tools, list)
            else None
        ),
        permission_denials=len((result or {}).get("permission_denials") or []),
        repeat_reads=repeat_reads,
        repeat_commands=repeat_commands,
        not_ready=(
            {**asdict(handling), "correct": handling.correct} if handling is not None else None
        ),
        events=tuple(events),
    )
