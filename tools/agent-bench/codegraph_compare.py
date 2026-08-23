#!/usr/bin/env python3
"""GitCortex vs CodeGraph (https://github.com/colbymchenry/codegraph) comparison.

Reuses baseline (grep) and gcx numbers already collected against
big-repos-v1.toml (5 big repos — tokio, django, nextjs, moby, spring-boot —
10 tasks total) by agent_run.py. Only the codegraph arm is run fresh here,
against the same tasks/questions/evidence contracts, on the SAME client the
baseline was captured with (--client claude|codex) — the comparison must
hold the LLM constant and only swap the retrieval tool, so each client reuses
its own baseline file (see BASELINE_LABEL_BY_CLIENT), never another client's.
Before comparing, check_baseline_freshness() refuses to mix a baseline
captured against a different gcx build or suite version with a fresh run.

agy is not wired in yet — its cwd-vs-home-directory tool-call bug (see
docs/CROSS-AGENT-BENCHMARK-DESIGN.md) needs its own fix first.

CodeGraph has no MCP mode wired into this harness (yet) — it's driven via
its own CLI (`codegraph query`/`codegraph callers`), mirroring how
graphify_compare.py drives Graphify and how agent_run.py's codex lane drives
gcx through its own CLI rather than MCP. This is a code-only comparison:
CodeGraph also indexes docs/SQL/configs (it parses far more file types than
GitCortex's tree-sitter-only Rust/Go/Python/TS/Java scope), and that gap is
reported explicitly rather than tested around.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shlex
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from bench import load_suite, sha256  # noqa: E402
from agent_run import (  # noqa: E402
    ArmResult,
    question,
    parse_claude_events,
    parse_codex_events,
    error_arm_result,
)

REPOS_DIR = Path("/private/tmp/gcx-agent-bench/repos")
RESULTS_DIR = HERE / "results"
SUITE_PATH = HERE / "big-repos-v1.toml"
DEFAULT_GCX = HERE.parent.parent / "target" / "release" / "gcx"

# Baseline (grep-only) answers are client-specific — the comparison must hold
# the LLM constant and only swap the retrieval tool, so codex's CodeGraph arm
# must reuse codex's own baseline, not claude's.
BASELINE_LABEL_BY_CLIENT = {
    "claude": "big-repos-claude-fixed",
    "codex": "big-repos-codex",
}

TASK_IDS = [
    "tokio-search-notify",
    "tokio-callers-notify-waiters",
    "django-search-queryset",
    "django-callers-filter",
    "nextjs-search-use-router",
    "nextjs-callers-normalize-page-path",
    "moby-search-container-create",
    "moby-callers-container-kill",
    "springboot-search-spring-application",
    "springboot-callers-get-main-application-class",
]


def find_baseline_source(client: str) -> Path:
    label = BASELINE_LABEL_BY_CLIENT[client]
    candidates = sorted(RESULTS_DIR.glob(f"{label}-*.agent.jsonl"))
    if not candidates:
        raise SystemExit(f"no completed baseline file matching {label}-*.agent.jsonl in {RESULTS_DIR}")
    # Prefer the most recently written, complete (has a summary line) file.
    for path in reversed(candidates):
        lines = path.read_text(encoding="utf-8").splitlines()
        if any(json.loads(line).get("type") == "summary" for line in lines if line.strip()):
            return path
    raise SystemExit(f"no complete (summary-terminated) baseline file found among {candidates}")


def check_baseline_freshness(baseline_path: Path, gcx_path: Path) -> None:
    """Refuse to mix a baseline captured against a different gcx build or a
    different suite version with a fresh CodeGraph comparison run — a silent
    mismatch here would compare CodeGraph against a gcx/suite pair that may
    no longer exist."""
    lines = baseline_path.read_text(encoding="utf-8").splitlines()
    meta = next((json.loads(line) for line in lines if line.strip() and json.loads(line).get("type") == "meta"), None)
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

    current_suite_sha = sha256(SUITE_PATH)
    if meta.get("suite_sha256") != current_suite_sha:
        raise SystemExit(
            f"baseline {baseline_path} was captured against suite_sha256={meta.get('suite_sha256')}, "
            f"but {SUITE_PATH} currently hashes to {current_suite_sha}. "
            "The task suite changed since the baseline was captured — rerun the baseline first."
        )


def short_name(query: str) -> str:
    return query.rsplit("::", 1)[-1].rsplit(".", 1)[-1]


def codegraph_command(task) -> list[str]:
    name = short_name(task.query or "")
    if task.action == "callers":
        return ["codegraph", "callers", name]
    # default: "search"
    return ["codegraph", "query", name]


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


def count_bash_tool_calls(stdout: str) -> tuple[int, int]:
    """Count Bash `tool_use` events in a Claude stream-json transcript,
    split into (codegraph_calls, other_bash_calls).

    Consecutive identical Bash commands are collapsed before counting.
    10/10 real runs showed "codegraph command invoked 2 times" — a local
    PreToolUse gate hook in this environment intercepts and retries the
    first Bash call in every run, logging the identical command twice in a
    row, not the model genuinely retrying (see
    docs/RETRIEVAL-ACCURACY-AND-TOKEN-EFFICIENCY-SPIKE.md §1.5/§4.3). A
    model that actually retries against instructions — a different command,
    or the same command non-adjacently — is still counted as more than one
    call.
    """
    commands: list[str] = []
    for line in stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") != "assistant":
            continue
        for block in (event.get("message") or {}).get("content", []):
            if block.get("type") != "tool_use" or block.get("name") != "Bash":
                continue
            commands.append(str(block.get("input", {}).get("command", "")))

    deduped: list[str] = []
    for cmd in commands:
        if not deduped or deduped[-1] != cmd:
            deduped.append(cmd)

    codegraph_calls = sum(1 for cmd in deduped if "codegraph" in cmd)
    other_bash_calls = sum(1 for cmd in deduped if "codegraph" not in cmd)
    return codegraph_calls, other_bash_calls


def run_codegraph_arm(client: str, task, repo_dir: Path, log_path: Path) -> ArmResult:
    if client == "codex":
        return run_codegraph_arm_codex(task, repo_dir, log_path)
    return run_codegraph_arm_claude(task, repo_dir, log_path)


def run_codegraph_arm_codex(task, repo_dir: Path, log_path: Path) -> ArmResult:
    cmd = codegraph_command(task)
    exact = shlex.join(cmd)
    q = question(task)
    prompt = f"""You are evaluating a graph-first code exploration workflow using the
