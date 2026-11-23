"""Executable documentation contract tests for GitCortex first-use and recovery docs."""

from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

FOUR_DOCS = (
    REPO_ROOT / "README.md",
    REPO_ROOT / "docs" / "QUICKSTART.md",
    REPO_ROOT / "docs" / "ADOPTION.md",
    REPO_ROOT / "docs" / "REFERENCE.md",
)

EXPECTED_INSTALLERS: dict[str, str] = {
    "claude": ".mcp.json",
    "codex": ".codex/config.toml",
    "cursor": ".cursor/mcp.json",
    "antigravity": ".agents/plugins/gitcortex/mcp_config.json",
    "windsurf": "~/.codeium/windsurf/mcp_config.json",
    "copilot": ".vscode/mcp.json",
}


def load_help(binary: Path, cwd: Path, env: dict[str, str], args: list[str]) -> str:
    """Invoke `binary` with `args` in an isolated environment and return stdout."""
    proc = subprocess.run(
        [str(binary), *args],
        cwd=str(cwd),
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise AssertionError(
            f"command failed ({proc.returncode}): {[str(binary), *args]}\n"
            f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
        )
    return proc.stdout


def _resolve_test_binary() -> Path | None:
    """Resolve `GCX_TEST_BINARY`, or fall back to a local Cargo build if present."""
    configured = os.environ.get("GCX_TEST_BINARY", "").strip()
    if configured:
        candidate = Path(configured).expanduser().resolve()
        if not candidate.is_file():
            raise AssertionError(
                f"GCX_TEST_BINARY points to missing binary: {candidate}"
            )
        return candidate

    for rel in ("target/debug/gcx", "target/release/gcx"):
        candidate = REPO_ROOT / rel
        if candidate.is_file():
            return candidate
    return None


def _isolated_env(home_dir: Path, store_dir: Path) -> dict[str, str]:
    env = dict(os.environ)
    env["HOME"] = str(home_dir)
    env["GCX_STORE_PATH"] = str(store_dir)
    env["NO_COLOR"] = "1"
    env.pop("GCX_BRANCH_OVERRIDE", None)
    return env


def _init_git_repo(repo_dir: Path, branch: str = "main") -> None:
    def _git(*args: str) -> None:
        proc = subprocess.run(
            ["git", *args],
            cwd=str(repo_dir),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise AssertionError(
                f"git {args} failed ({proc.returncode}): {proc.stderr}"
            )

    _git("init", "-b", branch)
    (repo_dir / ".gitignore").write_text(".gitcortex/\n", encoding="utf-8")
    (repo_dir / "lib.rs").write_text(
        "pub fn leaf() -> i32 { 1 }\npub fn caller() -> i32 { leaf() }\n",
        encoding="utf-8",
    )
    _git("add", ".")
    _git(
        "-c",
        "user.name=DocTest",
        "-c",
        "user.email=doctest@example.com",
        "commit",
        "-m",
        "initial",
    )


def _parse_possible_values(help_text: str, option_header: str) -> set[str]:
    """Extract clap `Possible values:` entries for an option section."""
    start = help_text.find(option_header)
    if start == -1:
        raise AssertionError(f"missing option {option_header!r} in help:\n{help_text}")
    section = help_text[start:]
    next_opt = re.search(r"\n\s{2,6}--[a-z]", section[len(option_header) :])
    if next_opt:
        section = section[: len(option_header) + next_opt.start()]

    inline = re.search(r"\[possible values:\s*([^\]]+)\]", section)
    if inline:
        return {part.strip() for part in inline.group(1).split(",") if part.strip()}

    values: set[str] = set()
    in_block = False
    for line in section.splitlines():
        if "Possible values:" in line:
            in_block = True
            continue
        if in_block:
            match = re.match(r"^\s*-\s+([a-z0-9-]+):", line)
            if match:
                values.add(match.group(1))
            elif line.strip() and not line.startswith(" "):
                break
    return values


def _extract_markdown_section(text: str, heading_pattern: str) -> str:
    match = re.search(heading_pattern, text, flags=re.MULTILINE)
    if not match:
        raise AssertionError(f"missing section matching {heading_pattern!r}")
    rest = text[match.end() :]
    next_heading = re.search(r"^#{1,2}\s+", rest, flags=re.MULTILINE)
    return rest[: next_heading.start()] if next_heading else rest


def _extract_table_rows(section_text: str) -> list[list[str]]:
    rows: list[list[str]] = []
    for line in section_text.splitlines():
        stripped = line.strip()
        if not (stripped.startswith("|") and stripped.endswith("|")):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if all(set(cell) <= {"-", ":", " "} for cell in cells):
            continue
        rows.append(cells)
    return rows


def _extract_json_code_blocks(text: str) -> list[str]:
    return re.findall(r"```json\s*\n(.*?)\n```", text, flags=re.DOTALL)


class UsabilityDocsTests(unittest.TestCase):
    """Pin shipped CLI contracts across README, QUICKSTART, ADOPTION, and REFERENCE."""

    def _read(self, rel_path: str) -> str:
        return (REPO_ROOT / rel_path).read_text(encoding="utf-8")

    def test_documented_viz_formats_exist(self) -> None:
        all_docs = {doc.relative_to(REPO_ROOT).as_posix(): doc.read_text(encoding="utf-8") for doc in FOUR_DOCS}

        documented_viz_formats: set[str] = set()
        viz_format_re = re.compile(
            r"\bgcx viz\b[^`\n|;&>]*--format\s+([a-z0-9-]+)"
        )
        for path, text in all_docs.items():
            self.assertNotIn(
                "gcx viz --format mermaid",
                text,
                f"{path} still documents unsupported `gcx viz --format mermaid`",
            )
            for match in viz_format_re.finditer(text):
                documented_viz_formats.add(match.group(1))

        # Reference table should list the real viz formats and disclose CDN use on legacy html.
        reference = all_docs["docs/REFERENCE.md"]
        viz_section = _extract_markdown_section(reference, r"^### gcx viz\s*$")
        self.assertIn("vis-network", viz_section)
        self.assertIn("CDN", viz_section)
        self.assertIn("gcx export --format mermaid", viz_section)
        self.assertNotIn("`mermaid`", viz_section.split("**Embedded")[0])

        expected_shipped = {"web", "html", "dot", "svg", "graphml", "cypher"}
        self.assertLessEqual(
            documented_viz_formats,
            expected_shipped,
            f"unexpected documented viz formats: {documented_viz_formats - expected_shipped}",
        )

        binary = _resolve_test_binary()
        if binary is not None:
            with tempfile.TemporaryDirectory() as tmp:
                tmp_path = Path(tmp)
                repo = tmp_path / "repo"
                repo.mkdir()
                _init_git_repo(repo)
                env = _isolated_env(tmp_path / "home", tmp_path / "store")
                viz_help = load_help(binary, repo, env, ["viz", "--help"])
                cli_formats = _parse_possible_values(viz_help, "--format <FORMAT>")
                self.assertEqual(cli_formats, expected_shipped)
                self.assertNotIn("mermaid", cli_formats)
                self.assertIn("CDN", viz_help)
                self.assertLessEqual(documented_viz_formats, cli_formats)

    def test_base_and_branch_defaults_match_help(self) -> None:
        reference = self._read("docs/REFERENCE.md")
        quickstart = self._read("docs/QUICKSTART.md")
        readme = self._read("README.md")

        # None of the docs may claim `--branch` or `--base` defaults statically to `main`.
        self.assertNotIn("accept `--branch <NAME>` (default: `main`)", reference)
        self.assertNotIn("| `--base <BRANCH>` | `main` |", reference)
        self.assertNotIn("| `--branch <BRANCH>` | `main` |", reference)
        self.assertIn("current checkout branch", reference)
        self.assertIn("gcx blast-radius --base main --head HEAD --format text", quickstart)
        self.assertIn("gcx blast-radius --base main --head HEAD --format text", readme)
        self.assertIn("gcx blast-radius --base main --head HEAD --format text", reference)
        self.assertIn("`--depth` | `1`", reference)

        binary = _resolve_test_binary()
        if binary is not None:
            with tempfile.TemporaryDirectory() as tmp:
                tmp_path = Path(tmp)
                repo = tmp_path / "repo"
                repo.mkdir()
                _init_git_repo(repo, branch="main")
                env = _isolated_env(tmp_path / "home", tmp_path / "store")

                # Initialize on main, then cut a feature branch and verify dynamic defaults + F0 HEAD ref.
                load_help(binary, repo, env, ["init", "--editor", "none"])
                subprocess.run(
                    ["git", "-c", "core.hooksPath=/dev/null", "checkout", "-b", "feat/docs-contract"],
                    cwd=str(repo),
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                )
                (repo / "lib.rs").write_text(
                    "pub fn leaf() -> i32 { 2 }\npub fn caller() -> i32 { leaf() }\n",
                    encoding="utf-8",
                )
                subprocess.run(
                    ["git", "add", "lib.rs"],
                    cwd=str(repo),
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                )
                subprocess.run(
                    [
                        "git",
                        "-c",
                        "core.hooksPath=/dev/null",
                        "-c",
                        "user.name=DocTest",
                        "-c",
                        "user.email=doctest@example.com",
                        "commit",
                        "-m",
                        "change leaf",
                    ],
                    cwd=str(repo),
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                )
                load_help(binary, repo, env, ["hook"])

                blast_help = load_help(binary, repo, env, ["blast-radius", "--help"])
                viz_help = load_help(binary, repo, env, ["viz", "--help"])
                query_help = load_help(
                    binary, repo, env, ["query", "lookup-symbol", "--help"]
                )
                self.assertIn("[default: feat/docs-contract]", blast_help)
                self.assertIn("[default: HEAD]", blast_help)
                self.assertIn("[default: feat/docs-contract]", viz_help)
                self.assertIn("[default: feat/docs-contract]", query_help)

                # Documented HEAD blast-radius command must succeed on a clean feature branch.
                blast_out = load_help(
                    binary,
                    repo,
                    env,
                    ["blast-radius", "--base", "main", "--head", "HEAD", "--format", "text"],
                )
                self.assertIn("Blast Radius Report", blast_out)

    def test_init_cli_only_is_explicit(self) -> None:
        readme = self._read("README.md")
        quickstart = self._read("docs/QUICKSTART.md")
        reference = self._read("docs/REFERENCE.md")

        for name, text in (
            ("README.md", readme),
            ("docs/QUICKSTART.md", quickstart),
            ("docs/REFERENCE.md", reference),
        ):
            self.assertIn("gcx init --editor none", text, f"{name} missing explicit CLI-only init")
            self.assertNotIn("omit --editor for CLI-only", text, f"{name} claims bare init is CLI-only")
            self.assertNotRegex(
                text,
                r"(?m)^gcx init\s+#\s*no editor configuration",
                f"{name} falsely claims bare `gcx init` never opens a chooser",
            )
            self.assertIn("Agy CLI", text, f"{name} should state Agy CLI is omitted/unsupported")
            self.assertIn("not exercised", text, f"{name} must record real client discovery status")
            self.assertNotIn(
                "~/.gemini/config/mcp_config.json",
                text,
                f"{name} still references unshipped agy CLI config path",
            )
            for token, cfg_path in EXPECTED_INSTALLERS.items():
                self.assertIn(f"`{token}`", text, f"{name} missing installer token {token}")
                self.assertIn(cfg_path, text, f"{name} missing config path {cfg_path}")

        # Quickstart and Reference explain interactive TTY chooser vs explicit `--editor none`.
        self.assertIn("interactive terminal", quickstart)
        self.assertIn("interactive TTY", reference)

        binary = _resolve_test_binary()
        if binary is not None:
            with tempfile.TemporaryDirectory() as tmp:
                tmp_path = Path(tmp)
                repo = tmp_path / "repo"
                repo.mkdir()
                _init_git_repo(repo)
                env = _isolated_env(tmp_path / "home", tmp_path / "store")
                init_help = load_help(binary, repo, env, ["init", "--help"])
                self.assertIn("--editor <EDITOR>", init_help)
                self.assertIn("none", init_help)
                self.assertIn("interactive TTY", init_help)

    def test_ci_claims_match_shipped_workflow_pair(self) -> None:
        universal_rs = self._read("crates/gitcortex-cli/src/cmd/init/universal.rs")
        for expected in (
            "blast-radius:",
            "comment:",
            "contents: read",
            "pull-requests: write",
            "github.event.pull_request.head.repo.full_name == github.repository",
            "gitcortex-blast-radius",
        ):
            self.assertIn(expected, universal_rs)

        for doc in FOUR_DOCS:
            text = doc.read_text(encoding="utf-8")
            rel = doc.relative_to(REPO_ROOT).as_posix()
            for unshipped in ("bharath03-a/gitcortex-action", "workflow_run", "impact-json"):
                self.assertNotIn(
                    unshipped,
                    text,
                    f"{rel} claims unshipped CI feature {unshipped!r}",
                )

        for rel in ("README.md", "docs/QUICKSTART.md", "docs/ADOPTION.md"):
            text = self._read(rel)
            self.assertIn("blast-radius", text)
            self.assertIn("comment", text)
            self.assertIn("gitcortex-blast-radius", text)
            self.assertIn("contents: read", text)

        binary = _resolve_test_binary()
        if binary is not None:
            with tempfile.TemporaryDirectory() as tmp:
                tmp_path = Path(tmp)
                repo = tmp_path / "repo"
                repo.mkdir()
                _init_git_repo(repo)
                env = _isolated_env(tmp_path / "home", tmp_path / "store")
                load_help(binary, repo, env, ["init", "--ci", "--editor", "none"])
                workflow = (
                    repo / ".github" / "workflows" / "gcx-blast-radius.yml"
                ).read_text(encoding="utf-8")
                self.assertIn("blast-radius:", workflow)
                self.assertIn("comment:", workflow)
                self.assertIn("contents: read", workflow)
                self.assertIn("pull-requests: write", workflow)
                self.assertIn(
                    "github.event.pull_request.head.repo.full_name == github.repository",
                    workflow,
                )
                self.assertIn("gitcortex-blast-radius", workflow)

    def test_recovery_table_has_commands_and_expected_states(self) -> None:
        required_row_checks = (
            ("outside a git repository", "gcx doctor", "exits `0`"),
            ("unindexed branch", "gcx index --branch", "ready"),
            ("missing or deleted graph store", "gcx init --editor none", "ready"),
            ("stale committed index", "gcx hook", "ready"),
            ("unverified", "git status", "ready"),
            ("incompatible schema", "gcx storage list", "gcx clean && gcx init --editor none"),
        )

        for rel, heading in (
            ("docs/QUICKSTART.md", r"^## 6\. Recovery\s*$"),
            ("docs/REFERENCE.md", r"^## Recovery\s*$"),
        ):
            section = _extract_markdown_section(self._read(rel), heading)
            # Explicit caveats required by F9a.
            self.assertIn("gcx deinit", section)
            self.assertIn("not** repair or restore", section)
            self.assertIn("not** verify uncommitted", section)

            rows = _extract_table_rows(section)
            # Header + 6 recovery rows.
            self.assertEqual(
                len(rows),
                7,
                f"{rel} recovery table should have header + 6 rows, got {len(rows)}",
            )
            body_rows = rows[1:]
            for idx, (symptom_needle, cmd_needle, state_needle) in enumerate(required_row_checks):
                row_text = " | ".join(body_rows[idx]).lower()
                self.assertIn(
                    symptom_needle.lower(),
                    row_text,
                    f"{rel} row {idx + 1} missing symptom {symptom_needle!r}",
                )
                self.assertIn(
                    cmd_needle.lower(),
                    row_text,
                    f"{rel} row {idx + 1} missing command {cmd_needle!r}",
                )
                self.assertIn(
                    state_needle.lower(),
                    row_text,
                    f"{rel} row {idx + 1} missing expected state/recovery {state_needle!r}",
                )

    def test_compact_example_uses_nested_params_and_index(self) -> None:
        for rel in ("docs/QUICKSTART.md", "docs/REFERENCE.md"):
            text = self._read(rel)
            self.assertIn("index.state", text, f"{rel} must document index.state inspection")
            self.assertIn("index.working_tree_verified", text)
            self.assertIn("index.next_action", text)

            found_compact_example = False
            for block in _extract_json_code_blocks(text):
                try:
                    parsed = json.loads(block)
                except json.JSONDecodeError:
                    continue
                if not isinstance(parsed, dict):
                    continue
                payload = parsed.get("arguments", parsed)
                if (
                    isinstance(payload, dict)
                    and isinstance(payload.get("action"), str)
                    and isinstance(payload.get("params"), dict)
                    and len(payload["params"]) > 0
                ):
                    found_compact_example = True
                    break
            self.assertTrue(
                found_compact_example,
                f"{rel} must contain a valid JSON compact MCP example with nested `params`",
            )


if __name__ == "__main__":
    unittest.main()
