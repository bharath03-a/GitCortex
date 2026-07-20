#!/usr/bin/env python3
"""Native client agent-loop lanes for the pinned GitCortex suite.

Codex uses an explicit graph-CLI lane because current ChatGPT-account exec
sessions do not expose ad-hoc MCP tools. Claude Code uses the compact MCP
single-dispatch tool with a strict per-run MCP configuration.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shlex
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from bench import (
    BenchError,
    DEFAULT_SUITE,
    DEFAULT_WORK,
    RetrievalAdapter,
    Task,
    load_suite,
    prepare_repo,
    sha256,
)

HERE = Path(__file__).resolve().parent


@dataclass
class ArmResult:
    input_tokens: int
    cached_input_tokens: int
    output_tokens: int
    reasoning_output_tokens: int
    total_tokens: int
    uncached_tokens: int
    commands: int
    gcx_calls: int
    gcx_errors: int
    answer: str
    required_found: list[str]
    required_missing: list[str]
    quality_score: float
    error: bool
    error_messages: list[str]


@dataclass
class AgentTaskResult:
    task_id: str
    repo: str
    action: str
    round: int
    arm_order: str
    baseline: ArmResult
    gcx: ArmResult
    token_ratio: float
    uncached_token_ratio: float
    quality_non_inferior: bool
    command_budget_met: bool
    valid: bool


def question(task: Task) -> str:
    if task.action == "search":
        return f"Where is '{task.query}' implemented in this codebase? List the relevant files and symbols."
    if task.action == "tour":
        return "Give me a concise tour of this codebase: the main components and how they fit together."
    if task.action == "callers":
        return f"If I change '{task.query}', what breaks? List direct callers and the most important impact."
    if task.action == "subgraph":
        return f"What is directly connected to '{task.query}'? Summarize callers, callees, types, and other important relationships."
    raise BenchError(f"unsupported task action: {task.action}")


def required_evidence(task: Task, answer: str) -> tuple[list[str], list[str]]:
    found: list[str] = []
    for needle in task.required:
        aliases = [needle]
        if task.action == "tour" and "/src" in needle:
            aliases.append(needle.split("/src", 1)[0])
        if any(alias in answer for alias in aliases):
            found.append(needle)
    return found, [needle for needle in task.required if needle not in found]


def parse_codex_events(raw: str, task: Task, gcx_marker: str, expect_gcx: bool) -> ArmResult:
    usage = {"input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0, "reasoning_output_tokens": 0}
    commands = 0
    gcx_calls = 0
    gcx_errors = 0
    answers: list[str] = []
    errors: list[str] = []
    for line in raw.splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        event_type = event.get("type")
        item = event.get("item") or {}
        if event_type == "item.completed" and item.get("type") == "command_execution":
            commands += 1
            command = item.get("command", "")
            if gcx_marker in command:
                gcx_calls += 1
                if item.get("exit_code") != 0:
                    gcx_errors += 1
        if event_type == "item.completed" and item.get("type") == "agent_message":
            answers.append(item.get("text", ""))
        if event_type == "turn.completed":
            event_usage = event.get("usage") or {}
            for key in usage:
                usage[key] += int(event_usage.get(key, 0))
        if event_type in {"error", "turn.failed"}:
            message = event.get("message") or (event.get("error") or {}).get("message") or str(event)
            errors.append(message)

    answer = answers[-1] if answers else ""
    # Some clients escape slashes in markdown links or emit absolute paths.
    # Required evidence is repo-relative, so normalize separators before scoring.
    normalized_answer = answer.replace("\\/", "/").replace("\\\\", "/")
    found, missing = required_evidence(task, normalized_answer)
    quality = len(found) / len(task.required) if task.required else 1.0
    total = usage["input_tokens"] + usage["output_tokens"]
    uncached = max(usage["input_tokens"] - usage["cached_input_tokens"], 0) + usage["output_tokens"]
    tool_invalid = (expect_gcx and (gcx_calls != 1 or gcx_errors != 0)) or (
        not expect_gcx and gcx_calls != 0
    )
    error = bool(errors) or total == 0 or not answer or tool_invalid
    return ArmResult(
        **usage,
        total_tokens=total,
        uncached_tokens=uncached,
        commands=commands,
        gcx_calls=gcx_calls,
        gcx_errors=gcx_errors,
        answer=answer,
        required_found=found,
        required_missing=missing,
        quality_score=quality,
        error=error,
        error_messages=errors,
    )


def claude_dispatch(task: Task) -> dict[str, Any]:
    if task.action == "search":
        return {"action": "search_code", "params": {"query": task.query, "limit": 10}}
    if task.action == "tour":
        return {"action": "start_tour", "params": {"limit": 10}}
    if task.action == "callers":
        return {
            "action": "find_callers",
            "params": {"function_name": task.query or "", "depth": 1},
        }
    if task.action == "subgraph":
        return {
            "action": "get_subgraph",
            "params": {"seed_name": task.query or "", "depth": 1, "limit": 30},
        }
    raise BenchError(f"unsupported task action: {task.action}")


def parse_claude_events(raw: str, task: Task, expect_gcx: bool) -> ArmResult:
    usage = {"input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0, "reasoning_output_tokens": 0}
    commands = 0
    gcx_calls = 0
    gcx_errors = 0
    gcx_ids: set[str] = set()
    answer = ""
    errors: list[str] = []
    for line in raw.splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "assistant":
            for block in (event.get("message") or {}).get("content", []):
                if block.get("type") != "tool_use":
                    continue
                if block.get("name") != "ToolSearch":
                    commands += 1
                if str(block.get("name", "")).startswith("mcp__gcx"):
                    gcx_calls += 1
                    gcx_ids.add(str(block.get("id", "")))
        elif event.get("type") == "user":
            for block in (event.get("message") or {}).get("content", []):
                if (
                    block.get("type") == "tool_result"
                    and str(block.get("tool_use_id", "")) in gcx_ids
                    and block.get("is_error", False)
                ):
                    gcx_errors += 1
        elif event.get("type") == "result":
            answer = event.get("result", "")
            event_usage = event.get("usage") or {}
            cache_read = int(event_usage.get("cache_read_input_tokens", 0))
            usage["input_tokens"] = (
                int(event_usage.get("input_tokens", 0))
                + int(event_usage.get("cache_creation_input_tokens", 0))
                + cache_read
            )
            usage["cached_input_tokens"] = cache_read
            usage["output_tokens"] = int(event_usage.get("output_tokens", 0))
            if event.get("subtype") not in {None, "success"}:
                errors.append(str(event.get("error") or event.get("subtype")))

    normalized_answer = answer.replace("\\/", "/").replace("\\\\", "/")
    found, missing = required_evidence(task, normalized_answer)
    quality = len(found) / len(task.required) if task.required else 1.0
    total = usage["input_tokens"] + usage["output_tokens"]
    uncached = max(usage["input_tokens"] - usage["cached_input_tokens"], 0) + usage["output_tokens"]
    tool_invalid = (expect_gcx and (gcx_calls != 1 or gcx_errors != 0)) or (
        not expect_gcx and gcx_calls != 0
    )
    return ArmResult(
        **usage,
        total_tokens=total,
        uncached_tokens=uncached,
        commands=commands,
        gcx_calls=gcx_calls,
        gcx_errors=gcx_errors,
        answer=answer,
        required_found=found,
        required_missing=missing,
        quality_score=quality,
        error=bool(errors) or total == 0 or not answer or tool_invalid,
        error_messages=errors,
    )


def run_codex_arm(
    task: Task,
    repo_dir: Path,
    model: str,
    reasoning: str,
    gcx_command: list[str],
    gcx_marker: str,
    arm: str,
    log_path: Path,
) -> ArmResult:
    q = question(task)
    if arm == "gcx":
        exact = shlex.join(gcx_command)
        prompt = f"""You are evaluating a graph-first code exploration workflow.

