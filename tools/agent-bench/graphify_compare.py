#!/usr/bin/env python3
"""GitCortex vs Graphify (https://github.com/Graphify-Labs/graphify) comparison.

Reuses baseline (grep) and gcx numbers already collected in
claude-v2-full-20260810T233127Z.agent.jsonl (with requests-refactor-session's
gcx arm swapped for the corrected re-run in claude-fix-recheck, since the
original run predates a harness fix for Claude Code's deferred MCP tool
loading). Only the graphify arm is run fresh here, against the same 15 tasks,
same questions, same required-evidence contracts, same client (Claude Code).

Graphify has no MCP mode wired into this harness (yet) — it's driven via its
own CLI (`graphify affected`/`graphify explain`), mirroring how the existing
harness already drives Codex through gcx's CLI. This is a code-only
comparison: Graphify also indexes docs/SQL/configs, which GitCortex does not
attempt today, and that gap is reported explicitly rather than tested around.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from bench import load_suite, DEFAULT_SUITE  # noqa: E402
from agent_run import ArmResult, question, parse_claude_events, error_arm_result  # noqa: E402

REPOS_DIR = Path("/private/tmp/gcx-agent-bench/repos")
RESULTS_DIR = HERE / "results"
BASELINE_SOURCE = RESULTS_DIR / "claude-v2-full-20260810T233127Z.agent.jsonl"
RECHECK_SOURCE = RESULTS_DIR / "claude-fix-recheck-20260810T235852Z.agent.jsonl"

TASK_IDS = [
    "ripgrep-impact-line-number",
    "ripgrep-architecture-searcher",
    "ripgrep-refactor-searcher",
    "requests-impact-send",
    "requests-architecture-session",
    "requests-refactor-session",
    "hono-impact-compose",
    "hono-architecture-dispatch",
    "hono-refactor-compose",
    "cobra-impact-add-command",
    "cobra-architecture-parse-flags",
    "cobra-refactor-add-command",
    "gson-impact-begin-array",
    "gson-architecture-peek",
    "gson-refactor-begin-array",
]


def short_name(query: str) -> str:
    return query.rsplit("::", 1)[-1].rsplit(".", 1)[-1]


def graphify_command(task) -> list[str]:
    name = short_name(task.query or "")
    if task.action == "impact":
        return ["graphify", "affected", name]
    return ["graphify", "explain", name]


def load_existing_arms(path: Path) -> dict[str, dict[str, ArmResult]]:
    arms: dict[str, dict[str, ArmResult]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        entry = json.loads(line)
        if entry.get("type") != "sample":
            continue
        arms[entry["task_id"]] = {
            "baseline": ArmResult(**entry["baseline"]),
            "gcx": ArmResult(**entry["gcx"]),
        }
    return arms


def run_graphify_arm(task, repo_dir: Path, log_path: Path) -> ArmResult:
    cmd = graphify_command(task)
    exact = " ".join(cmd)
    q = question(task)
    prompt = f"""You are evaluating a graph-first code exploration workflow using the
open-source tool Graphify (https://github.com/Graphify-Labs/graphify).

Before any ordinary source search, run this exact command once:
{exact}

Rules:
- Run exactly one graphify command and do not retry it.
- If it fails, state that failure and stop; do not fall back to grep.
- Use its output as your primary evidence.
- You may make at most three focused Read calls to verify details.
- Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
    command = [
        "claude",
        "-p",
        prompt,
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--model",
        "haiku",
        "--effort",
        "low",
        "--max-budget-usd",
        "0.40",
        "--allowed-tools",
        "Read Bash(graphify:*)",
        "--disallowed-tools",
        "Grep Glob Edit Write WebSearch WebFetch mcp__gcx",
        "--dangerously-skip-permissions",
    ]
    env = os.environ.copy()
    env.pop("CLAUDECODE", None)
    env.pop("CLAUDE_CODE_SSE_PORT", None)
    try:
        result = subprocess.run(
            command,
            cwd=repo_dir,
            env=env,
            text=True,
            input="",
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=330,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        return error_arm_result(str(exc))
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(result.stdout, encoding="utf-8")

    # parse_claude_events checks for an mcp__gcx tool call; graphify is a Bash
    # call instead, so score it against required evidence directly.
    parsed = parse_claude_events(result.stdout, task, expect_gcx=False)
    graphify_calls = 0
    other_bash_calls = 0
    for line in result.stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "assistant":
            for block in (event.get("message") or {}).get("content", []):
                if block.get("type") != "tool_use" or block.get("name") != "Bash":
                    continue
                command = str(block.get("input", {}).get("command", ""))
                if "graphify" in command:
                    graphify_calls += 1
                else:
                    other_bash_calls += 1
    if graphify_calls == 0:
        parsed.error = True
        parsed.error_messages.append("graphify command was never invoked")
    elif graphify_calls > 1:
        parsed.error = True
        parsed.error_messages.append(f"graphify command invoked {graphify_calls} times, expected 1")
    if other_bash_calls:
        parsed.error = True
        parsed.error_messages.append(f"{other_bash_calls} non-graphify Bash call(s) — allowlist leaked")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"claude exited {result.returncode}")
    return parsed


def main() -> int:
    _, repos, tasks = load_suite(DEFAULT_SUITE)
    by_id = {t.id: t for t in tasks}
    existing = load_existing_arms(BASELINE_SOURCE)
    recheck = load_existing_arms(RECHECK_SOURCE)
    if "requests-refactor-session" in recheck:
        existing["requests-refactor-session"]["gcx"] = recheck["requests-refactor-session"]["gcx"]

    output = RESULTS_DIR / "graphify-compare.jsonl"
    logs = RESULTS_DIR / "graphify-compare-logs"
    logs.mkdir(parents=True, exist_ok=True)

    results = []
    for index, task_id in enumerate(TASK_IDS, 1):
        task = by_id[task_id]
        repo_dir = REPOS_DIR / task.repo
        print(f"[{index}/{len(TASK_IDS)}] {task_id}", file=sys.stderr)
        graphify_arm = run_graphify_arm(task, repo_dir, logs / f"{task_id}-graphify.jsonl")
        baseline_arm = existing[task_id]["baseline"]
        gcx_arm = existing[task_id]["gcx"]
        print(
            f"  gcx={gcx_arm.quality_score:.2f}/{gcx_arm.total_tokens}tok "
            f"graphify={graphify_arm.quality_score:.2f}/{graphify_arm.total_tokens}tok "
            f"{'ERR:' + '; '.join(graphify_arm.error_messages) if graphify_arm.error else 'ok'}",
            file=sys.stderr,
        )
        results.append(
            {
                "task_id": task_id,
                "repo": task.repo,
                "action": task.action,
                "baseline": asdict(baseline_arm),
                "gcx": asdict(gcx_arm),
                "graphify": asdict(graphify_arm),
            }
        )

    with output.open("w", encoding="utf-8") as handle:
        for row in results:
            handle.write(json.dumps(row, sort_keys=True) + "\n")
    print(json.dumps({"output": str(output), "tasks": len(results)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
