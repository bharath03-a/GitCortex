"""Continuous-coding episodes: suite, scoring, session handling, spend cap, verification gate."""

from __future__ import annotations

import difflib
import json
import subprocess
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock

import continuous_run
from bench import BenchError, Repo
from budget import RunBudget
from continuous_env import TestOutcome, apply_patch, diff_stats, prepare_episode_repo, run_hidden_tests
from continuous_run import (
    EpisodeConfig,
    PlannedEpisode,
    check_verification,
    execute_episodes,
    plan_episode_runs,
    run_episode,
)
from continuous_suite import Episode, Step, load_continuous_suite
from continuous_verify import episode_digest, scoring_digest, verify_episode
from test_unforced_parse import GCX, assistant, init, result, stream, tool_result, tool_use, usage
from unforced_client import ARMS, RawRun

HERE = Path(__file__).resolve().parent
GOOD = "def add(a, b):\n    return a + b\n"
BAD = "def add(a, b):\n    return a - b\n"
HIDDEN_TEST = "from calc import add\n\n\ndef test_add():\n    assert add(2, 3) == 5\n"


def patch(old: str, new: str) -> str:
    return "".join(difflib.unified_diff(old.splitlines(True), new.splitlines(True), "a/calc.py", "b/calc.py"))


def git(cwd: Path, *args: str) -> str:
    done = subprocess.run(
        ["git", "-c", "user.email=t@example.invalid", "-c", "user.name=t", *args],
        cwd=cwd, capture_output=True, text=True, check=True,
    )
    return done.stdout.strip()


def make_mini(root: Path, hidden: str = HIDDEN_TEST) -> tuple[Path, str, Episode]:
    source = root / "source"
    source.mkdir()
    git(source, "init", "-q")
    (source / "calc.py").write_text(GOOD)
    git(source, "add", "-A")
    git(source, "commit", "-qm", "base")
    commit = git(source, "rev-parse", "HEAD")
    folder = root / "episode"
    (folder / "hidden").mkdir(parents=True)
    (folder / "start.patch").write_text(patch(GOOD, BAD))
    (folder / "reference-step1.patch").write_text(patch(BAD, GOOD))
    (folder / "hidden" / "test_step1.py").write_text(hidden)
    steps = (
        Step("step1", "edit", "Fix the arithmetic bug.", "test_step1.py", ()),
        Step("probe", "probe", "Which files reference add?", None, ("calc.py",)),
    )
    episode = Episode("mini", "mini", "python", "debug", folder, "pytest", ("calc.py",), steps)
    return source, commit, episode


