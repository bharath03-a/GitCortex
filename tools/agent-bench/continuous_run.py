#!/usr/bin/env python3
"""Paired continuous-coding episodes: baseline vs gcx on multi-step tasks.

Each (episode, arm, round) is one persistent Claude Code session that walks the
episode's steps. Hidden behavioral tests decide success; probe steps check for
false "no dependents" claims. Spend is capped exactly like the unforced lane,
and no episode runs without a passing verification record.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import re
import shutil
import sys
import tempfile
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from bench import DEFAULT_WORK, BenchError, sha256
from budget import BudgetExceeded, RunBudget
from continuous_client import build_step_command, session_id
from continuous_env import diff_stats, index_repo, prepare_episode_repo, release_index, run_hidden_tests
from continuous_suite import Episode, Step, load_continuous_suite
from continuous_verify import DEFAULT_CONTINUOUS_SUITE, HERE, ensure_checkout, episode_digest
from unforced_client import ARMS, Arm, RawRun, generate_guide, guide_sha256, installed_guide, run_claude
from unforced_grade import grade_answer
from unforced_parse import TranscriptMetrics, parse_transcript, repeat_counts
from unforced_run import DEFAULT_ARMS, arm_order, harness_commit, tool_version

HIDDEN_PEEK = re.compile(r"hidden_step|/hidden/|episodes/|reference-step|start\.patch")
Runner = Callable[[list[str], Path, int], RawRun]


@dataclass(frozen=True)
class EpisodeConfig:
    model: str
    effort: str
    gcx: Path
    timeout_s: int
    seed: int
    label: str


@dataclass(frozen=True)
class PlannedEpisode:
    round: int
    episode: Episode
    arm: Arm
    order_index: int


def plan_episode_runs(
    episodes: list[Episode], arms: tuple[str, ...], rounds: int, seed: int
) -> list[PlannedEpisode]:
    plan = []
    for round_number in range(1, rounds + 1):
        for episode in episodes:
            for index, name in enumerate(arm_order(seed, episode.id, round_number, arms)):
                plan.append(PlannedEpisode(round_number, episode, ARMS[name], index))
    return plan


def episode_run_id(planned: PlannedEpisode, label: str) -> str:
    return f"{label}:r{planned.round}:{planned.episode.id}:{planned.arm.name}"


def check_verification(path: Path, episodes: list[Episode]) -> str:
    """Refuse to spend on episodes whose hidden tests are not proven. Returns the file sha256."""
    record = json.loads(path.read_text(encoding="utf-8"))
    if not record.get("ok"):
        raise BenchError(f"verification record is not passing: {path}")
    verified = {e["episode"]: e for e in record.get("episodes", []) if e.get("ok")}
    for episode in episodes:
        entry = verified.get(episode.id)
        if entry is None:
            raise BenchError(f"{episode.id}: no passing verification in {path}")
        if entry["episode_sha256"] != episode_digest(episode):
            raise BenchError(f"{episode.id}: episode changed since verification; re-run continuous_verify.py")
    return sha256(path)


def _write_step_log(log_dir: Path, name: str, raw: RawRun) -> str:
    log_dir.mkdir(parents=True, exist_ok=True)
    (log_dir / f"{name}.jsonl").write_text(raw.stdout, encoding="utf-8")
    if raw.stderr.strip():
        (log_dir / f"{name}.stderr.txt").write_text(raw.stderr, encoding="utf-8")
    return f"{log_dir.name}/{name}.jsonl"


def _step_record(step: Step, raw: RawRun, metrics: TranscriptMetrics, log_rel: str) -> dict[str, Any]:
    body = metrics.to_dict()
    body.pop("events")
    return {
        "step": step.id,
        "kind": step.kind,
        "status": "timeout" if raw.timed_out else metrics.run_status,
        "wall_ms": raw.wall_ms,
        "metrics": body,
        "log": log_rel,
    }


def _aggregate(steps: list[dict[str, Any]], events: list[Any], offsets: list[int]) -> dict[str, Any]:
    """Session totals. `offsets[i]` is the count of tool calls before event i's step."""
    gcx = [(offset + event.position, event) for offset, event in zip(offsets, events) if event.is_gcx]
    repeat_reads, repeat_commands = repeat_counts(events)
    costs = [s["metrics"]["cost_usd"] for s in steps]
    return {
        "cost_usd": None if any(c is None for c in costs) else round(sum(costs), 6),
        "cost_estimated": any(c is None for c in costs),
        "total_tokens": sum(s["metrics"]["total_tokens"] for s in steps),
        "uncached_tokens": sum(s["metrics"]["uncached_tokens"] for s in steps),
        "turns": sum(s["metrics"]["turns"] or 0 for s in steps),
        "tool_calls": sum(s["metrics"]["tool_calls"] for s in steps),
        "wall_ms": sum(s["wall_ms"] for s in steps),
        "gcx_calls": len(gcx),
        "gcx_errors": sum(bool(event.is_error) for _, event in gcx),
        "adopted": bool(gcx),
        "first_gcx_position": gcx[0][0] if gcx else None,
        "repeat_reads": repeat_reads,
        "repeat_commands": repeat_commands,
    }


