#!/usr/bin/env python3
"""Pinned, replayable GitCortex retrieval benchmark.

This runner is intentionally model-free. It validates graph contracts, evidence,
payload budgets, and provenance before any paid provider/native-client lane runs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import statistics
import subprocess
import sys
import time
import tomllib
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
DEFAULT_SUITE = HERE / "suite.toml"
DEFAULT_WORK = Path(os.environ.get("GCX_BENCH_WORK", "/tmp/gcx-agent-bench"))


class BenchError(RuntimeError):
    pass


@dataclass(frozen=True)
class Repo:
    name: str
    url: str
    commit: str


@dataclass(frozen=True)
class Task:
    id: str
    repo: str
    action: str
    query: str | None
    required: tuple[str, ...]
    forbidden: tuple[str, ...]
    max_bytes: int
    min_evidence: int


@dataclass
class TaskResult:
    task_id: str
    repo: str
    action: str
    command: list[str]
    returncode: int
    elapsed_ms: int
    payload_bytes: int
    payload_tokens_est: int
    required_found: list[str]
    required_missing: list[str]
    forbidden_found: list[str]
    contract_status: str | None
    evidence_count: int | None
    contract_required: bool
    valid: bool
    quality_score: float
    stdout: str
    stderr: str


def run(command: list[str], cwd: Path | None = None, timeout: int = 900) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        raise BenchError(f"command timed out after {timeout}s: {command}") from exc


def require_ok(result: subprocess.CompletedProcess[str], context: str) -> str:
    if result.returncode != 0:
        raise BenchError(
            f"{context} failed ({result.returncode})\nstdout: {result.stdout}\nstderr: {result.stderr}"
        )
    return result.stdout


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_suite(path: Path) -> tuple[dict[str, Any], dict[str, Repo], list[Task]]:
    with path.open("rb") as handle:
        raw = tomllib.load(handle)
    repos = {entry["name"]: Repo(**entry) for entry in raw.get("repos", [])}
    tasks = []
    for entry in raw.get("tasks", []):
        task = Task(
            id=entry["id"],
            repo=entry["repo"],
            action=entry["action"],
            query=entry.get("query"),
            required=tuple(entry.get("required", [])),
            forbidden=tuple(entry.get("forbidden", [])),
            max_bytes=int(entry.get("max_bytes", 8000)),
            min_evidence=int(entry.get("min_evidence", 0)),
        )
        if task.repo not in repos:
            raise BenchError(f"task {task.id} references unknown repo {task.repo}")
        tasks.append(task)
    if not repos or not tasks:
        raise BenchError(f"suite has no repos/tasks: {path}")
    return raw, repos, tasks


def prepare_repo(repo: Repo, work: Path, gcx: Path, reuse_index: bool) -> tuple[Path, float]:
    repo_dir = work / "repos" / repo.name
    repo_dir.parent.mkdir(parents=True, exist_ok=True)
    if not (repo_dir / ".git").is_dir():
        require_ok(run(["git", "clone", "--filter=blob:none", "--no-checkout", repo.url, str(repo_dir)]), f"clone {repo.name}")
    require_ok(run(["git", "fetch", "origin", repo.commit, "--depth", "1"], repo_dir), f"fetch {repo.name}")
    require_ok(run(["git", "checkout", "-B", "gcx-bench", repo.commit], repo_dir), f"checkout {repo.name}")
    actual = require_ok(run(["git", "rev-parse", "HEAD"], repo_dir), "rev-parse").strip()
    if actual != repo.commit:
        raise BenchError(f"{repo.name}: expected {repo.commit}, checked out {actual}")

    config_dir = repo_dir / ".gitcortex"
    config_dir.mkdir(exist_ok=True)
    (config_dir / "config.toml").write_text(
        '[index]\nlanguages = ["rust", "go", "python", "typescript", "java"]\n'
        'max_file_size_kb = 500\n[lld]\nenabled = false\n[store]\nbackend = "local"\n',
        encoding="utf-8",
    )
    started = time.perf_counter()
    if not reuse_index:
        require_ok(run([str(gcx), "clean"], repo_dir), f"clean {repo.name}")
        require_ok(run([str(gcx), "hook"], repo_dir), f"index {repo.name}")
    status = require_ok(run([str(gcx), "status", "--branch", "gcx-bench"], repo_dir), f"status {repo.name}")
    if "nodes:      0" in status or "nodes: 0" in status:
        raise BenchError(f"{repo.name}: index contains zero nodes")
    return repo_dir, time.perf_counter() - started


class RetrievalAdapter:
    name = "retrieval"

    def __init__(self, gcx: Path):
        self.gcx = gcx

    def command(self, task: Task) -> list[str]:
        common = [str(self.gcx), "--color", "never", "query"]
        if task.action == "search":
            if not task.query:
                raise BenchError(f"{task.id}: search requires query")
            return common + [
                "search",
                task.query,
                "--branch",
                "gcx-bench",
                "--limit",
                "10",
                "--budget-tokens",
                "600",
                "--format",
                "agent-json",
            ]
        if task.action == "tour":
            return common + ["tour", "--branch", "gcx-bench", "--limit", "6"]
        if task.action == "callers":
            if not task.query:
                raise BenchError(f"{task.id}: callers requires query")
            return common + [
                "find-callers",
                task.query,
                "--branch",
                "gcx-bench",
                "--depth",
                "1",
                "--limit",
                "15",
                "--budget-tokens",
                "600",
                "--format",
                "agent-json",
            ]
        if task.action == "subgraph":
            if not task.query:
                raise BenchError(f"{task.id}: subgraph requires query")
            return common + [
                "get-subgraph",
                task.query,
                "--branch",
                "gcx-bench",
                "--depth",
                "1",
                "--direction",
                "both",
                "--limit",
                "12",
                "--budget-tokens",
                "400",
                "--format",
                "agent-json",
            ]
        raise BenchError(f"{task.id}: unsupported action {task.action}")

    def execute(self, task: Task, repo_dir: Path) -> TaskResult:
        command = self.command(task)
        started = time.perf_counter()
        result = run(command, repo_dir)
        elapsed_ms = round((time.perf_counter() - started) * 1000)
        return score_task(
            task,
            command,
            result.returncode,
            elapsed_ms,
            result.stdout,
            result.stderr,
            require_contract=True,
        )


class LegacyRetrievalAdapter(RetrievalAdapter):
    """Adapter for pre-agent-contract binaries used in base/head comparisons."""

    name = "legacy-retrieval"

    @staticmethod
    def short_name(query: str) -> str:
        return query.rsplit("::", 1)[-1].rsplit(".", 1)[-1]

    def command(self, task: Task) -> list[str]:
        common = [str(self.gcx), "--color", "never", "query"]
        if task.action == "search":
            return common + ["search", task.query or "", "--branch", "gcx-bench", "--limit", "10"]
        if task.action == "tour":
            return common + ["tour", "--branch", "gcx-bench", "--limit", "6"]
        if task.action == "callers":
            return common + [
                "find-callers",
                self.short_name(task.query or ""),
                "--branch",
                "gcx-bench",
                "--depth",
                "1",
            ]
        if task.action == "subgraph":
            return common + [
                "get-subgraph",
                self.short_name(task.query or ""),
                "--branch",
                "gcx-bench",
                "--depth",
                "1",
                "--direction",
                "both",
            ]
        raise BenchError(f"{task.id}: unsupported action {task.action}")

    def execute(self, task: Task, repo_dir: Path) -> TaskResult:
        command = self.command(task)
        started = time.perf_counter()
        result = run(command, repo_dir)
        elapsed_ms = round((time.perf_counter() - started) * 1000)
        return score_task(
            task,
            command,
            result.returncode,
            elapsed_ms,
            result.stdout,
            result.stderr,
            require_contract=False,
        )


def score_task(
    task: Task,
    command: list[str],
    returncode: int,
    elapsed_ms: int,
    stdout: str,
    stderr: str,
    require_contract: bool,
) -> TaskResult:
    required_found = [needle for needle in task.required if needle in stdout]
    required_missing = [needle for needle in task.required if needle not in stdout]
    forbidden_found = [needle for needle in task.forbidden if needle in stdout]
    contract_status: str | None = None
    evidence_count: int | None = None
    parse_valid = True
    if require_contract and task.action in {"search", "callers", "subgraph"} and returncode == 0:
        try:
            payload = json.loads(stdout)
            contract_status = payload.get("status")
            evidence = payload.get("evidence")
            evidence_count = len(evidence) if isinstance(evidence, list) else None
            parse_valid = contract_status == "ok" and isinstance(evidence, list)
        except json.JSONDecodeError:
            parse_valid = False
    payload_bytes = len(stdout.encode("utf-8"))
    quality_score = len(required_found) / len(task.required) if task.required else 1.0
    valid = (
        returncode == 0
        and parse_valid
        and not required_missing
        and not forbidden_found
        and payload_bytes <= task.max_bytes
        and (evidence_count is None or evidence_count >= task.min_evidence)
    )
    return TaskResult(
        task_id=task.id,
        repo=task.repo,
        action=task.action,
        command=command,
        returncode=returncode,
        elapsed_ms=elapsed_ms,
        payload_bytes=payload_bytes,
        payload_tokens_est=(payload_bytes + 3) // 4,
        required_found=required_found,
        required_missing=required_missing,
        forbidden_found=forbidden_found,
        contract_status=contract_status,
        evidence_count=evidence_count,
        contract_required=require_contract,
        valid=valid,
        quality_score=quality_score,
        stdout=stdout,
        stderr=stderr,
    )


def summarize(results: list[TaskResult]) -> dict[str, Any]:
    valid = [result for result in results if result.valid]
    payloads = [result.payload_bytes for result in results]
    latencies = [result.elapsed_ms for result in results]
    by_action: dict[str, dict[str, Any]] = {}
    for action in sorted({result.action for result in results}):
        rows = [result for result in results if result.action == action]
        by_action[action] = {
            "tasks": len(rows),
            "valid": sum(result.valid for result in rows),
            "quality_mean": round(statistics.mean(result.quality_score for result in rows), 3),
            "payload_bytes_median": round(statistics.median(result.payload_bytes for result in rows)),
            "latency_ms_median": round(statistics.median(result.elapsed_ms for result in rows)),
        }
    return {
        "tasks": len(results),
        "valid": len(valid),
        "invalid": len(results) - len(valid),
        "quality_mean": round(statistics.mean(result.quality_score for result in results), 3),
        "payload_bytes_median": round(statistics.median(payloads)) if payloads else 0,
        "latency_ms_median": round(statistics.median(latencies)) if latencies else 0,
        "by_action": by_action,
    }


def write_trace(path: Path, meta: dict[str, Any], results: list[TaskResult]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        handle.write(json.dumps({"type": "meta", **meta}, sort_keys=True) + "\n")
        for result in results:
            handle.write(json.dumps({"type": "task", **asdict(result)}, sort_keys=True) + "\n")
        handle.write(json.dumps({"type": "summary", **summarize(results)}, sort_keys=True) + "\n")


def read_trace(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any]]:
    meta: dict[str, Any] = {}
    tasks: list[dict[str, Any]] = []
    summary: dict[str, Any] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        kind = row.pop("type")
        if kind == "meta":
            meta = row
        elif kind == "task":
            tasks.append(row)
        elif kind == "summary":
            summary = row
    return meta, tasks, summary


def command_run(args: argparse.Namespace) -> int:
    suite_path = args.suite.resolve()
    raw, repos, tasks = load_suite(suite_path)
    gcx = args.gcx.resolve()
    if not gcx.is_file() or not os.access(gcx, os.X_OK):
        raise BenchError(f"gcx binary is not executable: {gcx}")
    selected = [
        task
        for task in tasks
        if (not args.repo or task.repo in args.repo) and (not args.task or task.id in args.task)
    ]
    if not selected:
        raise BenchError("no tasks selected")

    work = args.work.resolve()
    adapter = RetrievalAdapter(gcx) if args.adapter == "retrieval" else LegacyRetrievalAdapter(gcx)
    repo_dirs: dict[str, Path] = {}
    index_seconds: dict[str, float] = {}
    for repo_name in dict.fromkeys(task.repo for task in selected):
        print(f"prepare {repo_name} ...", file=sys.stderr)
        repo_dir, seconds = prepare_repo(repos[repo_name], work, gcx, args.reuse_index)
        repo_dirs[repo_name] = repo_dir
        index_seconds[repo_name] = round(seconds, 3)

    results = []
    for index, task in enumerate(selected, 1):
        print(f"[{index}/{len(selected)}] {task.id}", file=sys.stderr)
        result = adapter.execute(task, repo_dirs[task.repo])
        results.append(result)
        status = "PASS" if result.valid else "FAIL"
        print(
            f"  {status} quality={result.quality_score:.2f} bytes={result.payload_bytes} ms={result.elapsed_ms}",
            file=sys.stderr,
        )

    version = require_ok(run([str(gcx), "--version"]), "gcx --version").strip()
    meta = {
        "suite": raw["suite"],
        "suite_version": raw["version"],
        "suite_sha256": sha256(suite_path),
        "adapter": adapter.name,
        "label": args.label,
        "created_at": datetime.now(UTC).isoformat(),
        "gcx_path": str(gcx),
        "gcx_version": version,
        "gcx_sha256": sha256(gcx),
        "index_seconds": index_seconds,
        "repo_commits": {name: repos[name].commit for name in repo_dirs},
    }
    output = args.output or HERE / "results" / f"{args.label}-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}.jsonl"
    write_trace(output, meta, results)
    summary = summarize(results)
    print(json.dumps({"output": str(output), **summary}, indent=2))
    return 0 if summary["invalid"] == 0 else 1


def command_replay(args: argparse.Namespace) -> int:
    _, _, tasks = load_suite(args.suite.resolve())
    by_id = {task.id: task for task in tasks}
    meta, rows, _ = read_trace(args.trace)
    rescored = []
    for row in rows:
        task = by_id.get(row["task_id"])
        if task is None:
            raise BenchError(f"trace task missing from suite: {row['task_id']}")
        rescored.append(
            score_task(
                task,
                row["command"],
                row["returncode"],
                row["elapsed_ms"],
                row["stdout"],
                row["stderr"],
                require_contract=row.get("contract_required", meta.get("adapter") == "retrieval"),
            )
        )
    summary = summarize(rescored)
    print(json.dumps({"trace": str(args.trace), "label": meta.get("label"), **summary}, indent=2))
    return 0 if summary["invalid"] == 0 else 1


def command_compare(args: argparse.Namespace) -> int:
    left_meta, left_rows, _ = read_trace(args.left)
    right_meta, right_rows, _ = read_trace(args.right)
    if left_meta.get("suite_sha256") != right_meta.get("suite_sha256"):
        raise BenchError("cannot compare traces produced by different suite manifests")
    left = {row["task_id"]: row for row in left_rows}
    right = {row["task_id"]: row for row in right_rows}
    common = sorted(left.keys() & right.keys())
    if not common:
        raise BenchError("traces have no common tasks")
    comparisons = []
    ratios = []
    for task_id in common:
        before = left[task_id]
        after = right[task_id]
        ratio = before["payload_bytes"] / after["payload_bytes"] if after["payload_bytes"] else 0
        ratios.append(ratio)
        comparisons.append(
            {
                "task_id": task_id,
                "quality_before": before["quality_score"],
                "quality_after": after["quality_score"],
                "valid_before": before["valid"],
                "valid_after": after["valid"],
                "payload_ratio": round(ratio, 3),
                "bytes_before": before["payload_bytes"],
                "bytes_after": after["payload_bytes"],
            }
        )
    report = {
        "left": left_meta.get("label"),
        "right": right_meta.get("label"),
        "tasks": len(common),
        "payload_ratio_median": round(statistics.median(ratios), 3),
        "quality_non_inferior": all(
            item["quality_after"] >= item["quality_before"] for item in comparisons
        ),
        "all_after_valid": all(item["valid_after"] for item in comparisons),
        "comparisons": comparisons,
    }
    print(json.dumps(report, indent=2))
    return 0 if report["quality_non_inferior"] and report["all_after_valid"] else 1


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    sub = root.add_subparsers(dest="command", required=True)

    run_parser = sub.add_parser("run", help="run the free retrieval suite")
    run_parser.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    run_parser.add_argument("--gcx", type=Path, required=True)
    run_parser.add_argument("--work", type=Path, default=DEFAULT_WORK)
    run_parser.add_argument("--label", required=True)
    run_parser.add_argument(
        "--adapter", choices=["retrieval", "legacy-retrieval"], default="retrieval"
    )
    run_parser.add_argument("--output", type=Path)
    run_parser.add_argument("--repo", action="append")
    run_parser.add_argument("--task", action="append")
    run_parser.add_argument("--reuse-index", action="store_true")
    run_parser.set_defaults(func=command_run)

    replay = sub.add_parser("replay", help="rescore a trace without running tools")
    replay.add_argument("trace", type=Path)
    replay.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    replay.set_defaults(func=command_replay)

    compare = sub.add_parser("compare", help="compare two traces task-by-task")
    compare.add_argument("left", type=Path)
    compare.add_argument("right", type=Path)
    compare.set_defaults(func=command_compare)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        return args.func(args)
    except BenchError as error:
        print(f"agent-bench: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
