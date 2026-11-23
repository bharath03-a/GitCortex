#!/usr/bin/env python3
"""GitCortex vs Graphify (https://github.com/Graphify-Labs/graphify) comparison.

Reuses baseline (grep) and gcx numbers collected against suite.toml (15 tasks
across ripgrep, requests, hono, cobra, gson) by agent_run.py. Only the graphify
arm is run fresh here, against the same tasks/questions/evidence contracts, on
the SAME client the baseline was captured with (--client claude|codex).

Graphify has no MCP mode wired into this harness (yet) — it's driven via its
own CLI (`graphify affected`/`graphify explain`), mirroring how codegraph_compare.py
drives CodeGraph and how agent_run.py's codex lane drives gcx through its CLI.
This is a code-only comparison: Graphify also indexes docs/SQL/configs, which
GitCortex does not attempt today, and that gap is reported explicitly rather
than tested around.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from bench import DEFAULT_SUITE, load_suite, sha256  # noqa: E402
from agent_run import (  # noqa: E402
    ArmResult,
    error_arm_result,
    parse_claude_events,
    parse_codex_events,
    question,
)

REPOS_DIR = Path("/private/tmp/gcx-agent-bench/repos")
RESULTS_DIR = HERE / "results"
SUITE_PATH = DEFAULT_SUITE
DEFAULT_GCX = HERE.parent.parent / "target" / "release" / "gcx"
BASELINE_SOURCE = RESULTS_DIR / "claude-v2-full-20260810T233127Z.agent.jsonl"
RECHECK_SOURCE = RESULTS_DIR / "claude-fix-recheck-20260810T235852Z.agent.jsonl"

BASELINE_LABEL_BY_CLIENT = {
    "claude": "claude-v2-full",
    "codex": "codex-v2-full",
}
DEFAULT_MODEL_BY_CLIENT = {
    "claude": "haiku",
    "codex": "gpt-5.4-mini",
}

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


def find_baseline_source(client: str, results_dir: Path = RESULTS_DIR) -> Path:
    if client == "claude" and results_dir == RESULTS_DIR and BASELINE_SOURCE.exists():
        return BASELINE_SOURCE
    label = BASELINE_LABEL_BY_CLIENT[client]
    candidates = sorted(results_dir.glob(f"{label}-*.agent.jsonl"))
    if not candidates:
        raise SystemExit(f"no completed baseline file matching {label}-*.agent.jsonl in {results_dir}")
    for path in reversed(candidates):
        lines = path.read_text(encoding="utf-8").splitlines()
        if any(json.loads(line).get("type") == "summary" for line in lines if line.strip()):
            return path
    raise SystemExit(f"no complete (summary-terminated) baseline file found among {candidates}")


def check_baseline_freshness(baseline_path: Path, gcx_path: Path, suite_path: Path = SUITE_PATH) -> None:
    """Refuse to mix a baseline captured against a different gcx build or suite version."""
    lines = baseline_path.read_text(encoding="utf-8").splitlines()
    meta = next(
        (json.loads(line) for line in lines if line.strip() and json.loads(line).get("type") == "meta"),
        None,
    )
    if meta is None:
        raise SystemExit(f"{baseline_path} has no meta line — cannot verify freshness, refusing to proceed")

    if not gcx_path.exists():
        raise SystemExit(f"gcx binary not found at {gcx_path} — build it first (cargo build --release -p gitcortex)")
    current_gcx_sha = sha256(gcx_path)
    if meta.get("gcx_sha256") != current_gcx_sha:
        raise SystemExit(
            f"baseline {baseline_path} was captured against gcx_sha256={meta.get('gcx_sha256')}, "
            f"but the current binary at {gcx_path} hashes to {current_gcx_sha}. "
            "Rebuild the baseline (agent_run.py) against the current binary before comparing, "
            "or point --gcx at the binary the baseline actually used."
        )

    current_suite_sha = sha256(suite_path)
    if meta.get("suite_sha256") != current_suite_sha:
        raise SystemExit(
            f"baseline {baseline_path} was captured against suite_sha256={meta.get('suite_sha256')}, "
            f"but {suite_path} currently hashes to {current_suite_sha}. "
            "The task suite changed since the baseline was captured — rerun the baseline first."
        )


def load_existing_arms(path: Path) -> dict[str, dict[str, ArmResult]]:
    arms: dict[str, dict[str, ArmResult]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        entry = json.loads(line)
        if entry.get("type") != "sample":
            continue
        arms[entry["task_id"]] = {
            "baseline": ArmResult(**entry["baseline"]),
            "gcx": ArmResult(**entry["gcx"]),
        }
    return arms


def build_graphify_codex_command(
    task,
    repo_dir: Path,
    model: str = "gpt-5.4-mini",
    reasoning: str = "low",
) -> list[str]:
    cmd = graphify_command(task)
    exact = shlex.join(cmd)
    q = question(task)
    prompt = f"""You are evaluating a graph-first code exploration workflow using the