def _walk_steps(
    planned: PlannedEpisode, dest: Path, config: EpisodeConfig, budget: RunBudget,
    runner: Runner, session: str, name: str, log_dir: Path,
) -> tuple[list[dict[str, Any]], list[Any], list[int], str | None, RunBudget]:
    episode, arm = planned.episode, planned.arm
    steps: list[dict[str, Any]] = []
    events: list[Any] = []
    offsets: list[int] = []
    for position, step in enumerate(episode.steps):
        try:
            cap = budget.next_run_cap()
        except BudgetExceeded as stop:
            return steps, events, offsets, f"before step {step.id}: {stop}", budget
        command = build_step_command(
            arm, step.prompt, config.model, config.effort, config.gcx, cap, session, position == 0
        )
        raw = runner(command, dest, config.timeout_s)
        metrics = parse_transcript(raw.stdout)
        budget = budget.charge(metrics.cost_usd, cap)
        record = _step_record(step, raw, metrics, _write_step_log(log_dir, f"{name}-{step.id}", raw))
        if step.kind == "probe":
            record["grade"] = grade_answer(metrics.answer, step.required, (), dependents_exist=True).to_dict()
        else:
            record["tests_passed"] = run_hidden_tests(dest, episode, step).passed
        offsets.extend([len(events)] * len(metrics.events))
        events.extend(metrics.events)
        steps.append(record)
        if record["status"] != "ok":
            return steps, events, offsets, f"step {step.id} ended with status {record['status']}", budget
    return steps, events, offsets, None, budget


