#!/usr/bin/env python3
"""Replay and aggregate native agent traces without model calls."""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

from bench import BenchError, DEFAULT_SUITE, load_suite, sha256


def geomean(values: list[float]) -> float:
    positive = [value for value in values if value > 0]
    return math.exp(sum(math.log(value) for value in positive) / len(positive)) if positive else 0.0


def normalized(text: str) -> str:
    return text.replace("\\/", "/").replace("\\\\", "/")


def load_samples(paths: list[Path], suite_sha: str) -> tuple[list[dict[str, Any]], set[str]]:
    samples: list[dict[str, Any]] = []
    models: set[str] = set()
    for path in paths:
        meta: dict[str, Any] | None = None
        for line in path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if row.get("type") == "meta":
                meta = row
                models.add(row.get("model", "unknown"))
                if row.get("suite_sha256") != suite_sha:
                    raise BenchError(f"suite hash mismatch: {path}")
            elif row.get("type") == "sample":
                if meta is None:
                    raise BenchError(f"sample before metadata: {path}")
                row["_lane"] = meta.get("lane", "unknown")
                samples.append(row)
    return samples, models


def summarize(rows: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "samples": len(rows),
        "valid": sum(row["valid"] for row in rows),
        "invalid": sum(not row["valid"] for row in rows),
        "token_geomean": round(geomean([row["token_ratio"] for row in rows]), 3),
        "uncached_token_geomean": round(
            geomean([row["uncached_token_ratio"] for row in rows]), 3
        ),
        "quality_non_inferior": sum(row["quality_non_inferior"] for row in rows),
        "command_budget_met": sum(row["command_budget_met"] for row in rows),
        "baseline_tokens": sum(row["baseline"]["total_tokens"] for row in rows),
        "gcx_tokens": sum(row["gcx"]["total_tokens"] for row in rows),
        "baseline_uncached_tokens": sum(row["baseline"]["uncached_tokens"] for row in rows),
        "gcx_uncached_tokens": sum(row["gcx"]["uncached_tokens"] for row in rows),
    }


def required_evidence(task: Any, answer: str) -> tuple[list[str], list[str]]:
    found: list[str] = []
    for needle in task.required:
        aliases = [needle]
        if task.action == "tour" and "/src" in needle:
            aliases.append(needle.split("/src", 1)[0])
        if any(alias in answer for alias in aliases):
            found.append(needle)
    return found, [needle for needle in task.required if needle not in found]


def replay(samples: list[dict[str, Any]], tasks: dict[str, Any]) -> list[dict[str, Any]]:
    replayed = []
    for row in samples:
        task = tasks.get(row["task_id"])
        if task is None:
            raise BenchError(f"unknown task in trace: {row['task_id']}")
        if row.get("_lane") == "claude-code-mcp":
            # Claude's deferred MCP schema discovery emits one ToolSearch event.
            # It is fixed client overhead, not a source verification command.
            row["gcx"]["commands"] = max(row["gcx"]["commands"] - 1, 0)
        for arm_name in ("baseline", "gcx"):
            arm = row[arm_name]
            answer = normalized(arm["answer"])
            found, missing = required_evidence(task, answer)
            arm["required_found"] = found
            arm["required_missing"] = missing
            arm["quality_score"] = len(found) / len(task.required) if task.required else 1.0
            tool_error = (
                arm_name == "gcx"
                and (arm["gcx_calls"] != 1 or arm["gcx_errors"] != 0)
            ) or (arm_name == "baseline" and arm["gcx_calls"] != 0)
            arm["error"] = (
                bool(arm["error_messages"])
                or arm["total_tokens"] == 0
                or not arm["answer"]
                or tool_error
            )
        row["quality_non_inferior"] = (
            row["gcx"]["quality_score"] >= row["baseline"]["quality_score"]
        )
        row["command_budget_met"] = row["gcx"]["commands"] <= 4
        row["valid"] = (
            not row["baseline"]["error"]
            and not row["gcx"]["error"]
            and row["quality_non_inferior"]
            and row["gcx"]["quality_score"] >= 0.5
        )
        replayed.append(row)
    return replayed