Before any ordinary source search, run this exact command once:
{exact}

Rules:
- Run exactly one GitCortex command and do not retry it.
- If it fails, state that failure and stop; do not fall back to grep.
- Use its ranked evidence first.
- You may run at most three focused source-reading commands to verify details.
- Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
    else:
        prompt = f"""You are evaluating ordinary codebase exploration.

Do not use GitCortex, gcx, MCP, or any graph database. Use normal source search and focused reads. Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""

    command = [
        "codex",
        "exec",
        "--json",
        "--ephemeral",
        "--ignore-rules",
        "--dangerously-bypass-approvals-and-sandbox",
        "-m",
        model,
        "-c",
        f'model_reasoning_effort="{reasoning}"',
        "-C",
        str(repo_dir),
        prompt,
    ]
    result = subprocess.run(
        command,
        text=True,
        input="",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=900,
        check=False,
    )
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(result.stdout, encoding="utf-8")
    parsed = parse_codex_events(result.stdout, task, gcx_marker, arm == "gcx")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"codex exited {result.returncode}")
    return parsed


def run_claude_arm(
    task: Task,
    repo_dir: Path,
    model: str,
    reasoning: str,
    gcx: Path,
    arm: str,
    log_path: Path,
) -> ArmResult:
    q = question(task)
    if arm == "gcx":
        dispatch = json.dumps(claude_dispatch(task), separators=(",", ":"))
        prompt = f"""You are evaluating a graph-first code exploration workflow.

