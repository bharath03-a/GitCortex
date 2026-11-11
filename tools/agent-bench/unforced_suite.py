"""Load the unforced-lane suite and build neutral prompts.

A prompt must read like an ordinary user question: it never names GitCortex,
gcx, MCP, or a graph. Whether the agent reaches for a graph tool is the thing
being measured.
"""

from __future__ import annotations

import re
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from agent_run import question
from bench import BenchError, Repo, Task, load_suite

KINDS = ("graph", "restraint")
PROMPT_SUFFIX = " Answer concisely and cite repository-relative file paths. Do not edit files."
_LEAK = re.compile(r"gcx|gitcortex|\bmcp\b|knowledge graph|code graph|graph (?:database|tool)", re.IGNORECASE)


@dataclass(frozen=True)
class UnforcedTask:
    id: str
    repo: str
    kind: str
    prompt: str
    required: tuple[str, ...]
    forbidden: tuple[str, ...]

    @property
    def dependents_exist(self) -> bool:
        return self.kind == "graph"


def assert_neutral(prompt: str) -> str:
    leak = _LEAK.search(prompt)
    if leak:
        raise ValueError(f"unforced prompt mentions the tool under test: {leak.group(0)!r}")
    return prompt


def _from_core(entry: dict[str, Any], core_tasks: dict[str, Task]) -> UnforcedTask:
    core = core_tasks.get(entry["core"])
    if core is None:
        raise BenchError(f"{entry['id']}: unknown core task {entry['core']}")
    return UnforcedTask(
        id=entry["id"],
        repo=core.repo,
        kind="graph",
        prompt=assert_neutral(question(core) + PROMPT_SUFFIX),
        required=core.required,
        forbidden=core.forbidden,
    )


def _standalone(entry: dict[str, Any], repos: dict[str, Repo]) -> UnforcedTask:
    if entry["repo"] not in repos:
        raise BenchError(f"{entry['id']}: unknown repo {entry['repo']}")
    required = tuple(entry.get("required", []))
    if not required:
        raise BenchError(f"{entry['id']}: restraint task needs required evidence")
    return UnforcedTask(
        id=entry["id"],
        repo=entry["repo"],
        kind=entry["kind"],
        prompt=assert_neutral(entry["prompt"] + PROMPT_SUFFIX),
        required=required,
        forbidden=tuple(entry.get("forbidden", [])),
    )


def load_unforced_suite(
    path: Path,
) -> tuple[dict[str, Any], Path, dict[str, Repo], list[UnforcedTask]]:
    """Returns (raw, core_suite_path, repos, tasks)."""
    with path.open("rb") as handle:
        raw = tomllib.load(handle)
    core_path = (path.parent / raw["core_suite"]).resolve()
    _, repos, core_list = load_suite(core_path)
    core_tasks = {task.id: task for task in core_list}
    tasks: list[UnforcedTask] = []
    seen: set[str] = set()
    for entry in raw.get("tasks", []):
        if entry.get("kind") not in KINDS:
            raise BenchError(f"{entry.get('id')}: kind must be one of {KINDS}")
        if entry["id"] in seen:
            raise BenchError(f"duplicate task id {entry['id']}")
        seen.add(entry["id"])
        tasks.append(
            _from_core(entry, core_tasks) if "core" in entry else _standalone(entry, repos)
        )
    if not tasks:
        raise BenchError(f"unforced suite has no tasks: {path}")
    return raw, core_path, repos, tasks