open-source tool Graphify (https://github.com/Graphify-Labs/graphify).

Before any ordinary source search, run this exact command once:
{exact}

Rules:
- Run exactly one graphify command and do not retry it.
- If it fails, state that failure and stop; do not fall back to grep.
- Use its output as your primary evidence.
- You may run at most three focused source-reading commands to verify details.
- Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
    return [
        "codex",
        "exec",
        "--json",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--dangerously-bypass-approvals-and-sandbox",
        "-m",
        model,
        "-c",
        f"model_reasoning_effort={json.dumps(reasoning)}",
        "-C",
        str(repo_dir),
        prompt,
    ]


def run_graphify_arm_codex(
    task,
    repo_dir: Path,
    log_path: Path,
    model: str = "gpt-5.4-mini",
    reasoning: str = "low",
) -> ArmResult:
    command = build_graphify_codex_command(task, repo_dir, model=model, reasoning=reasoning)
    try:
        result = subprocess.run(
            command,
            text=True,
            input="",
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=900,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        return error_arm_result(str(exc))
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(result.stdout, encoding="utf-8")

    # parse_codex_events counts command_execution items containing "graphify"
    # in the gcx_calls field (repurposed here for the competitor CLI tool).
    parsed = parse_codex_events(result.stdout, task, "graphify", expect_gcx=True)
    if parsed.gcx_calls == 0:
        parsed.error = True
        parsed.error_messages.append("graphify command was never invoked")
    elif parsed.gcx_calls > 1:
        parsed.error = True
        parsed.error_messages.append(f"graphify command invoked {parsed.gcx_calls} times, expected 1")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"codex exited {result.returncode}")
    return parsed


def run_graphify_arm_claude(
    task,
    repo_dir: Path,
    log_path: Path,
    model: str = "haiku",
    reasoning: str = "low",
) -> ArmResult:
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
        model,
        "--effort",
        reasoning,
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
                bash_cmd = str(block.get("input", {}).get("command", ""))
                if "graphify" in bash_cmd:
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


def run_graphify_arm(
    client: str,
    task,
    repo_dir: Path,
    log_path: Path,
    model: str | None = None,
    reasoning: str = "low",
) -> ArmResult:
    resolved_model = model or DEFAULT_MODEL_BY_CLIENT[client]
    if client == "codex":
        return run_graphify_arm_codex(task, repo_dir, log_path, model=resolved_model, reasoning=reasoning)
    return run_graphify_arm_claude(task, repo_dir, log_path, model=resolved_model, reasoning=reasoning)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", choices=["claude", "codex"], default="claude")
    parser.add_argument("--baseline-source", type=Path, default=None)
    parser.add_argument("--gcx", type=Path, default=None)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--model", default=None)
    parser.add_argument("--reasoning", default="low")
    parser.add_argument("--repos-dir", type=Path, default=REPOS_DIR)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    _, _, tasks = load_suite(SUITE_PATH)
    by_id = {t.id: t for t in tasks}

    baseline_source = args.baseline_source or find_baseline_source(args.client)
    gcx_path = args.gcx if args.gcx is not None else (DEFAULT_GCX if args.client == "codex" else None)
    if gcx_path is not None:
        check_baseline_freshness(baseline_source, gcx_path)

    existing = load_existing_arms(baseline_source)
    if args.client == "claude" and args.baseline_source is None and RECHECK_SOURCE.exists():
        recheck = load_existing_arms(RECHECK_SOURCE)
        if "requests-refactor-session" in recheck:
            existing["requests-refactor-session"]["gcx"] = recheck["requests-refactor-session"]["gcx"]

    missing = [tid for tid in TASK_IDS if tid not in existing]
    if missing:
        raise SystemExit(f"baseline file {baseline_source} is missing task ids: {missing}")

    if args.out is not None:
        output = args.out
    elif args.client == "claude":
        output = RESULTS_DIR / "graphify-compare.jsonl"
    else:
        output = RESULTS_DIR / f"graphify-compare-{args.client}.jsonl"
    logs = output.with_suffix("").with_name(output.stem + "-logs")
    logs.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)

    results = []
    for index, task_id in enumerate(TASK_IDS, 1):
        task = by_id[task_id]
        repo_dir = args.repos_dir / task.repo
        print(f"[{index}/{len(TASK_IDS)}] {task_id}", file=sys.stderr)
        graphify_arm = run_graphify_arm(
            args.client,
            task,
            repo_dir,
            logs / f"{task_id}-graphify.jsonl",
            model=args.model,
            reasoning=args.reasoning,
        )
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
    print(json.dumps(
        {
            "output": str(output),
            "client": args.client,
            "baseline_source": str(baseline_source),
            "tasks": len(results),
        },
        indent=2,
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
