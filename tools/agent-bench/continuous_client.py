"""Claude Code commands for multi-step episodes: one persistent session, edits allowed."""

from __future__ import annotations

import uuid
from pathlib import Path

from unforced_client import READ_ONLY_BASH, Arm, mcp_config

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


def build_step_command(
    arm: Arm,
    prompt: str,
    model: str,
    effort: str,
    gcx: Path,
    max_budget_usd: float,
    session: str,
    first_step: bool,
) -> list[str]:
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
