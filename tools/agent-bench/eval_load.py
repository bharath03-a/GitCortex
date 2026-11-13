"""Load unforced and continuous traces into flat, comparable rows.

Nothing is dropped: failed, timed-out, and aborted runs become rows like any
other, so every aggregate is over all attempted runs.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

LANGUAGE_BY_REPO = {
    "ripgrep": "rust",
    "requests": "python",
    "hono": "typescript",
    "cobra": "go",
    "gson": "java",
}


class TraceError(ValueError):
    pass


@dataclass(frozen=True)
class Row:
    trace: str
    lane: str
    client: str
    model: str
    repo: str
    language: str
    workflow: str
    task: str
    kind: str
    arm: str
    round: int
    success: bool
    status: str
    cost_usd: float | None
    total_tokens: int
    uncached_tokens: int
    wall_ms: int
    turns: int
    tool_calls: int
    gcx_calls: int
    gcx_errors: int
    adopted: bool
    first_gcx_position: int | None
    tokens_before_first_total: int | None
    tokens_before_first_uncached: int | None
    gcx_connected: bool | None
    not_ready: dict[str, Any] | None
    false_safe: bool
    repeat_reads: int
    patch_lines: int | None
    unrelated_files: int | None


@dataclass(frozen=True)
class Trace:
    path: Path
    sha256: str
    meta: dict[str, Any]
    records: tuple[dict[str, Any], ...]
    summary: dict[str, Any] | None
    rows: tuple[Row, ...]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def workflow_of_task(task_id: str, kind: str) -> str:
    if kind == "restraint":
        return "restraint"
    for word in ("impact", "callers"):
        if f"-{word}-" in task_id:
            return word
    return kind


def _model(meta: dict[str, Any], records: list[dict[str, Any]]) -> str:
    for record in records:
        model = (record.get("metrics") or {}).get("model")
        if model:
            return str(model)
        for step in record.get("steps", []):
            if step["metrics"].get("model"):
                return str(step["metrics"]["model"])
    return str(meta.get("model_requested", "unknown"))


def _unforced_row(meta: dict[str, Any], model: str, trace: str, r: dict[str, Any]) -> Row:
    m = r["metrics"]
    return Row(
        trace=trace, lane=meta["lane"], client=meta["client"], model=model,
        repo=r["repo"], language=LANGUAGE_BY_REPO.get(r["repo"], "unknown"),
        workflow=workflow_of_task(r["task_id"], r["kind"]), task=r["task_id"], kind=r["kind"],
        arm=r["arm"], round=r["round"], success=bool(r["success"]), status=r["status"],
        cost_usd=m["cost_usd"], total_tokens=m["total_tokens"], uncached_tokens=m["uncached_tokens"],
        wall_ms=r["wall_ms"], turns=m["turns"] or 0, tool_calls=m["tool_calls"],
        gcx_calls=m["gcx_calls"], gcx_errors=m["gcx_errors"], adopted=m["adopted"],
        first_gcx_position=m["first_gcx_position"],
        tokens_before_first_total=m["tokens_before_first_gcx_total"],
        tokens_before_first_uncached=m["tokens_before_first_gcx_uncached"],
        gcx_connected=m["gcx_connected"], not_ready=m["not_ready"],
        false_safe=bool(r["grade"]["claims_no_dependents"]),
        repeat_reads=m["repeat_reads"], patch_lines=None, unrelated_files=None,
    )


def _episode_row(meta: dict[str, Any], model: str, trace: str, r: dict[str, Any]) -> Row:
    m = r["metrics"]
    patch = r["patch"]
    connected = [s["metrics"]["gcx_connected"] for s in r["steps"] if s["metrics"]["gcx_connected"] is not None]
    status = "aborted" if r["aborted"] else "ok"
    return Row(
        trace=trace, lane=meta["lane"], client=meta["client"], model=model,
        repo=r["repo"], language=r["language"], workflow=r["family"], task=r["episode"], kind="episode",
        arm=r["arm"], round=r["round"], success=bool(r["success"]), status=status,
        cost_usd=m["cost_usd"], total_tokens=m["total_tokens"], uncached_tokens=m["uncached_tokens"],
        wall_ms=m["wall_ms"], turns=m["turns"], tool_calls=m["tool_calls"],
        gcx_calls=m["gcx_calls"], gcx_errors=m["gcx_errors"], adopted=m["adopted"],
        first_gcx_position=m["first_gcx_position"], tokens_before_first_total=None,
        tokens_before_first_uncached=None, gcx_connected=all(connected) if connected else None,
        not_ready=None, false_safe=bool(r["false_no_dependents"]), repeat_reads=m["repeat_reads"],
        patch_lines=patch["insertions"] + patch["deletions"], unrelated_files=len(patch["unrelated_files"]),
    )


def load_trace(path: Path) -> Trace:
    lines = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    metas = [x for x in lines if x.get("type") == "meta"]
    if len(metas) != 1:
        raise TraceError(f"{path}: expected exactly one meta line, found {len(metas)}")
    meta = metas[0]
    runs = [x for x in lines if x.get("type") in {"run", "episode"}]
    summaries = [x for x in lines if x.get("type") == "summary"]
    model = _model(meta, runs)
    build = _episode_row if meta.get("lane") == "claude-code-continuous" else _unforced_row
    return Trace(
        path=path, sha256=file_sha256(path), meta=meta, records=tuple(runs),
        summary=summaries[-1] if summaries else None,
        rows=tuple(build(meta, model, meta["label"], r) for r in runs),
    )
