"""Episode environments: seeded checkouts, hidden-test scoring, patch statistics."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from bench import BenchError, require_ok, run, run_with_lock_retry
from continuous_suite import Episode, Step

AUTHOR = ["-c", "user.email=bench@example.invalid", "-c", "user.name=bench"]
TEST_TIMEOUT_S = 600
TAIL_CHARS = 1500
NOISE_PATHS = (".gitcortex", ".claude", "__pycache__", ".pytest_cache")
TEST_FILE = re.compile(r"(_test\.go$|(^|/)test_[^/]*\.py$|(^|/)tests?/)")
INDEX_CONFIG = (
    '[index]\nlanguages = ["rust", "go", "python", "typescript", "java"]\n'
    'max_file_size_kb = 500\n[lld]\nenabled = false\n[store]\nbackend = "local"\n'
)


@dataclass(frozen=True)
class TestOutcome:
    __test__ = False

    step_id: str
    passed: bool
    returncode: int
    tail: str


@dataclass(frozen=True)
class DiffStats:
    files: tuple[str, ...]
    insertions: int
    deletions: int
    test_files: tuple[str, ...]
    patch: str


def git(repo: Path, *args: str) -> str:
    return require_ok(run(["git", *args], repo), f"git {' '.join(args[:2])}").strip()


def apply_patch(repo: Path, patch: Path) -> None:
    git(repo, "apply", "--whitespace=nowarn", str(patch))


def prepare_episode_repo(source: Path, commit: str, episode: Episode, dest: Path) -> str:
    """Copy the pinned checkout, seed the start patch, and commit it. Returns the start sha."""
    if dest.exists():
        raise BenchError(f"episode checkout already exists: {dest}")
    shutil.copytree(source, dest, symlinks=True)
    git(dest, "checkout", "-q", "-B", "gcx-bench", commit)
    git(dest, "reset", "-q", "--hard", commit)
    git(dest, "clean", "-qfdx")
    apply_patch(dest, episode.start_patch)
    git(dest, *AUTHOR, "commit", "-qam", f"episode start: {episode.id}")
    return git(dest, "rev-parse", "HEAD")


def index_repo(repo: Path, gcx: Path) -> None:
    config = repo / ".gitcortex"
    config.mkdir(exist_ok=True)
    (config / "config.toml").write_text(INDEX_CONFIG, encoding="utf-8")
    run_with_lock_retry([str(gcx), "hook"], repo, f"index {repo.name}")


def release_index(repo: Path, gcx: Path) -> None:
    """Drop the per-checkout graph store so episodes do not leak store directories."""
    run([str(gcx), "clean"], repo)


def _pytest_command(step: Step) -> list[str]:
    return [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(step.tests)]


def _tail(text: str) -> str:
    return text[-TAIL_CHARS:]


def _run_pytest(repo: Path, episode: Episode, step: Step) -> TestOutcome:
    with tempfile.TemporaryDirectory(prefix="hidden-") as tmp:
        hidden = Path(tmp) / "hidden"
        shutil.copytree(episode.hidden_dir, hidden)
        source_root = repo / "src" if (repo / "src").is_dir() else repo
        env = {
            **os.environ,
            "PYTHONPATH": str(source_root),
            "EPISODE_CHECKOUT": str(repo),
            "PYTHONDONTWRITEBYTECODE": "1",
        }
        done = subprocess.run(
            _pytest_command(step), cwd=hidden, env=env, capture_output=True, text=True,
            timeout=TEST_TIMEOUT_S, check=False,
        )
    return TestOutcome(step.id, done.returncode == 0, done.returncode, _tail(done.stdout + done.stderr))


def _run_gotest(repo: Path, episode: Episode, step: Step) -> TestOutcome:
    copied = []
    try:
        for source in sorted(episode.hidden_dir.glob("*.go")):
            target = repo / source.name
            shutil.copyfile(source, target)
            copied.append(target)
        done = subprocess.run(
            ["go", "test", "-count=1", "-run", f"^{step.tests}", "."], cwd=repo,
            capture_output=True, text=True, timeout=TEST_TIMEOUT_S, check=False,
        )
    finally:
        for target in copied:
            target.unlink(missing_ok=True)
    return TestOutcome(step.id, done.returncode == 0, done.returncode, _tail(done.stdout + done.stderr))


def run_hidden_tests(repo: Path, episode: Episode, step: Step) -> TestOutcome:
    try:
        if episode.runner == "pytest":
            return _run_pytest(repo, episode, step)
        return _run_gotest(repo, episode, step)
    except subprocess.TimeoutExpired:
        return TestOutcome(step.id, False, -1, f"hidden tests timed out after {TEST_TIMEOUT_S}s")


def _is_noise(path: str) -> bool:
    return any(part in path.split("/") for part in NOISE_PATHS)


def diff_stats(repo: Path, start_sha: str) -> DiffStats:
    """Everything the agent changed since the seeded start state, excluding tool noise."""
    git(repo, "add", "-A")
    numstat = git(repo, "diff", "--cached", "--numstat", start_sha)
    files, inserted, deleted = [], 0, 0
    for line in numstat.splitlines():
        added, removed, path = line.split("\t", 2)
        if _is_noise(path):
            continue
        files.append(path)
        inserted += int(added) if added.isdigit() else 0
        deleted += int(removed) if removed.isdigit() else 0
    patch = require_ok(run(["git", "diff", "--cached", start_sha, "--", *files], repo), "git diff") if files else ""
    return DiffStats(
        files=tuple(files),
        insertions=inserted,
        deletions=deleted,
        test_files=tuple(f for f in files if TEST_FILE.search(f)),
        patch=patch,
    )
