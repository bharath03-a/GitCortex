#!/usr/bin/env python3
"""Build the section 4A/4B evaluation summary from retained traces.

Everything is computed from the JSONL traces, over all attempted runs. Failed,
timed-out, and aborted runs are counted, and every comparison where gcx loses
or is inconclusive is listed under `negative_results`; nothing is filtered.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter, defaultdict
from collections.abc import Sequence
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from eval_load import Row, Trace, TraceError, file_sha256, load_trace
from eval_stats import BASELINE, mean, median, paired_ratios, rate, summarize_pairs

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DIMENSIONS = ("overall", "repo", "language", "workflow", "client", "model", "lane")
PAIRED_METRICS = ("total_tokens", "uncached_tokens", "cost_usd")
MIN_ROUNDS = 3
DEFAULT_MARGIN = 0.05
GRAPH_KINDS = {"graph", "episode"}


def _round(value: float | None, places: int = 4) -> float | None:
    return None if value is None else round(value, places)


def arm_metrics(rows: Sequence[Row], is_baseline: bool) -> dict[str, Any]:
    """Section 4A outcome metrics, plus 4B discoverability for arms that have gcx."""
    costs = [r.cost_usd for r in rows if r.cost_usd is not None]
    wins = sum(r.success for r in rows)
    block: dict[str, Any] = {
        "runs": len(rows),
        "success": rate(wins, len(rows)),
        "false_safe": sum(r.false_safe for r in rows),
        "status_counts": dict(Counter(r.status for r in rows)),
        "total_tokens_mean": _round(mean([r.total_tokens for r in rows]), 1),
        "uncached_tokens_mean": _round(mean([r.uncached_tokens for r in rows]), 1),
        "cost_usd_mean": _round(mean(costs), 5),
        "cost_usd_total": _round(sum(costs), 5),
        "cost_unknown_runs": len(rows) - len(costs),
        "cost_per_success": _round(sum(costs) / wins, 5) if wins and len(costs) == len(rows) else None,
        "wall_s_mean": _round(mean([r.wall_ms / 1000 for r in rows]), 2),
        "turns_mean": _round(mean([r.turns for r in rows]), 2),
        "tool_calls_mean": _round(mean([r.tool_calls for r in rows]), 2),
        "repeat_reads_mean": _round(mean([r.repeat_reads for r in rows]), 3),
        "patch_lines_mean": _round(mean([r.patch_lines for r in rows if r.patch_lines is not None]), 1),
        "unrelated_files_total": sum(r.unrelated_files or 0 for r in rows),
        "discoverability": None,
    }
    if not is_baseline:
        graph = [r for r in rows if r.kind in GRAPH_KINDS]
        restraint = [r for r in rows if r.kind == "restraint"]
        handled = [r.not_ready for r in rows if r.not_ready]
        known = [r.gcx_connected for r in rows if r.gcx_connected is not None]
        block["discoverability"] = {
            "adoption": rate(sum(r.adopted for r in rows), len(rows)),
            "right_tool": rate(sum(r.adopted for r in graph), len(graph)),
            "restraint": rate(sum(not r.adopted for r in restraint), len(restraint)),
            "first_position_median": median([r.first_gcx_position for r in rows if r.first_gcx_position]),
            "tokens_before_first_total_median": median(
                [r.tokens_before_first_total for r in rows if r.tokens_before_first_total]
            ),
            "tokens_before_first_uncached_median": median(
                [r.tokens_before_first_uncached for r in rows if r.tokens_before_first_uncached]
            ),
            "gcx_error_rate": rate(sum(r.gcx_errors for r in rows), sum(r.gcx_calls for r in rows)),
            "not_ready_correct": rate(sum(bool(h.get("correct")) for h in handled), len(handled)),
            "connection_success": rate(sum(known), len(known)),
        }
    return block


def paired_block(rows: Sequence[Row], arm: str, margin: float, seed: int) -> dict[str, Any]:
    base = [r for r in rows if r.arm == BASELINE]
    other = [r for r in rows if r.arm == arm]
    base_rate = sum(r.success for r in base) / len(base) if base else None
    arm_rate = sum(r.success for r in other) / len(other) if other else None
    delta = None if base_rate is None or arm_rate is None else arm_rate - base_rate
    outcomes = {(r.trace, r.task, r.round): r.success for r in base}
    discordant: Counter[str] = Counter()
    for r in other:
        key = (r.trace, r.task, r.round)
        if key in outcomes and outcomes[key] != r.success:
            discordant["baseline_only" if outcomes[key] else "arm_only"] += 1
    return {
        "ratios": {m: summarize_pairs(paired_ratios(rows, m, arm), seed) for m in PAIRED_METRICS},
        "success_delta": _round(delta),
        "non_inferior": None if delta is None else delta >= -margin,
        "discordant": dict(discordant),
    }


def group_block(rows: Sequence[Row], margin: float, seed: int) -> dict[str, Any]:
    arms = sorted({r.arm for r in rows}, key=lambda a: (a != BASELINE, a))
    return {
        "runs": len(rows),
        "arms": {a: arm_metrics([r for r in rows if r.arm == a], a == BASELINE) for a in arms},
        "paired": {a: paired_block(rows, a, margin, seed) for a in arms if a != BASELINE and BASELINE in arms},
    }


def build_groups(rows: Sequence[Row], margin: float, seed: int) -> dict[str, dict[str, Any]]:
    groups: dict[str, dict[str, Any]] = {}
    for dim in DIMENSIONS:
        buckets: dict[str, list[Row]] = defaultdict(list)
        for row in rows:
            buckets["all" if dim == "overall" else str(getattr(row, dim))].append(row)
        groups[dim] = {key: group_block(items, margin, seed) for key, items in sorted(buckets.items())}
    return groups


def negative_results(groups: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    """Every loss, regression, unused tool, and inconclusive comparison. Never filtered."""
    found: list[dict[str, Any]] = []

    def add(dim: str, key: str, arm: str, metric: str, value: Any, detail: str) -> None:
        found.append({"group": f"{dim}={key}", "arm": arm, "metric": metric, "value": value, "detail": detail})

    for dim, buckets in groups.items():
        for key, block in buckets.items():
            for arm, paired in block["paired"].items():
                for metric, stats in paired["ratios"].items():
                    gm, ci = stats["geomean"], stats["ci95"]
                    if gm is not None and gm < 1.0:
                        add(dim, key, arm, f"{metric}_ratio", gm, f"{arm} used more {metric} than baseline (baseline/arm < 1)")
                    elif ci and ci[0] <= 1.0 <= ci[1]:
                        add(dim, key, arm, f"{metric}_ratio", gm, f"inconclusive: 95% interval {ci} contains 1.0")
                if paired["success_delta"] is not None and paired["success_delta"] < 0:
                    add(dim, key, arm, "success_delta", paired["success_delta"], "success rate below baseline")
                if paired["non_inferior"] is False:
                    add(dim, key, arm, "non_inferior", False, "outside the non-inferiority margin")
            for arm, metrics in block["arms"].items():
                if metrics["false_safe"]:
                    add(dim, key, arm, "false_safe", metrics["false_safe"], "claimed no dependents on a task that has them")
                bad = {s: c for s, c in metrics["status_counts"].items() if s != "ok"}
                if bad:
                    add(dim, key, arm, "run_status", bad, "runs that did not finish ok")
                disc = metrics["discoverability"]
                if disc and disc["adoption"]["den"] and disc["adoption"]["num"] == 0:
                    add(dim, key, arm, "adoption", 0, "gcx was available and never used")
    return found


def _relative(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return path.name


def _logs_manifest(trace_path: Path) -> dict[str, Any] | None:
    logs = trace_path.with_name(trace_path.stem + "-logs")
    if not logs.is_dir():
        return None
    digest = hashlib.sha256()
    files = sorted(p for p in logs.rglob("*") if p.is_file())
    for path in files:
        digest.update(f"{path.relative_to(logs)}\0{file_sha256(path)}\n".encode())
    return {"path": _relative(logs), "files": len(files), "sha256": digest.hexdigest()}


def trace_block(trace: Trace) -> dict[str, Any]:
    meta = trace.meta
    return {
        "label": meta["label"],
        "path": _relative(trace.path),
        "sha256": trace.sha256,
        "lane": meta["lane"],
        "client": meta["client"],
        "client_version": meta.get("client_version"),
        "model_requested": meta.get("model_requested"),
        "model": trace.rows[0].model if trace.rows else None,
        "created_at": meta.get("created_at"),
        "gcx_sha256": meta.get("gcx_sha256"),
        "gcx_version": meta.get("gcx_version"),
        "suite_sha256": meta.get("suite_sha256"),
        "core_suite_sha256": meta.get("core_suite_sha256"),
        "repo_commits": meta.get("repo_commits"),
        "guide_sha256": meta.get("guide_sha256"),
        "verification_sha256": meta.get("verification_sha256"),
        "harness": meta.get("harness"),
        "seed": meta.get("seed"),
        "rounds": len({r.round for r in trace.rows}),
        "planned_runs": meta.get("planned_runs", meta.get("planned_episodes")),
        "completed_runs": len(trace.records),
        "aborted": (trace.summary or {}).get("aborted"),
        "spent_usd": ((trace.summary or {}).get("budget") or {}).get("spent_usd"),
        "logs": _logs_manifest(trace.path),
    }


def build_summary(
    traces: Sequence[Trace], summary_id: str, margin: float = DEFAULT_MARGIN, seed: int = 0
) -> dict[str, Any]:
    if not traces:
        raise TraceError("no traces given")
    rows = [row for trace in traces for row in trace.rows]
    blocks = [trace_block(t) for t in traces]
    groups = build_groups(rows, margin, seed)
    return {
        "type": "eval-summary",
        "id": summary_id,
        "generated_at": datetime.now(UTC).isoformat(),
        "non_inferiority_margin": margin,
        "bootstrap_seed": seed,
        "min_rounds": MIN_ROUNDS,
        "variance_ok": all(b["rounds"] >= MIN_ROUNDS for b in blocks),
        "complete": all(b["aborted"] is None and b["planned_runs"] == b["completed_runs"] for b in blocks),
        "release_grade_harness": all(not (b["harness"] or {}).get("dirty", True) for b in blocks),
        "traces": blocks,
        "groups": groups,
        "negative_results": negative_results(groups),
        "artifacts": [{"path": b["path"], "sha256": b["sha256"]} for b in blocks],
    }


def main(argv: list[str] | None = None) -> int:
    from eval_render import render_markdown

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traces", nargs="+", type=Path)
    parser.add_argument("--id", required=True, dest="summary_id")
    parser.add_argument("--output-dir", type=Path, default=HERE / "results")
    parser.add_argument("--margin", type=float, default=DEFAULT_MARGIN, help="predeclared non-inferiority margin")
    parser.add_argument("--seed", type=int, default=0, help="bootstrap seed")
    args = parser.parse_args(argv)
    try:
        traces = [load_trace(p.resolve()) for p in args.traces]
        summary = build_summary(traces, args.summary_id, args.margin, args.seed)
    except (TraceError, KeyError, OSError, json.JSONDecodeError) as error:
        print(f"eval-report: {error}", file=sys.stderr)
        return 2
    args.output_dir.mkdir(parents=True, exist_ok=True)
    out = args.output_dir / f"{args.summary_id}.summary.json"
    out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (args.output_dir / f"{args.summary_id}.report.md").write_text(render_markdown(summary), encoding="utf-8")
    print(json.dumps({
        "summary": str(out), "variance_ok": summary["variance_ok"], "complete": summary["complete"],
        "negative_results": len(summary["negative_results"]),
    }))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