def run_episode(
    planned: PlannedEpisode, source: Path, commit: str, config: EpisodeConfig, budget: RunBudget,
    runner: Runner, guide: dict[str, str], scratch: Path, log_dir: Path,
) -> tuple[dict[str, Any], RunBudget]:
    episode, arm = planned.episode, planned.arm
    run_id = episode_run_id(planned, config.label)
    name = f"r{planned.round}-{episode.id}-{arm.name}"
    dest = scratch / name
    start_sha = prepare_episode_repo(source, commit, episode, dest)
    session = session_id(run_id)
    try:
        if arm.gcx:
            index_repo(dest, config.gcx)
        scope = installed_guide(dest, guide) if arm.guide else contextlib.nullcontext()
        with scope:
            steps, events, offsets, aborted, budget = _walk_steps(
                planned, dest, config, budget, runner, session, name, log_dir
            )
        finals = {s.id: run_hidden_tests(dest, episode, s).passed for s in episode.edit_steps}
        stats = diff_stats(dest, start_sha)
    finally:
        if arm.gcx:
            release_index(dest, config.gcx)
        shutil.rmtree(dest, ignore_errors=True)
    patch_rel = ""
    if stats.patch:
        log_dir.mkdir(parents=True, exist_ok=True)
        (log_dir / f"{name}.patch").write_text(stats.patch, encoding="utf-8")
        patch_rel = f"{log_dir.name}/{name}.patch"
    probe = next((s for s in steps if s["kind"] == "probe"), None)
    probe_grade = (probe or {}).get("grade")
    allowed = set(episode.reference_files) | set(stats.test_files)
    result = {
        "type": "episode",
        "run_id": run_id,
        "episode": episode.id,
        "repo": episode.repo,
        "language": episode.language,
        "family": episode.family,
        "arm": arm.name,
        "round": planned.round,
        "order_index": planned.order_index,
        "seed": config.seed,
        "session_id": session,
        "start_sha": start_sha,
        "aborted": aborted,
        "success": all(finals.values()),
        "final_tests": finals,
        "steps_passed": sum(finals.values()),
        "steps_total": len(finals),
        "steps": steps,
        "metrics": _aggregate(steps, events, offsets),
        "probe_correct": probe_grade["success"] if probe_grade else None,
        "false_no_dependents": bool(probe_grade and probe_grade["claims_no_dependents"]),
        "patch": {
            "files": list(stats.files),
            "insertions": stats.insertions,
            "deletions": stats.deletions,
            "test_files": list(stats.test_files),
            "unrelated_files": [f for f in stats.files if f not in allowed],
            "path": patch_rel,
        },
        "hidden_access_suspected": any(e.target and HIDDEN_PEEK.search(e.target) for e in events),
        "review": None,
    }
    return result, budget


def execute_episodes(
    plan: list[PlannedEpisode], sources: dict[str, tuple[Path, str]], config: EpisodeConfig,
    budget: RunBudget, runner: Runner, guide: dict[str, str], scratch: Path, log_dir: Path,
) -> Iterator[tuple[dict[str, Any], RunBudget]]:
    """Yield (record, budget) per episode. BudgetExceeded propagates before an episode starts."""
    for planned in plan:
        budget.next_run_cap()
        source, commit = sources[planned.episode.repo]
        record, budget = run_episode(planned, source, commit, config, budget, runner, guide, scratch, log_dir)
        yield record, budget


