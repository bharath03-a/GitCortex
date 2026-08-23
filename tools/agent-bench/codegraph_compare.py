#!/usr/bin/env python3
"""GitCortex vs CodeGraph (https://github.com/colbymchenry/codegraph) comparison.

Reuses baseline (grep) and gcx numbers already collected in
big-repos-claude-fixed-*.agent.jsonl (the completed run against
big-repos-v1.toml: 5 big repos — tokio, django, nextjs, moby, spring-boot —
10 tasks total). Only the codegraph arm is run fresh here, against the same
tasks, same questions, same required-evidence contracts, same client
(Claude Code).

CodeGraph has no MCP mode wired into this harness (yet) — it's driven via
its own CLI (`codegraph query`/`codegraph callers`), mirroring how
graphify_compare.py drives Graphify and how the existing harness drives
Codex through gcx's CLI. This is a code-only comparison: CodeGraph also
indexes docs/SQL/configs (it parses far more file types than GitCortex's
tree-sitter-only Rust/Go/Python/TS/Java scope), and that gap is reported
explicitly rather than tested around.
"""

from __future__ import annotations

import glob
import json
import os
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from bench import load_suite  # noqa: E402
from agent_run import ArmResult, question, parse_claude_events, error_arm_result  # noqa: E402

REPOS_DIR = Path("/private/tmp/gcx-agent-bench/repos")
RESULTS_DIR = HERE / "results"
SUITE_PATH = HERE / "big-repos-v1.toml"
BASELINE_LABEL = "big-repos-claude-fixed"

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


def find_baseline_source() -> Path:
    candidates = sorted(RESULTS_DIR.glob(f"{BASELINE_LABEL}-*.agent.jsonl"))
    if not candidates:
        raise SystemExit(f"no completed baseline file matching {BASELINE_LABEL}-*.agent.jsonl in {RESULTS_DIR}")
    # Prefer the most recently written, complete (has a summary line) file.
    for path in reversed(candidates):
        lines = path.read_text(encoding="utf-8").splitlines()
        if any(json.loads(line).get("type") == "summary" for line in lines if line.strip()):
            return path
    raise SystemExit(f"no complete (summary-terminated) baseline file found among {candidates}")


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


def run_codegraph_arm(task, repo_dir: Path, log_path: Path) -> ArmResult:
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
    _, repos, tasks = load_suite(SUITE_PATH)
    by_id = {t.id: t for t in tasks}
    baseline_source = find_baseline_source()
    existing = load_existing_arms(baseline_source)
    missing = [tid for tid in TASK_IDS if tid not in existing]
    if missing:
        raise SystemExit(f"baseline file {baseline_source} is missing task ids: {missing}")

    output = RESULTS_DIR / "codegraph-compare.jsonl"
    logs = RESULTS_DIR / "codegraph-compare-logs"
    logs.mkdir(parents=True, exist_ok=True)

    results = []
    for index, task_id in enumerate(TASK_IDS, 1):
        task = by_id[task_id]
        repo_dir = REPOS_DIR / task.repo
        print(f"[{index}/{len(TASK_IDS)}] {task_id}", file=sys.stderr)
        codegraph_arm = run_codegraph_arm(task, repo_dir, logs / f"{task_id}-codegraph.jsonl")
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
    print(json.dumps({"output": str(output), "baseline_source": str(baseline_source), "tasks": len(results)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
