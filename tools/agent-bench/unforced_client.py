"""Claude Code client adapter for the unforced lane.

Every arm gets the same read-only tool surface and the same isolation from the
operator's own Claude config; only the gcx MCP server and the generated guide
differ between arms.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import time
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

GUIDE_FILES = (".claude/CLAUDE.md", ".gitcortex/AGENT_GUIDE.md")
READ_ONLY_BASH = (
    "grep", "rg", "find", "cat", "ls", "head", "tail", "sed", "wc", "sort", "uniq", "awk",
    "git log", "git grep", "git show", "git ls-files", "git diff", "git blame",
)
ALWAYS_DISALLOWED = ("Edit", "Write", "NotebookEdit", "WebSearch", "WebFetch", "Task", "Agent")
SCRUBBED_ENV = ("CLAUDECODE", "CLAUDE_CODE_SSE_PORT")


@dataclass(frozen=True)
class Arm:
    name: str
    gcx: bool
    guide: bool


ARMS = {
    "baseline": Arm("baseline", gcx=False, guide=False),
    "gcx-bare": Arm("gcx-bare", gcx=True, guide=False),
    "gcx-guide": Arm("gcx-guide", gcx=True, guide=True),
}


@dataclass(frozen=True)
class RawRun:
    stdout: str
    stderr: str
    returncode: int | None
    wall_ms: int
    timed_out: bool


def allowed_tools(arm: Arm) -> str:
    tools = ["Read", "Grep", "Glob", "ToolSearch"]
    tools += [f"Bash({command}:*)" for command in READ_ONLY_BASH]
    if arm.gcx:
        tools.append("mcp__gcx")
    return " ".join(tools)


def disallowed_tools(arm: Arm) -> str:
    tools = list(ALWAYS_DISALLOWED)
    if not arm.gcx:
        tools.append("mcp__gcx")
    return " ".join(tools)


def mcp_config(arm: Arm, gcx: Path) -> str:
    servers = {"gcx": {"command": str(gcx), "args": ["serve"]}} if arm.gcx else {}
    return json.dumps({"mcpServers": servers}, separators=(",", ":"))


def build_command(
    arm: Arm, prompt: str, model: str, effort: str, gcx: Path, max_budget_usd: float
) -> list[str]:
    return [
        "claude",
        "-p",
        prompt,
        "--output-format", "stream-json",
        "--verbose",
        "--no-session-persistence",
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


def client_env() -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in SCRUBBED_ENV}
    env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] = "1"
    return env


def _text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    return value.decode("utf-8", "replace") if isinstance(value, bytes) else value


def _elapsed_ms(started: float) -> int:
    return int((time.perf_counter() - started) * 1000)


def run_claude(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
    started = time.perf_counter()
    try:
        done = subprocess.run(
            command,
            cwd=cwd,
            env=client_env(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_s,
            check=False,
        )
        return RawRun(done.stdout, done.stderr, done.returncode, _elapsed_ms(started), False)
    except subprocess.TimeoutExpired as exc:
        return RawRun(_text(exc.stdout), _text(exc.stderr), None, _elapsed_ms(started), True)


def generate_guide(gcx: Path) -> dict[str, str]:
    """The guide `gcx init --editor claude` writes, captured from a scratch repo."""
    scratch = Path(tempfile.mkdtemp(prefix="gcx-guide-"))
    try:
        for command in (
            ["git", "init", "-q"],
            ["git", "-c", "user.email=bench@example.invalid", "-c", "user.name=bench",
             "commit", "-q", "--allow-empty", "-m", "scratch"],
            [str(gcx), "init", "--editor", "claude"],
        ):
            done = subprocess.run(command, cwd=scratch, capture_output=True, text=True, check=False)
            if done.returncode != 0:
                raise RuntimeError(f"{' '.join(command[:3])} failed: {done.stderr or done.stdout}")
        files = {name: (scratch / name).read_text(encoding="utf-8") for name in GUIDE_FILES}
        subprocess.run([str(gcx), "clean"], cwd=scratch, capture_output=True, check=False)
        return files
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def guide_sha256(files: dict[str, str]) -> str:
    digest = hashlib.sha256()
    for name in sorted(files):
        digest.update(name.encode() + b"\0" + files[name].encode() + b"\0")
    return digest.hexdigest()


@contextlib.contextmanager
def installed_guide(repo_dir: Path, files: dict[str, str]) -> Iterator[None]:
    """Write guide files into the checkout for one run, then remove them."""
    targets = [repo_dir / name for name in files]
    clash = [t for t in targets if t.exists()]
    if clash:
        raise RuntimeError(f"refusing to overwrite existing guide files: {clash}")
    created_dirs: list[Path] = []
    try:
        for name, text in files.items():
            target = repo_dir / name
            for parent in reversed(target.relative_to(repo_dir).parents):
                directory = repo_dir / parent
                if parent != Path(".") and not directory.exists():
                    directory.mkdir()
                    created_dirs.append(directory)
            target.write_text(text, encoding="utf-8")
        yield
    finally:
        for target in targets:
            target.unlink(missing_ok=True)
        for directory in reversed(created_dirs):
            with contextlib.suppress(OSError):
                directory.rmdir()
