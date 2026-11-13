#!/usr/bin/env python3
"""Unforced agent lane: does an agent choose gcx when nothing requires it?

The prompt never mentions GitCortex. Arms differ only in what the agent is
given: baseline (no gcx), gcx-bare (MCP server, no guidance), gcx-guide (MCP
server plus the guide `gcx init` generates). Reached via
`agent_run.py --unforced` or directly.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import subprocess
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, TextIO

from bench import DEFAULT_WORK, BenchError, prepare_repo, sha256
from budget import BudgetExceeded, RunBudget
from unforced_client import (
    ARMS,
    Arm,
    RawRun,
    build_command,
    generate_guide,
    guide_sha256,
    installed_guide,
    run_claude,
)
from unforced_grade import grade_answer
from unforced_parse import parse_transcript
from unforced_suite import UnforcedTask, load_unforced_suite

HERE = Path(__file__).resolve().parent
DEFAULT_UNFORCED_SUITE = HERE / "unforced-suite.toml"
DEFAULT_ARMS = ("baseline", "gcx-bare", "gcx-guide")
Runner = Callable[[list[str], Path, int], RawRun]


@dataclass(frozen=True)
class RunConfig:
    model: str
    effort: str
    gcx: Path
    timeout_s: int
    seed: int
    label: str


@dataclass(frozen=True)
class PlannedRun:
    round: int
    task: UnforcedTask
    arm: Arm
    order_index: int


def arm_order(seed: int, task_id: str, round_number: int, arms: tuple[str, ...]) -> tuple[str, ...]:
    """Seeded base order per task, rotated each round so every arm moves through every slot."""
    ranked = sorted(arms, key=lambda a: hashlib.sha256(f"{seed}:{task_id}:{a}".encode()).hexdigest())
    shift = (round_number - 1) % len(ranked)
    return tuple(ranked[shift:] + ranked[:shift])


def plan_runs(
    tasks: list[UnforcedTask], arms: tuple[str, ...], rounds: int, seed: int
) -> list[PlannedRun]:
    plan = []
    for round_number in range(1, rounds + 1):
        for task in tasks:
            for index, name in enumerate(arm_order(seed, task.id, round_number, arms)):
                plan.append(PlannedRun(round_number, task, ARMS[name], index))
    return plan


def run_id(planned: PlannedRun, label: str) -> str:
    return f"{label}:r{planned.round}:{planned.task.id}:{planned.arm.name}"


def build_record(
    planned: PlannedRun, config: RunConfig, raw: RawRun, cap: float, log_rel: str
) -> dict[str, Any]:
    metrics = parse_transcript(raw.stdout)
    task = planned.task
    grade = grade_answer(metrics.answer, task.required, task.forbidden, task.dependents_exist)
    status = "timeout" if raw.timed_out else metrics.run_status
    body = metrics.to_dict()
    events = body.pop("events")
    return {
        "type": "run",
        "run_id": run_id(planned, config.label),
        "task_id": task.id,
        "repo": task.repo,
        "kind": task.kind,
        "arm": planned.arm.name,
        "round": planned.round,
        "order_index": planned.order_index,
        "seed": config.seed,
        "prompt_sha256": hashlib.sha256(task.prompt.encode()).hexdigest(),
        "run_cap_usd": cap,
        "wall_ms": raw.wall_ms,
        "returncode": raw.returncode,
        "status": status,
        "success": status == "ok" and grade.success,
        "cost_estimated": metrics.cost_usd is None,
        "grade": grade.to_dict(),
        "metrics": body,
        "events": events,
        "log": log_rel,
    }


def write_log(log_dir: Path, planned: PlannedRun, raw: RawRun) -> str:
    log_dir.mkdir(parents=True, exist_ok=True)
    name = f"r{planned.round}-{planned.task.id}-{planned.arm.name}"
    (log_dir / f"{name}.jsonl").write_text(raw.stdout, encoding="utf-8")
    if raw.stderr.strip():
        (log_dir / f"{name}.stderr.txt").write_text(raw.stderr, encoding="utf-8")
    return f"{log_dir.name}/{name}.jsonl"


def execute_plan(
    plan: list[PlannedRun],
    repo_dirs: dict[str, Path],
    config: RunConfig,
    budget: RunBudget,
    runner: Runner,
    guide: dict[str, str],
    log_dir: Path,
) -> Iterator[tuple[dict[str, Any], RunBudget]]:
    """Yield (record, updated budget) per run. BudgetExceeded propagates before a run starts."""
    for planned in plan:
        cap = budget.next_run_cap()
        repo_dir = repo_dirs[planned.task.repo]
        command = build_command(
            planned.arm, planned.task.prompt, config.model, config.effort, config.gcx, cap
        )
        scope = installed_guide(repo_dir, guide) if planned.arm.guide else contextlib.nullcontext()
        with scope:
            raw = runner(command, repo_dir, config.timeout_s)
        record = build_record(planned, config, raw, cap, write_log(log_dir, planned, raw))
        budget = budget.charge(record["metrics"]["cost_usd"], cap)
        yield record, budget


def tool_version(command: list[str]) -> str:
    done = subprocess.run(command, capture_output=True, text=True, timeout=30, check=False)
    return (done.stdout or done.stderr).strip()


def harness_commit() -> dict[str, Any]:
    def git(*args: str) -> str:
        return subprocess.run(
            ["git", *args], cwd=HERE, capture_output=True, text=True, check=False
        ).stdout.strip()

    return {"commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain", "--", str(HERE)))}


def build_meta(
    args: argparse.Namespace, suite_path: Path, core_path: Path, repos: dict[str, Any],
    repo_names: list[str], guide: dict[str, str], plan: list[PlannedRun], budget: RunBudget,
) -> dict[str, Any]:
    return {
        "type": "meta",
        "lane": "claude-code-unforced",
        "label": args.label,
        "created_at": datetime.now(UTC).isoformat(),
        "client": "claude",
        "client_version": tool_version(["claude", "--version"]),
        "model_requested": args.model,
        "reasoning": args.reasoning,
        "gcx_path": str(args.gcx.resolve()),
        "gcx_sha256": sha256(args.gcx.resolve()),
        "gcx_version": tool_version([str(args.gcx), "--version"]),
        "suite": "unforced-v1",
        "suite_sha256": sha256(suite_path),
        "core_suite_sha256": sha256(core_path),
        "repo_commits": {name: repos[name].commit for name in repo_names},
        "guide_sha256": guide_sha256(guide),
        "harness": harness_commit(),
        "isolation": {
            "setting_sources": "project,local",
            "slash_commands_disabled": True,
            "auto_memory_disabled": True,
        },
        "seed": args.seed,
        "rounds": args.rounds,
        "arms": list(args.arms),
        "planned_runs": len(plan),
        "tasks": sorted({p.task.id for p in plan}),
        "budget": budget.snapshot(),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--suite", type=Path, default=DEFAULT_UNFORCED_SUITE)
    p.add_argument("--gcx", type=Path, required=True, help="gcx binary under test (candidate or baseline)")
    p.add_argument("--work", type=Path, default=DEFAULT_WORK)
    p.add_argument("--client", default="claude")
    p.add_argument("--model", default="sonnet")
    p.add_argument("--reasoning", default="low")
    p.add_argument("--label", required=True)
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--arms", type=lambda v: tuple(v.split(",")), default=DEFAULT_ARMS)
    p.add_argument("--repo", action="append")
    p.add_argument("--task", action="append")
    p.add_argument("--reuse-index", action="store_true")
    p.add_argument("--timeout", type=int, default=600, help="seconds per run")
    p.add_argument("--max-total-usd", type=float, required=True, help="hard spend cap for the whole run")
    p.add_argument("--max-run-usd", type=float, default=1.0, help="hard cap handed to each client run")
    p.add_argument("--max-runs", type=int)
    p.add_argument("--dry-run", action="store_true", help="print the plan and spend nothing")
    p.add_argument("--output", type=Path)
    args = p.parse_args(argv)
    if args.client != "claude":
        p.error("the unforced lane supports --client claude only: `codex exec` does not expose ad-hoc MCP tools")
    unknown = [a for a in args.arms if a not in ARMS]
    if unknown:
        p.error(f"unknown arms {unknown}; choose from {sorted(ARMS)}")
    if args.rounds < 1:
        p.error("--rounds must be at least 1")
    return args


def emit(handle: TextIO, record: dict[str, Any]) -> None:
    handle.write(json.dumps(record, sort_keys=True) + "\n")
    handle.flush()


def print_progress(record: dict[str, Any], budget: RunBudget) -> None:
    m = record["metrics"]
    cost = m["cost_usd"]
    print(
        f"  {record['arm']:<10} {record['task_id']:<36} {record['status']:<15} "
        f"success={record['success']} gcx_calls={m['gcx_calls']} cost="
        f"{'?' if cost is None else f'${cost:.4f}'} spent=${budget.spent_usd:.4f}/{budget.max_total_usd:.2f}",
        file=sys.stderr,
    )


def main(argv: list[str] | None = None, runner: Runner = run_claude) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        suite_path = args.suite.resolve()
        _, core_path, repos, tasks = load_unforced_suite(suite_path)
        tasks = [
            t for t in tasks
            if (not args.repo or t.repo in args.repo) and (not args.task or t.id in args.task)
        ]
        if not tasks:
            raise BenchError("no tasks selected")
        plan = plan_runs(tasks, args.arms, args.rounds, args.seed)
        budget = RunBudget(args.max_total_usd, args.max_run_usd, args.max_runs)
        if args.dry_run:
            for p in plan:
                print(f"r{p.round} {p.task.id} [{p.task.kind}] #{p.order_index} {p.arm.name}")
            print(f"{len(plan)} runs; hard cap ${budget.max_total_usd:.2f}", file=sys.stderr)
            return 0
        work = args.work.resolve()
        repo_names = list(dict.fromkeys(t.repo for t in tasks))
        repo_dirs = {n: prepare_repo(repos[n], work, args.gcx.resolve(), args.reuse_index)[0] for n in repo_names}
        guide = generate_guide(args.gcx.resolve())
        config = RunConfig(args.model, args.reasoning, args.gcx.resolve(), args.timeout, args.seed, args.label)
        output = args.output or HERE / "results" / f"{args.label}-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}.unforced.jsonl"
        output.parent.mkdir(parents=True, exist_ok=True)
        log_dir = output.with_name(output.stem + "-logs")
        aborted: str | None = None
        completed = 0
        with output.open("w", encoding="utf-8") as handle:
            emit(handle, build_meta(args, suite_path, core_path, repos, repo_names, guide, plan, budget))
            try:
                for record, budget in execute_plan(plan, repo_dirs, config, budget, runner, guide, log_dir):
                    emit(handle, record)
                    completed += 1
                    print_progress(record, budget)
            except BudgetExceeded as stop:
                aborted = str(stop)
                print(f"stopped: {aborted}", file=sys.stderr)
            emit(handle, {
                "type": "summary", "completed_runs": completed, "planned_runs": len(plan),
                "aborted": aborted, "budget": budget.snapshot(),
            })
        print(json.dumps({"output": str(output), "completed": completed, "planned": len(plan),
                          "spent_usd": budget.spent_usd, "aborted": aborted}, indent=2))
        return 3 if aborted else 0
    except (BenchError, RuntimeError, ValueError) as error:
        print(f"unforced-run: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
