"""Load continuous-coding episodes: pinned repo, seeded start state, steps, hidden tests."""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from bench import BenchError, Repo, load_suite
from unforced_suite import assert_neutral

STEP_KINDS = ("edit", "probe")
RUNNERS = ("pytest", "gotest")


@dataclass(frozen=True)
class Step:
    id: str
    kind: str
    prompt: str
    tests: str | None
    required: tuple[str, ...]


@dataclass(frozen=True)
class Episode:
    id: str
    repo: str
    language: str
    family: str
    dir: Path
    runner: str
    reference_files: tuple[str, ...]
    steps: tuple[Step, ...]

    @property
    def start_patch(self) -> Path:
        return self.dir / "start.patch"

    @property
    def hidden_dir(self) -> Path:
        return self.dir / "hidden"

    @property
    def edit_steps(self) -> tuple[Step, ...]:
        return tuple(s for s in self.steps if s.kind == "edit")

    def reference_patch(self, step: Step) -> Path:
        return self.dir / f"reference-{step.id}.patch"


def _step(episode_id: str, entry: dict[str, Any]) -> Step:
    if entry.get("kind") not in STEP_KINDS:
        raise BenchError(f"{episode_id}/{entry.get('id')}: kind must be one of {STEP_KINDS}")
    kind, required = entry["kind"], tuple(entry.get("required", []))
    if kind == "edit" and not entry.get("tests"):
        raise BenchError(f"{episode_id}/{entry['id']}: edit step needs hidden `tests`")
    if kind == "probe" and not required:
        raise BenchError(f"{episode_id}/{entry['id']}: probe step needs required evidence")
    return Step(entry["id"], kind, assert_neutral(entry["prompt"]), entry.get("tests"), required)


def _episode(entry: dict[str, Any], base: Path, repos: dict[str, Repo]) -> Episode:
    if entry["repo"] not in repos:
        raise BenchError(f"{entry['id']}: unknown repo {entry['repo']}")
    if entry["runner"] not in RUNNERS:
        raise BenchError(f"{entry['id']}: runner must be one of {RUNNERS}")
    episode = Episode(
        id=entry["id"],
        repo=entry["repo"],
        language=entry["language"],
        family=entry["family"],
        dir=(base / entry["dir"]).resolve(),
        runner=entry["runner"],
        reference_files=tuple(entry.get("reference_files", [])),
        steps=tuple(_step(entry["id"], s) for s in entry.get("steps", [])),
    )
    if not episode.edit_steps:
        raise BenchError(f"{episode.id}: needs at least one edit step")
    missing = [p for p in [episode.start_patch, episode.hidden_dir] if not p.exists()]
    missing += [episode.reference_patch(s) for s in episode.edit_steps if not episode.reference_patch(s).exists()]
    if missing:
        raise BenchError(f"{episode.id}: missing episode files {[str(m) for m in missing]}")
    return episode


def load_continuous_suite(path: Path) -> tuple[dict[str, Any], Path, dict[str, Repo], list[Episode]]:
    """Returns (raw, core_suite_path, repos, episodes)."""
    with path.open("rb") as handle:
        raw = tomllib.load(handle)
    core_path = (path.parent / raw["core_suite"]).resolve()
    _, repos, _ = load_suite(core_path)
    episodes = [_episode(entry, path.parent, repos) for entry in raw.get("episodes", [])]
    ids = [e.id for e in episodes]
    if not episodes or len(set(ids)) != len(ids):
        raise BenchError(f"continuous suite needs unique, non-empty episodes: {ids}")
    return raw, core_path, repos, episodes
