"""Client commands (Claude Code and Codex CLI) for multi-step episodes: one persistent session, edits allowed."""

from __future__ import annotations

import json
import uuid
from pathlib import Path

from unforced_client import READ_ONLY_BASH, Arm, codex_mcp_config_args, mcp_config

# Agents must be able to run the project's own tests while fixing a bug.
BUILD_AND_TEST_BASH = (
    "go test", "go build", "go vet", "gofmt", "python3", "python", "pytest", "python3 -m pytest",
)
EDIT_TOOLS = ("Edit", "Write")
DISALLOWED = ("NotebookEdit", "WebSearch", "WebFetch", "Task", "Agent")
SESSION_NAMESPACE = uuid.UUID("6f1c2d3e-8a4b-4c5d-9e0f-1a2b3c4d5e6f")


def session_id(run_id: str) -> str:
    """Deterministic id so a replayed plan reuses the same session names."""
    return str(uuid.uuid5(SESSION_NAMESPACE, run_id))


def allowed_tools(arm: Arm) -> str:
    tools = ["Read", "Grep", "Glob", "ToolSearch", *EDIT_TOOLS]
    tools += [f"Bash({c}:*)" for c in (*READ_ONLY_BASH, *BUILD_AND_TEST_BASH)]
    if arm.gcx:
        tools.append("mcp__gcx")
    return " ".join(tools)


def disallowed_tools(arm: Arm) -> str:
    tools = list(DISALLOWED)
    if not arm.gcx:
        tools.append("mcp__gcx")
    return " ".join(tools)


def build_codex_step_command(
    arm: Arm,
    prompt: str,
    model: str,
    effort: str,
    gcx: Path,
    session: str,
    first_step: bool,
    cwd: Path | None = None,
) -> list[str]:
    """Build a persistent `codex exec` (step 1) or `codex exec resume` (step 2+) command.

    Note: `codex exec resume` in codex-cli 0.171.0 does not accept `-C/--cd` (cwd
    comes from the subprocess working directory), and `--ephemeral` is omitted on
    every step so session state persists across the episode.
    """
    common = [
        "--json",
        "--ignore-user-config",
        "--ignore-rules",
        "--dangerously-bypass-approvals-and-sandbox",
        "-m", model,
        "-c", f"model_reasoning_effort={json.dumps(effort)}",
        *codex_mcp_config_args(arm, gcx),
    ]
    if first_step:
        command = ["codex", "exec", *common]
        if cwd is not None:
            command.extend(["-C", str(cwd)])
        command.append(prompt)
        return command
    if not session or not session.strip():
        raise ValueError("Codex resume requires a non-empty session/thread id")
    return ["codex", "exec", "resume", *common, session, prompt]


def build_step_command(
    arm: Arm,
    prompt: str,
    model: str,
    effort: str,
    gcx: Path,
    max_budget_usd: float,
    session: str,
    first_step: bool,
    client: str = "claude",
    cwd: Path | None = None,
) -> list[str]:
    if client == "codex":
        return build_codex_step_command(
            arm, prompt, model, effort, gcx, session, first_step, cwd=cwd
        )
    if client != "claude":
        raise ValueError(f"unsupported client: {client}")
    session_flag = ["--session-id", session] if first_step else ["--resume", session]
    return [
        "claude",
        "-p",
        prompt,
        "--output-format", "stream-json",
        "--verbose",
        *session_flag,
        "--strict-mcp-config",
        "--mcp-config", mcp_config(arm, gcx),
        "--setting-sources", "project,local",
        "--disable-slash-commands",
        "--model", model,
        "--effort", effort,
        "--max-budget-usd", f"{max_budget_usd:.4f}",
        "--allowed-tools", allowed_tools(arm),
        "--disallowed-tools", disallowed_tools(arm),
    ]