def build_meta(
    args: argparse.Namespace, suite_path: Path, core_path: Path, repos: dict[str, Any],
    episodes: list[Episode], guide: dict[str, str], plan: list[PlannedEpisode],
    budget: RunBudget, verification_sha: str,
) -> dict[str, Any]:
    names = sorted({e.repo for e in episodes})
    return {
        "type": "meta",
        "lane": "claude-code-continuous",
        "label": args.label,
        "created_at": datetime.now(UTC).isoformat(),
        "client": "claude",
        "client_version": tool_version(["claude", "--version"]),
        "model_requested": args.model,
        "reasoning": args.reasoning,
        "gcx_path": str(args.gcx.resolve()),
        "gcx_sha256": sha256(args.gcx.resolve()),
        "gcx_version": tool_version([str(args.gcx), "--version"]),
        "suite": "continuous-v1",
        "suite_sha256": sha256(suite_path),
        "core_suite_sha256": sha256(core_path),
        "repo_commits": {n: repos[n].commit for n in names},
        "episode_sha256": {e.id: episode_digest(e) for e in episodes},
        "verification_sha256": verification_sha,
        "guide_sha256": guide_sha256(guide) if guide else None,
        "harness": harness_commit(),
        "isolation": {"setting_sources": "project,local", "slash_commands_disabled": True, "auto_memory_disabled": True},
        "seed": args.seed,
        "rounds": args.rounds,
        "arms": list(args.arms),
        "planned_episodes": len(plan),
        "episodes": [e.id for e in episodes],
        "budget": budget.snapshot(),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--suite", type=Path, default=DEFAULT_CONTINUOUS_SUITE)
    p.add_argument("--gcx", type=Path, required=True)
    p.add_argument("--work", type=Path, default=DEFAULT_WORK)
    p.add_argument("--client", default="claude")
    p.add_argument("--model", default="sonnet")
    p.add_argument("--reasoning", default="medium")
    p.add_argument("--label", required=True)
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--arms", type=lambda v: tuple(v.split(",")), default=DEFAULT_ARMS)
    p.add_argument("--episode", action="append")
    p.add_argument("--verification", type=Path, help="passing continuous_verify.py record (required unless --dry-run)")
    p.add_argument("--timeout", type=int, default=900, help="seconds per step")
    p.add_argument("--max-total-usd", type=float, required=True)
    p.add_argument("--max-run-usd", type=float, default=2.0, help="cap handed to each step's client call")
    p.add_argument("--max-runs", type=int, help="cap on client calls (steps), not episodes")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--output", type=Path)
    args = p.parse_args(argv)
    if args.client != "claude":
        p.error("continuous episodes support --client claude only")
    unknown = [a for a in args.arms if a not in ARMS]
    if unknown:
        p.error(f"unknown arms {unknown}; choose from {sorted(ARMS)}")
    if not args.dry_run and args.verification is None:
        p.error("--verification is required: episodes must be proven before spending")
    return args


def main(argv: list[str] | None = None, runner: Runner = run_claude) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        suite_path = args.suite.resolve()
        _, core_path, repos, episodes = load_continuous_suite(suite_path)
        episodes = [e for e in episodes if not args.episode or e.id in args.episode]
        if not episodes:
            raise BenchError("no episodes selected")
        plan = plan_episode_runs(episodes, args.arms, args.rounds, args.seed)
        budget = RunBudget(args.max_total_usd, args.max_run_usd, args.max_runs)
        if args.dry_run:
            for p in plan:
                print(f"r{p.round} {p.episode.id} [{p.episode.language}] #{p.order_index} {p.arm.name}")
            steps = sum(len(p.episode.steps) for p in plan)
            print(f"{len(plan)} episodes, {steps} client calls; hard cap ${budget.max_total_usd:.2f}", file=sys.stderr)
            return 0
        verification_sha = check_verification(args.verification.resolve(), episodes)
        work = args.work.resolve()
        sources = {n: (ensure_checkout(repos[n], work), repos[n].commit) for n in {e.repo for e in episodes}}
        guide = generate_guide(args.gcx.resolve()) if any(a.guide for a in {p.arm for p in plan}) else {}
        config = EpisodeConfig(args.model, args.reasoning, args.gcx.resolve(), args.timeout, args.seed, args.label)
        stamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
        output = args.output or HERE / "results" / f"{args.label}-{stamp}.continuous.jsonl"
        output.parent.mkdir(parents=True, exist_ok=True)
        log_dir = output.with_name(output.stem + "-logs")
        aborted: str | None = None
        completed = 0
        with output.open("w", encoding="utf-8") as handle, tempfile.TemporaryDirectory(prefix="gcx-episodes-") as tmp:
            handle.write(json.dumps(build_meta(args, suite_path, core_path, repos, episodes, guide, plan, budget, verification_sha), sort_keys=True) + "\n")
            handle.flush()
            try:
                for record, budget in execute_episodes(plan, sources, config, budget, runner, guide, Path(tmp), log_dir):
                    handle.write(json.dumps(record, sort_keys=True) + "\n")
                    handle.flush()
                    completed += 1
                    print(f"  {record['arm']:<10} {record['episode']:<24} success={record['success']} "
                          f"steps={record['steps_passed']}/{record['steps_total']} spent=${budget.spent_usd:.4f}", file=sys.stderr)
            except BudgetExceeded as stop:
                aborted = str(stop)
                print(f"stopped: {aborted}", file=sys.stderr)
            handle.write(json.dumps({"type": "summary", "completed_episodes": completed, "planned_episodes": len(plan),
                                     "aborted": aborted, "budget": budget.snapshot()}, sort_keys=True) + "\n")
        print(json.dumps({"output": str(output), "completed": completed, "planned": len(plan),
                          "spent_usd": budget.spent_usd, "aborted": aborted}, indent=2))
        return 3 if aborted else 0
    except (BenchError, RuntimeError, ValueError, OSError) as error:
        print(f"continuous-run: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