def scripted_runner(
    commands: list[list[str]], fix: bool = True, cost: float = 0.04,
    probe_answer: str = "calc.py defines it.", peek: bool = False,
):
    def run(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
        commands.append(command)
        first = "--session-id" in command
        events = [init(), assistant("m1", usage(10, 0, 1), tool_use("a", "Read", file_path="calc.py")), tool_result("a", GOOD)]
        if first and fix:
            (cwd / "calc.py").write_text(GOOD)
        if first and peek:
            events += [
                assistant("m2", usage(1, 0, 1), tool_use("p", "Bash", command="cat episodes/mini/hidden/test_step1.py")),
                tool_result("p", "x"),
            ]
        events.append(result("Fixed in calc.py." if first else probe_answer, cost))
        return RawRun(stream(*events), "", 0, 500, False)

    return run


class SuiteTests(unittest.TestCase):
    def test_real_suite_has_two_languages_and_verifiable_episodes(self) -> None:
        _, _, _, episodes = load_continuous_suite(HERE / "continuous-suite.toml")
        self.assertGreaterEqual(len(episodes), 2)
        self.assertGreaterEqual(len({e.language for e in episodes}), 2)
        for episode in episodes:
            self.assertTrue(episode.edit_steps)
            self.assertTrue(any(s.kind == "probe" for s in episode.steps))
            self.assertTrue(episode.start_patch.exists())

    def test_plan_alternates_arms_across_rounds(self) -> None:
        _, _, _, episodes = load_continuous_suite(HERE / "continuous-suite.toml")
        plan = plan_episode_runs(episodes[:1], ("baseline", "gcx-bare"), 2, 5)
        self.assertEqual(len(plan), 4)
        first = [p.arm.name for p in plan if p.round == 1]
        second = [p.arm.name for p in plan if p.round == 2]
        self.assertEqual(first, list(reversed(second)))


class EnvAndVerificationTests(unittest.TestCase):
    def test_hidden_tests_fail_at_start_and_pass_after_reference(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            dest = Path(tmp) / "work"
            prepare_episode_repo(source, commit, episode, dest)
            step = episode.steps[0]
            self.assertFalse(run_hidden_tests(dest, episode, step).passed)
            apply_patch(dest, episode.reference_patch(step))
            self.assertTrue(run_hidden_tests(dest, episode, step).passed)

    def test_hidden_test_output_and_timeout_are_retained_for_both_runners(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, _, episode = make_mini(Path(tmp))
            stdout = "x" * 2000
            for runner in ("pytest", "gotest"):
                selected = replace(episode, runner=runner)
                with mock.patch("continuous_env.subprocess.run", return_value=subprocess.CompletedProcess(
                    [], 2, stdout, "import error",
                )):
                    outcome = run_hidden_tests(source, selected, episode.steps[0])
                self.assertFalse(outcome.passed)
                self.assertEqual(outcome.returncode, 2)
                self.assertEqual(outcome.stdout, stdout)
                self.assertEqual(outcome.stderr, "import error")
                self.assertFalse(outcome.timed_out)
                with mock.patch("continuous_env.subprocess.run", side_effect=subprocess.TimeoutExpired(
                    [], 600, output=b"partial output", stderr=b"partial error",
                )):
                    outcome = run_hidden_tests(source, selected, episode.steps[0])
                self.assertTrue(outcome.timed_out)
                self.assertFalse(outcome.passed)
                self.assertEqual(outcome.returncode, -1)
                self.assertEqual(outcome.stdout, "partial output")
                self.assertEqual(outcome.stderr, "partial error")

    def test_verify_episode_passes_for_a_sound_episode(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            verdict = verify_episode(episode, source, commit, Path(tmp) / "scratch")
            self.assertTrue(verdict["ok"], verdict)
            self.assertEqual([c["passed"] for c in verdict["matrix"]], [False, True])

    def test_verify_rejects_hidden_tests_that_pass_on_the_broken_start(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp), hidden="def test_vacuous():\n    assert True\n")
            verdict = verify_episode(episode, source, commit, Path(tmp) / "scratch")
            self.assertFalse(verdict["ok"])
            self.assertIn("start/step1: expected pass=False", verdict["mismatches"])

    def test_diff_stats_excludes_tool_noise_and_flags_test_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            dest = Path(tmp) / "work"
            start = prepare_episode_repo(source, commit, episode, dest)
            (dest / "calc.py").write_text(GOOD)
            (dest / "test_calc.py").write_text("def test_x():\n    pass\n")
            (dest / ".gitcortex").mkdir()
            (dest / ".gitcortex" / "config.toml").write_text("x")
            stats = diff_stats(dest, start)
            self.assertEqual(set(stats.files), {"calc.py", "test_calc.py"})
            self.assertEqual(stats.test_files, ("test_calc.py",))
            self.assertIn("+    return a + b", stats.patch)

    def test_verification_gate_refuses_unproven_or_changed_episodes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            _, _, episode = make_mini(Path(tmp))
            repo = Repo("mini", "https://example.invalid/mini", "abc")
            repos = {"mini": repo}
            entry = {"episode": "mini", "ok": True, "episode_sha256": episode_digest(episode),
                     "repo": repo.name, "repo_url": repo.url, "repo_commit": repo.commit,
                     "scoring_sha256": scoring_digest()}
            good = {"ok": True, "episodes": [entry]}
            path = Path(tmp) / "verify.json"
            path.write_text(json.dumps(good))
            self.assertEqual(len(check_verification(path, [episode], repos)), 64)
            for field in ("repo", "repo_url", "repo_commit", "scoring_sha256"):
                for value in ("stale", None):
                    path.write_text(json.dumps({"ok": True, "episodes": [{**entry, field: value}]}))
                    with self.assertRaises(BenchError):
                        check_verification(path, [episode], repos)
            path.write_text(json.dumps(good))
            with self.assertRaises(BenchError):
                check_verification(path, [replace(episode, runner="gotest")], repos)
            for bad in (
                {**good, "ok": False},
                {"ok": True, "episodes": []},
                {"ok": True, "episodes": [{**entry, "episode_sha256": "0" * 64}]},
            ):
                path.write_text(json.dumps(bad))
                with self.assertRaises(BenchError):
                    check_verification(path, [episode], repos)


class SessionTests(unittest.TestCase):
    CONFIG = EpisodeConfig("sonnet", "low", Path("/opt/gcx"), 60, 1, "unit")

    def _run(self, runner, budget=None, arm="baseline"):
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            planned = PlannedEpisode(1, episode, ARMS[arm], 0)
            return run_episode(
                planned, source, commit, self.CONFIG, budget or RunBudget(5.0, 1.0), runner, {},
                Path(tmp) / "scratch", Path(tmp) / "logs",
            )[0]

    def test_one_persistent_session_across_steps(self) -> None:
        commands: list[list[str]] = []
        record = self._run(scripted_runner(commands))
        self.assertEqual(len(commands), 2)
        session = commands[0][commands[0].index("--session-id") + 1]
        self.assertEqual(commands[1][commands[1].index("--resume") + 1], session)
        self.assertNotIn("--no-session-persistence", commands[0])
        self.assertEqual(record["session_id"], session)

    def test_success_is_decided_by_hidden_tests_not_the_agents_claim(self) -> None:
        fixed = self._run(scripted_runner([], fix=True))
        unfixed = self._run(scripted_runner([], fix=False))
        self.assertTrue(fixed["success"])
        self.assertFalse(unfixed["success"])
        self.assertEqual(unfixed["final_tests"], {"step1": False})
        self.assertEqual(fixed["patch"]["unrelated_files"], [])

    def test_nonzero_client_exit_aborts_even_after_the_patch_passes_tests(self) -> None:
        runner = scripted_runner([], fix=True)

        def failed_client(command, cwd, timeout_s):
            return replace(runner(command, cwd, timeout_s), returncode=1)

        record = self._run(failed_client)
        self.assertEqual(record["final_tests"], {"step1": True})
        self.assertFalse(record["success"])
        self.assertIn("client_error", record["aborted"])
        self.assertEqual(record["steps"][0]["returncode"], 1)

    def test_step_and_final_scoring_preserve_full_outcomes(self) -> None:
        for outcome in (
            TestOutcome("step1", False, 2, "tail", "full stdout", "import error"),
            TestOutcome("step1", False, -1, "timeout", "partial", "stderr", True),
        ):
            with mock.patch.object(continuous_run, "run_hidden_tests", return_value=outcome):
                record = self._run(scripted_runner([]))
            self.assertFalse(record["success"])
            self.assertFalse(record["steps"][0]["tests_passed"])
            self.assertEqual(record["final_tests"], {"step1": False})
            self.assertEqual(record["steps"][0]["test_outcome"], outcome.to_dict())
            self.assertEqual(record["final_test_outcomes"], {"step1": outcome.to_dict()})
            self.assertEqual(json.loads(json.dumps(record))["final_test_outcomes"]["step1"], outcome.to_dict())

    def test_false_no_dependents_claim_is_recorded(self) -> None:
        wrong = self._run(scripted_runner([], probe_answer="Nothing else calls it; there are no callers."))
        right = self._run(scripted_runner([], probe_answer="calc.py defines add."))
        self.assertTrue(wrong["false_no_dependents"])
        self.assertFalse(right["false_no_dependents"])
        self.assertTrue(right["probe_correct"])

    def test_session_totals_and_repeat_exploration_span_steps(self) -> None:
        metrics = self._run(scripted_runner([], cost=0.04))["metrics"]
        self.assertAlmostEqual(metrics["cost_usd"], 0.08)
        self.assertEqual(metrics["tool_calls"], 2)
        self.assertEqual(metrics["repeat_reads"], 1)
        self.assertFalse(metrics["adopted"])

    def test_peeking_at_hidden_files_is_flagged(self) -> None:
        self.assertTrue(self._run(scripted_runner([], peek=True))["hidden_access_suspected"])
        self.assertFalse(self._run(scripted_runner([]))["hidden_access_suspected"])

    def test_budget_exhaustion_mid_episode_is_recorded_not_dropped(self) -> None:
        record = self._run(scripted_runner([], cost=0.06), RunBudget(0.10, 0.08))
        self.assertIn("before step probe", record["aborted"])
        self.assertEqual(len(record["steps"]), 1)
        self.assertIsNone(record["probe_correct"])

    def test_gcx_positions_are_global_across_steps(self) -> None:
        def runner(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
            first = "--session-id" in command
            events = [init(), assistant("m1", usage(1, 0, 1), tool_use("a", "Read", file_path="calc.py")), tool_result("a", "x")]
            if not first:
                events += [
                    assistant("m2", usage(1, 0, 1), tool_use("g", GCX, action="find_callers")),
                    tool_result("g", json.dumps({"index": {"state": "ready"}})),
                ]
            events.append(result("calc.py", 0.01))
            return RawRun(stream(*events), "", 0, 1, False)

        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            plan = [PlannedEpisode(1, episode, ARMS["gcx-bare"], 0)]
            with mock.patch.object(continuous_run, "index_repo"), mock.patch.object(continuous_run, "release_index"):
                records = list(execute_episodes(
                    plan, {"mini": (source, commit)}, self.CONFIG, RunBudget(5, 1), runner, {},
                    Path(tmp) / "scratch", Path(tmp) / "logs",
                ))
        metrics = records[0][0]["metrics"]
        # step 1: Read (call 1); step 2: Read (call 2), gcx (call 3).
        self.assertEqual((metrics["gcx_calls"], metrics["first_gcx_position"]), (1, 3))


class CodexContinuousTests(unittest.TestCase):
    CONFIG = EpisodeConfig("gpt-5.4-mini", "medium", Path("/opt/gcx"), 60, 1, "codex-unit", client="codex")

    def test_codex_step_commands_use_exec_then_resume_without_ephemeral(self) -> None:
        from continuous_client import build_step_command

        cwd = Path("/tmp/ep")
        step1 = build_step_command(
            ARMS["gcx-bare"], "Fix calc.py", "gpt-5.4-mini", "medium", Path("/opt/gcx"), 2.0,
            "placeholder", first_step=True, client="codex", cwd=cwd,
        )
        step2 = build_step_command(
            ARMS["gcx-bare"], "Which files reference add?", "gpt-5.4-mini", "medium", Path("/opt/gcx"), 2.0,
            "0199a213-81c0-7800-8aa1-bbab2a035a53", first_step=False, client="codex", cwd=cwd,
        )
        base1 = build_step_command(
            ARMS["baseline"], "Fix calc.py", "gpt-5.4-mini", "medium", Path("/opt/gcx"), 2.0,
            "placeholder", first_step=True, client="codex", cwd=cwd,
        )

        self.assertEqual(step1[:2], ["codex", "exec"])
        self.assertNotIn("resume", step1)
        self.assertNotIn("--ephemeral", step1)
        self.assertIn("--ignore-user-config", step1)
        self.assertIn("--ignore-rules", step1)
        self.assertEqual(step1[step1.index("-C") + 1], str(cwd))
        self.assertEqual(step1[-1], "Fix calc.py")

        self.assertEqual(step2[:3], ["codex", "exec", "resume"])
        self.assertNotIn("--ephemeral", step2)
        self.assertNotIn("-C", step2)
        self.assertIn("--ignore-user-config", step2)
        self.assertIn("--ignore-rules", step2)
        self.assertEqual(step2[-2:], ["0199a213-81c0-7800-8aa1-bbab2a035a53", "Which files reference add?"])

        self.assertFalse(any("mcp_servers.gcx" in arg for arg in base1))
        self.assertTrue(any("mcp_servers.gcx.command=" in arg for arg in step1))
        self.assertTrue(any("mcp_servers.gcx.command=" in arg for arg in step2))

        with self.assertRaises(ValueError):
            build_step_command(
                ARMS["baseline"], "probe", "gpt-5.4-mini", "medium", Path("/opt/gcx"), 2.0,
                "", first_step=False, client="codex",
            )

    def test_codex_episode_resumes_real_thread_id_across_steps(self) -> None:
        commands: list[list[str]] = []
        thread_id = "0199a213-81c0-7800-8aa1-bbab2a035a53"

        def runner(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
            commands.append(command)
            is_resume = len(command) >= 3 and command[2] == "resume"
            if not is_resume:
                (cwd / "calc.py").write_text(GOOD)
                events = [
                    {"type": "thread.started", "thread_id": thread_id},
                    {"type": "turn.started"},
                    {
                        "type": "item.completed",
                        "item": {"type": "file_change", "changes": [{"path": "calc.py", "kind": "update"}], "status": "completed"},
                    },
                    {"type": "item.completed", "item": {"type": "agent_message", "text": "Fixed calc.py."}},
                    {"type": "turn.completed", "usage": {"input_tokens": 400, "cached_input_tokens": 0, "output_tokens": 50}},
                ]
            else:
                events = [
                    {"type": "thread.started", "thread_id": thread_id},
                    {"type": "turn.started"},
                    {
                        "type": "item.completed",
                        "item": {
                            "type": "mcp_tool_call",
                            "server": "gcx",
                            "tool": "gcx",
                            "arguments": {"action": "find_callers"},
                            "result": {"content": [{"type": "text", "text": json.dumps({"index": {"state": "ready"}})}]},
                            "status": "completed",
                        },
                    },
                    {"type": "item.completed", "item": {"type": "agent_message", "text": "calc.py defines add."}},
                    {"type": "turn.completed", "usage": {"input_tokens": 500, "cached_input_tokens": 300, "output_tokens": 30}},
                ]
            return RawRun(stream(*events), "", 0, 500, False)

        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            planned = PlannedEpisode(1, episode, ARMS["gcx-bare"], 0)
            with mock.patch.object(continuous_run, "index_repo"), mock.patch.object(continuous_run, "release_index"):
                record, _ = run_episode(
                    planned, source, commit, self.CONFIG, RunBudget(10000.0, 2.0), runner, {},
                    Path(tmp) / "scratch", Path(tmp) / "logs",
                )

        self.assertEqual(len(commands), 2)
        self.assertEqual(commands[1][:3], ["codex", "exec", "resume"])
        self.assertEqual(commands[1][-2], thread_id)
        self.assertEqual(record["session_id"], thread_id)
        self.assertTrue(record["success"])
        self.assertTrue(record["probe_correct"])
        self.assertTrue(record["metrics"]["adopted"])
        self.assertEqual(record["metrics"]["first_gcx_position"], 2)
        self.assertEqual(record["metrics"]["total_tokens"], 980)
        self.assertEqual(record["metrics"]["uncached_tokens"], 680)

    def test_codex_episode_aborts_if_step1_omits_thread_id(self) -> None:
        commands: list[list[str]] = []

        def no_thread_runner(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
            commands.append(command)
            (cwd / "calc.py").write_text(GOOD)
            events = [
                {"type": "turn.started"},
                {"type": "item.completed", "item": {"type": "agent_message", "text": "Fixed calc.py."}},
                {"type": "turn.completed", "usage": {"input_tokens": 100, "cached_input_tokens": 0, "output_tokens": 10}},
            ]
            return RawRun(stream(*events), "", 0, 100, False)

        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            planned = PlannedEpisode(1, episode, ARMS["baseline"], 0)
            record, _ = run_episode(
                planned, source, commit, self.CONFIG, RunBudget(10000.0, 2.0), no_thread_runner, {},
                Path(tmp) / "scratch", Path(tmp) / "logs",
            )

        self.assertEqual(len(commands), 1)
        self.assertFalse(record["success"])
        self.assertIn("thread_id", record["aborted"])

    def test_diff_stats_excludes_codex_dir_and_parse_args_supports_codex(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source, commit, episode = make_mini(Path(tmp))
            dest = Path(tmp) / "work"
            start = prepare_episode_repo(source, commit, episode, dest)
            (dest / "calc.py").write_text(GOOD)
            (dest / ".codex").mkdir()
            (dest / ".codex" / "config.toml").write_text("x = 1\n")
            stats = diff_stats(dest, start)
            self.assertEqual(stats.files, ("calc.py",))

        args = continuous_run.parse_args(["--client", "codex", "--gcx", "/opt/gcx", "--label", "dry", "--dry-run"])
        self.assertEqual(args.client, "codex")
        self.assertEqual(args.model, "gpt-5.4-mini")
        self.assertEqual(args.max_total_usd, 10000.0)


if __name__ == "__main__":
    unittest.main()