Before any ordinary source search, call the GitCortex MCP `gcx` tool exactly once with this payload:
{dispatch}

Rules:
- Make exactly that one GitCortex call and do not retry it.
- After that call, never call any MCP tool again for any reason; only focused Read calls are allowed.
- If it fails, state that failure and stop; do not fall back to grep.
- Use its ranked evidence first.
- You may make at most three focused Read calls to verify details.
- Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
        mcp_config = json.dumps(
            {"mcpServers": {"gcx": {"command": str(gcx), "args": ["serve"]}}}
        )
        allowed = "Read mcp__gcx"
        disallowed = "Grep Glob Bash Edit Write WebSearch WebFetch"
    else:
        prompt = f"""You are evaluating ordinary codebase exploration.

Do not use GitCortex, gcx, MCP, or any graph database. Use normal source search and focused reads. Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
        mcp_config = '{"mcpServers":{}}'
        allowed = "Read Grep Glob Bash(grep:*) Bash(rg:*) Bash(find:*) Bash(cat:*) Bash(ls:*) Bash(head:*) Bash(sed:*)"
        disallowed = "Edit Write WebSearch WebFetch mcp__gcx"

    command = [
        "claude",
        "-p",
        prompt,
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config",
        mcp_config,
        "--model",
        model,
        "--effort",
        reasoning,
        "--max-budget-usd",
        "0.40",
        "--allowed-tools",
        allowed,
        "--disallowed-tools",
        disallowed,
    ]
    env = os.environ.copy()
    env.pop("CLAUDECODE", None)
    env.pop("CLAUDE_CODE_SSE_PORT", None)
    result = subprocess.run(
        command,
        cwd=repo_dir,
        env=env,
        text=True,
        input="",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=900,
        check=False,
    )
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(result.stdout, encoding="utf-8")
    parsed = parse_claude_events(result.stdout, task, arm == "gcx")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"claude exited {result.returncode}")
    return parsed


def geomean(values: list[float]) -> float:
    positive = [value for value in values if value > 0]
    return math.exp(sum(math.log(value) for value in positive) / len(positive)) if positive else 0.0


def summarize(results: list[AgentTaskResult]) -> dict[str, Any]:
    return {
        "samples": len(results),
        "valid": sum(result.valid for result in results),
        "invalid": sum(not result.valid for result in results),
        "token_geomean": round(geomean([result.token_ratio for result in results]), 3),
        "uncached_token_geomean": round(
            geomean([result.uncached_token_ratio for result in results]), 3
        ),
        "quality_non_inferior": sum(result.quality_non_inferior for result in results),
        "command_budget_met": sum(result.command_budget_met for result in results),
        "baseline_tokens": sum(result.baseline.total_tokens for result in results),
        "gcx_tokens": sum(result.gcx.total_tokens for result in results),
        "baseline_uncached_tokens": sum(result.baseline.uncached_tokens for result in results),
        "gcx_uncached_tokens": sum(result.gcx.uncached_tokens for result in results),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    parser.add_argument("--gcx", type=Path, required=True)
    parser.add_argument("--work", type=Path, default=DEFAULT_WORK)
    parser.add_argument("--client", choices=["codex", "claude"], default="codex")
    parser.add_argument("--model")
    parser.add_argument("--reasoning", default="low")
    parser.add_argument("--label", required=True)
    parser.add_argument("--rounds", type=int, default=1)
    parser.add_argument("--repo", action="append")
    parser.add_argument("--task", action="append")
    parser.add_argument("--reuse-index", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    model = args.model or ("gpt-5.4-mini" if args.client == "codex" else "haiku")

    try:
        suite_path = args.suite.resolve()
        raw_suite, repos, tasks = load_suite(suite_path)
        gcx = args.gcx.resolve()
        selected = [
            task
            for task in tasks
            if (not args.repo or task.repo in args.repo)
            and (not args.task or task.id in args.task)
        ]
        if not selected:
            raise BenchError("no tasks selected")
        work = args.work.resolve()
        repo_dirs: dict[str, Path] = {}
        for repo_name in dict.fromkeys(task.repo for task in selected):
            print(f"prepare {repo_name} ...", file=sys.stderr)
            repo_dirs[repo_name], _ = prepare_repo(
                repos[repo_name], work, gcx, args.reuse_index
            )

        gcx_link = work / "gcx-bin"
        gcx_link.parent.mkdir(parents=True, exist_ok=True)
        if gcx_link.exists() or gcx_link.is_symlink():
            gcx_link.unlink()
        gcx_link.symlink_to(gcx)
        retrieval = RetrievalAdapter(gcx_link)
        output = args.output or HERE / "results" / (
            f"{args.label}-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}.agent.jsonl"
        )
        logs = output.with_suffix("").with_name(output.stem + "-logs")
        results: list[AgentTaskResult] = []

        for round_number in range(1, args.rounds + 1):
            for index, task in enumerate(selected, 1):
                order = "baseline-gcx" if int(hashlib.sha256(f"{task.id}:{round_number}".encode()).hexdigest(), 16) % 2 == 0 else "gcx-baseline"
                print(
                    f"[r{round_number} {index}/{len(selected)}] {task.id} ({order})",
                    file=sys.stderr,
                )
                gcx_command = retrieval.command(task)
                arms: dict[str, ArmResult] = {}
                for arm in order.split("-"):
                    log_path = logs / f"r{round_number}-{task.id}-{arm}.jsonl"
                    if args.client == "codex":
                        arms[arm] = run_codex_arm(
                            task,
                            repo_dirs[task.repo],
                            model,
                            args.reasoning,
                            gcx_command,
                            str(gcx_link),
                            arm,
                            log_path,
                        )
                    else:
                        arms[arm] = run_claude_arm(
                            task,
                            repo_dirs[task.repo],
                            model,
                            args.reasoning,
                            gcx_link,
                            arm,
                            log_path,
                        )
                baseline, graph = arms["baseline"], arms["gcx"]
                ratio = baseline.total_tokens / graph.total_tokens if graph.total_tokens else 0
                uncached_ratio = (
                    baseline.uncached_tokens / graph.uncached_tokens
                    if graph.uncached_tokens
                    else 0
                )
                quality_non_inferior = graph.quality_score >= baseline.quality_score
                valid = (
                    not baseline.error
                    and not graph.error
                    and quality_non_inferior
                    and graph.quality_score >= 0.5
                )
                command_budget_met = graph.commands <= 4
                sample = AgentTaskResult(
                    task_id=task.id,
                    repo=task.repo,
                    action=task.action,
                    round=round_number,
                    arm_order=order,
                    baseline=baseline,
                    gcx=graph,
                    token_ratio=round(ratio, 3),
                    uncached_token_ratio=round(uncached_ratio, 3),
                    quality_non_inferior=quality_non_inferior,
                    command_budget_met=command_budget_met,
                    valid=valid,
                )
                results.append(sample)
                print(
                    f"  {'PASS' if valid else 'FAIL'} total={sample.token_ratio:.2f}x uncached={sample.uncached_token_ratio:.2f}x quality={baseline.quality_score:.2f}->{graph.quality_score:.2f} gcx_calls={graph.gcx_calls} commands={graph.commands}{'' if command_budget_met else '!'}",
                    file=sys.stderr,
                )

        version_result = subprocess.run(
            [args.client, "--version"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=30,
            check=False,
        )
        meta = {
            "type": "meta",
            "suite": raw_suite["suite"],
            "suite_sha256": sha256(suite_path),
            "lane": "codex-graph-cli" if args.client == "codex" else "claude-code-mcp",
            "client": args.client,
            "client_version": version_result.stdout.strip(),
            "label": args.label,
            "model": model,
            "reasoning": args.reasoning,
            "created_at": datetime.now(UTC).isoformat(),
            "gcx_sha256": sha256(gcx),
            "repo_commits": {name: repos[name].commit for name in repo_dirs},
        }
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("w", encoding="utf-8") as handle:
            handle.write(json.dumps(meta, sort_keys=True) + "\n")
            for result in results:
                handle.write(json.dumps({"type": "sample", **asdict(result)}, sort_keys=True) + "\n")
            handle.write(json.dumps({"type": "summary", **summarize(results)}, sort_keys=True) + "\n")
        summary = summarize(results)
        print(json.dumps({"output": str(output), **summary}, indent=2))
        return 0 if summary["invalid"] == 0 else 1
    except (BenchError, subprocess.TimeoutExpired) as error:
        print(f"agent-run: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
