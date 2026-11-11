#!/usr/bin/env python3
"""Prove each episode's hidden tests fail on the seeded start state and pass on the reference.

Matrix per episode: after applying reference patches 0..k in order, edit steps
<= k must pass and edit steps > k must fail. Any mismatch fails verification, and
`continuous_run.py` refuses to spend on an episode set without a passing record.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from bench import DEFAULT_WORK, BenchError, Repo, require_ok, run, sha256
from continuous_env import apply_patch, git, prepare_episode_repo, run_hidden_tests
from continuous_suite import Episode, load_continuous_suite

HERE = Path(__file__).resolve().parent
DEFAULT_CONTINUOUS_SUITE = HERE / "continuous-suite.toml"


def ensure_checkout(repo: Repo, work: Path) -> Path:
    """Pinned checkout without indexing; shares the directory bench.prepare_repo uses."""
    repo_dir = work / "repos" / repo.name
    repo_dir.parent.mkdir(parents=True, exist_ok=True)
    if not (repo_dir / ".git").is_dir():
        require_ok(run(["git", "clone", "--filter=blob:none", "--no-checkout", repo.url, str(repo_dir)]), f"clone {repo.name}")
    require_ok(run(["git", "fetch", "origin", repo.commit, "--depth", "1"], repo_dir), f"fetch {repo.name}")
    require_ok(run(["git", "checkout", "-q", "-B", "gcx-bench", repo.commit], repo_dir), f"checkout {repo.name}")
    return repo_dir


def episode_digest(episode: Episode) -> str:
    """Hash of everything that defines the episode: patches, hidden tests, prompts."""
    digest = hashlib.sha256()
    files = [episode.start_patch, *(episode.reference_patch(s) for s in episode.edit_steps)]
    files += sorted(p for p in episode.hidden_dir.rglob("*") if p.is_file())
    for path in files:
        digest.update(str(path.relative_to(episode.dir)).encode() + b"\0" + path.read_bytes() + b"\0")
    for step in episode.steps:
        digest.update(f"{step.id}\0{step.kind}\0{step.prompt}\0{step.tests}\0{step.required}".encode())
    return digest.hexdigest()


def stray_files(repo: Path) -> list[str]:
    """Hidden-test runs must leave no untracked files in the checkout."""
    return git(repo, "ls-files", "--others", "--exclude-standard").splitlines()


def verify_episode(episode: Episode, source: Path, commit: str, scratch: Path) -> dict[str, Any]:
    dest = scratch / episode.id
    prepare_episode_repo(source, commit, episode, dest)
    edit_steps = episode.edit_steps
    cells: list[dict[str, Any]] = []
    mismatches: list[str] = []
    strays: list[str] = []
    for applied in range(len(edit_steps) + 1):
        if applied:
            apply_patch(dest, episode.reference_patch(edit_steps[applied - 1]))
        state = "start" if applied == 0 else f"after-{edit_steps[applied - 1].id}"
        for index, step in enumerate(edit_steps):
            expected = index < applied
            outcome = run_hidden_tests(dest, episode, step)
            strays += stray_files(dest)
            cell: dict[str, Any] = {
                "state": state, "step": step.id, "expected_pass": expected, "passed": outcome.passed,
            }
            if outcome.passed != expected:
                cell["tail"] = outcome.tail
                mismatches.append(f"{state}/{step.id}: expected pass={expected}")
            cells.append(cell)
    return {
        "episode": episode.id,
        "repo": episode.repo,
        "language": episode.language,
        "repo_commit": commit,
        "episode_sha256": episode_digest(episode),
        "matrix": cells,
        "mismatches": mismatches,
        "stray_files": sorted(set(strays)),
        "ok": not mismatches and not strays,
    }


def toolchain() -> dict[str, str]:
    def version(command: list[str]) -> str:
        done = subprocess.run(command, capture_output=True, text=True, check=False)
        lines = (done.stdout or done.stderr).strip().splitlines()
        return lines[0] if lines else "unavailable"

    return {
        "python": sys.version.split()[0],
        "pytest": version([sys.executable, "-m", "pytest", "--version"]),
        "go": version(["go", "version"]),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, default=DEFAULT_CONTINUOUS_SUITE)
    parser.add_argument("--work", type=Path, default=DEFAULT_WORK)
    parser.add_argument("--episode", action="append")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        suite_path = args.suite.resolve()
        _, core_path, repos, episodes = load_continuous_suite(suite_path)
        episodes = [e for e in episodes if not args.episode or e.id in args.episode]
        if not episodes:
            raise BenchError("no episodes selected")
        results = []
        with tempfile.TemporaryDirectory(prefix="gcx-verify-") as scratch:
            for episode in episodes:
                repo = repos[episode.repo]
                source = ensure_checkout(repo, args.work.resolve())
                result = verify_episode(episode, source, repo.commit, Path(scratch))
                results.append(result)
                verdict = "OK" if result["ok"] else f"FAILED {result['mismatches'] or result['stray_files']}"
                print(f"{episode.id}: {verdict}", file=sys.stderr)
        record = {
            "type": "continuous-verification",
            "created_at": datetime.now(UTC).isoformat(),
            "suite_sha256": sha256(suite_path),
            "core_suite_sha256": sha256(core_path),
            "toolchain": toolchain(),
            "episodes": results,
            "ok": all(r["ok"] for r in results),
        }
        stamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
        output = args.output or HERE / "results" / f"continuous-verify-{stamp}.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(json.dumps({"output": str(output), "ok": record["ok"]}))
        return 0 if record["ok"] else 1
    except (BenchError, RuntimeError, OSError) as error:
        print(f"continuous-verify: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