open-source tool CodeGraph (https://github.com/colbymchenry/codegraph).

Before any ordinary source search, run this exact command once:
{exact}

Rules:
- Run exactly one codegraph command and do not retry it.
- If it fails, state that failure and stop; do not fall back to grep.
- Use its output as your primary evidence.
- You may run at most three focused source-reading commands to verify details.
- Do not edit files. Keep the final answer concise and cite repository-relative files.

Question: {q}"""
    command = [
        "codex",
        "exec",
        "--json",
        "--ephemeral",
        "--ignore-rules",
        "--dangerously-bypass-approvals-and-sandbox",
        "-m",
        "gpt-5.4-mini",
        "-c",
        'model_reasoning_effort="low"',
        "-C",
        str(repo_dir),
        prompt,
    ]
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

    # parse_codex_events counts command_execution items whose command
    # contains gcx_marker as "gcx_calls" — repurposed here to count
    # codegraph invocations instead (same field, different tool).
    parsed = parse_codex_events(result.stdout, task, "codegraph", expect_gcx=True)
    if parsed.gcx_calls == 0:
        parsed.error = True
        parsed.error_messages.append("codegraph command was never invoked")
    elif parsed.gcx_calls > 1:
        parsed.error = True
        parsed.error_messages.append(f"codegraph command invoked {parsed.gcx_calls} times, expected 1")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"codex exited {result.returncode}")
    return parsed


def run_codegraph_arm_claude(task, repo_dir: Path, log_path: Path) -> ArmResult:
    cmd = codegraph_command(task)
    exact = " ".join(cmd)
    q = question(task)
    prompt = f"""You are evaluating a graph-first code exploration workflow using the
open-source tool CodeGraph (https://github.com/colbymchenry/codegraph).

Before any ordinary source search, run this exact command once:
{exact}

Rules:
- Run exactly one codegraph command and do not retry it.
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
        "Read Bash(codegraph:*)",
        "--disallowed-tools",
        "Grep Glob Edit Write WebSearch WebFetch mcp__gcx",
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

    # parse_claude_events checks for an mcp__gcx tool call; codegraph is a
    # Bash call instead, so score it against required evidence directly.
    parsed = parse_claude_events(result.stdout, task, expect_gcx=False)
    codegraph_calls, other_bash_calls = count_bash_tool_calls(result.stdout)
    if codegraph_calls == 0:
        parsed.error = True
        parsed.error_messages.append("codegraph command was never invoked")
    elif codegraph_calls > 1:
        parsed.error = True
        parsed.error_messages.append(f"codegraph command invoked {codegraph_calls} times, expected 1")
    if other_bash_calls:
        parsed.error = True
        parsed.error_messages.append(f"{other_bash_calls} non-codegraph Bash call(s) — allowlist leaked")
    if result.returncode != 0:
        parsed.error = True
        parsed.error_messages.append(f"claude exited {result.returncode}")
    return parsed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", choices=["claude", "codex"], default="claude")
    parser.add_argument("--gcx", type=Path, default=DEFAULT_GCX)
    args = parser.parse_args()

    _, repos, tasks = load_suite(SUITE_PATH)
    by_id = {t.id: t for t in tasks}
    baseline_source = find_baseline_source(args.client)
    check_baseline_freshness(baseline_source, args.gcx)
    existing = load_existing_arms(baseline_source)
    missing = [tid for tid in TASK_IDS if tid not in existing]
    if missing:
        raise SystemExit(f"baseline file {baseline_source} is missing task ids: {missing}")

    output = RESULTS_DIR / f"codegraph-compare-{args.client}.jsonl"
    logs = RESULTS_DIR / f"codegraph-compare-{args.client}-logs"
    logs.mkdir(parents=True, exist_ok=True)

    results = []
    for index, task_id in enumerate(TASK_IDS, 1):
        task = by_id[task_id]
        repo_dir = REPOS_DIR / task.repo
        print(f"[{index}/{len(TASK_IDS)}] {task_id}", file=sys.stderr)
        codegraph_arm = run_codegraph_arm(args.client, task, repo_dir, logs / f"{task_id}-codegraph.jsonl")
        baseline_arm = existing[task_id]["baseline"]
        gcx_arm = existing[task_id]["gcx"]
        print(
            f"  gcx={gcx_arm.quality_score:.2f}/{gcx_arm.total_tokens}tok "
            f"codegraph={codegraph_arm.quality_score:.2f}/{codegraph_arm.total_tokens}tok "
            f"{'ERR:' + '; '.join(codegraph_arm.error_messages) if codegraph_arm.error else 'ok'}",
            file=sys.stderr,
        )
        results.append(
            {
                "task_id": task_id,
                "repo": task.repo,
                "action": task.action,
                "baseline": asdict(baseline_arm),
                "gcx": asdict(gcx_arm),
                "codegraph": asdict(codegraph_arm),
            }
        )

    with output.open("w", encoding="utf-8") as handle:
        for row in results:
            handle.write(json.dumps(row, sort_keys=True) + "\n")
    print(json.dumps(
        {"output": str(output), "client": args.client, "baseline_source": str(baseline_source), "tasks": len(results)},
        indent=2,
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