def markdown(report: dict[str, Any]) -> str:
    overall = report["overall"]
    lines = [
        "# Native Agent Gate",
        "",
        "Pinned native agent-loop validation. Lane provenance distinguishes graph CLI from MCP; they are never conflated.",
        "",
        f"Lane: `{', '.join(report['lanes'])}`  ",
        f"Model: `{', '.join(report['models'])}`  ",
        f"Samples: {overall['samples']} ({overall['valid']} valid)  ",
        f"Total-token geomean: **{overall['token_geomean']:.2f}×**  ",
        f"Uncached-token geomean: **{overall['uncached_token_geomean']:.2f}×**  ",
        f"Quality non-inferior: **{overall['quality_non_inferior']}/{overall['samples']}**  ",
        f"≤3 verification commands: **{overall['command_budget_met']}/{overall['samples']}**",
        "",
        "## By repository",
        "",
        "| Repo | Valid | Total | Uncached | Command budget |",
        "|---|---:|---:|---:|---:|",
    ]
    for name, values in report["by_repo"].items():
        lines.append(
            f"| {name} | {values['valid']}/{values['samples']} | {values['token_geomean']:.2f}× | {values['uncached_token_geomean']:.2f}× | {values['command_budget_met']}/{values['samples']} |"
        )
    lines.extend(
        [
            "",
            "## By workflow",
            "",
            "| Workflow | Valid | Total | Uncached | Command budget |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    for name, values in report["by_action"].items():
        lines.append(
            f"| {name} | {values['valid']}/{values['samples']} | {values['token_geomean']:.2f}× | {values['uncached_token_geomean']:.2f}× | {values['command_budget_met']}/{values['samples']} |"
        )
    lines.extend(
        [
            "",
            "## Methodology and limitations",
            "",
            "- Repositories and tasks are pinned in `tools/agent-bench/suite.toml`.",
            "- Baseline and graph arms run in separate ephemeral native-client sessions with deterministic alternating order.",
            "- Graph validity requires exactly one successful GitCortex command and non-inferior required source evidence.",
            "- Total and uncached usage are both reported because client cache behavior materially affects totals.",
            "- The command budget means one graph call plus at most three verification commands.",
            "- Run three complete rounds before treating a single-round aggregate as a release claim.",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traces", nargs="+", type=Path)
    parser.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    parser.add_argument("--markdown", type=Path)
    args = parser.parse_args()
    try:
        suite_path = args.suite.resolve()
        _, _, task_list = load_suite(suite_path)
        samples, models = load_samples(args.traces, sha256(suite_path))
        # Later traces replace earlier samples for the same task/round. This
        # supports targeted reruns without paying to repeat the whole suite.
        deduped: dict[tuple[str, int], dict[str, Any]] = {}
        for sample in samples:
            deduped[(sample["task_id"], sample["round"])] = sample
        samples = replay(list(deduped.values()), {task.id: task for task in task_list})
        grouped_repo: dict[str, list[dict[str, Any]]] = defaultdict(list)
        grouped_action: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for sample in samples:
            grouped_repo[sample["repo"]].append(sample)
            grouped_action[sample["action"]].append(sample)
        report = {
            "models": sorted(models),
            "lanes": sorted({sample.get("_lane", "unknown") for sample in samples}),
            "overall": summarize(samples),
            "by_repo": {name: summarize(rows) for name, rows in sorted(grouped_repo.items())},
            "by_action": {
                name: summarize(rows) for name, rows in sorted(grouped_action.items())
            },
        }
        if args.markdown:
            args.markdown.write_text(markdown(report), encoding="utf-8")
        print(json.dumps(report, indent=2))
        return 0 if report["overall"]["invalid"] == 0 else 1
    except BenchError as error:
        print(f"agent-report: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
