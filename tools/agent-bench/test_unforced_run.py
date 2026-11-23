"""Unforced lane wiring: neutral prompts, arm isolation, ordering, cap enforcement."""

from __future__ import annotations

import json
import tempfile
import unittest
from dataclasses import replace
from collections import Counter
from pathlib import Path

from budget import BudgetExceeded, RunBudget
from test_unforced_parse import GCX, assistant, init, result, stream, tool_result, tool_use, usage
from unforced_client import ARMS, RawRun, build_command, installed_guide, mcp_config
from unforced_run import RunConfig, arm_order, execute_plan, plan_runs
from unforced_suite import assert_neutral, load_unforced_suite

HERE = Path(__file__).resolve().parent
SUITE = HERE / "unforced-suite.toml"
GCX_BIN = Path("/opt/gcx")
_, _, REPOS, TASKS = load_unforced_suite(SUITE)
LEAK_WORDS = ("gcx", "gitcortex", "mcp", "knowledge graph")


class SuiteTests(unittest.TestCase):
    def test_unforced_lane_counts_adoption_without_prompt_mention(self) -> None:
        for task in TASKS:
            lowered = task.prompt.lower()
            self.assertFalse([w for w in LEAK_WORDS if w in lowered], task.id)
        for name in ARMS:
            command = build_command(ARMS[name], TASKS[0].prompt, "sonnet", "low", GCX_BIN, 0.5)
            prompt = command[command.index("-p") + 1]
            self.assertEqual(prompt, TASKS[0].prompt)

    def test_leaky_prompt_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            assert_neutral("Use the GitCortex graph to answer")
        with self.assertRaises(ValueError):
            assert_neutral("call the gcx tool")

    def test_both_task_kinds_present_with_pinned_repos(self) -> None:
        kinds = Counter(t.kind for t in TASKS)
        self.assertGreater(kinds["graph"], 0)
        self.assertGreater(kinds["restraint"], 0)
        self.assertTrue(all(t.repo in REPOS and t.required for t in TASKS))

    def test_graph_tasks_inherit_core_ground_truth(self) -> None:
        by_id = {t.id: t for t in TASKS}
        self.assertIn("command.go", by_id["cobra-impact-add-command"].required)
        self.assertTrue(by_id["cobra-impact-add-command"].dependents_exist)
        self.assertFalse(by_id["cobra-restraint-unknown-command"].dependents_exist)


class ArmIsolationTests(unittest.TestCase):
    def _flag(self, command: list[str], flag: str) -> str:
        return command[command.index(flag) + 1]

    def test_baseline_has_no_mcp_server_or_gcx_permission(self) -> None:
        command = build_command(ARMS["baseline"], "q", "sonnet", "low", GCX_BIN, 0.5)
        self.assertEqual(json.loads(self._flag(command, "--mcp-config")), {"mcpServers": {}})
        self.assertNotIn("mcp__gcx", self._flag(command, "--allowed-tools"))
        self.assertIn("mcp__gcx", self._flag(command, "--disallowed-tools"))

    def test_gcx_arms_get_the_server_and_nothing_else_differs(self) -> None:
        base = build_command(ARMS["baseline"], "q", "sonnet", "low", GCX_BIN, 0.5)
        bare = build_command(ARMS["gcx-bare"], "q", "sonnet", "low", GCX_BIN, 0.5)
        guided = build_command(ARMS["gcx-guide"], "q", "sonnet", "low", GCX_BIN, 0.5)
        self.assertEqual(bare, guided)
        servers = json.loads(self._flag(bare, "--mcp-config"))["mcpServers"]
        self.assertEqual(servers["gcx"], {"command": str(GCX_BIN), "args": ["serve"]})
        differing = [i for i, (a, b) in enumerate(zip(base, bare)) if a != b]
        flags = {base[i - 1] for i in differing}
        self.assertEqual(flags, {"--mcp-config", "--allowed-tools", "--disallowed-tools"})

    def test_operator_config_is_isolated_and_edits_disabled(self) -> None:
        command = build_command(ARMS["baseline"], "q", "sonnet", "low", GCX_BIN, 0.5)
        self.assertEqual(self._flag(command, "--setting-sources"), "project,local")
        self.assertIn("--disable-slash-commands", command)
        self.assertIn("--strict-mcp-config", command)
        disallowed = self._flag(command, "--disallowed-tools").split()
        for tool in ("Edit", "Write", "Task"):
            self.assertIn(tool, disallowed)

    def test_budget_flag_carries_the_cap(self) -> None:
        command = build_command(ARMS["baseline"], "q", "sonnet", "low", GCX_BIN, 0.1234)
        self.assertEqual(self._flag(command, "--max-budget-usd"), "0.1234")
        self.assertEqual(mcp_config(ARMS["gcx-bare"], GCX_BIN).count("gcx"), 2)


class GuideInstallTests(unittest.TestCase):
    FILES = {".claude/CLAUDE.md": "guide", ".gitcortex/AGENT_GUIDE.md": "full guide"}

    def test_guide_is_present_during_run_and_removed_after(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / ".gitcortex").mkdir()
            with installed_guide(repo, self.FILES):
                self.assertEqual((repo / ".claude/CLAUDE.md").read_text(), "guide")
                self.assertTrue((repo / ".gitcortex/AGENT_GUIDE.md").exists())
            self.assertFalse((repo / ".claude").exists())
            self.assertFalse((repo / ".gitcortex/AGENT_GUIDE.md").exists())
            self.assertTrue((repo / ".gitcortex").exists())

    def test_guide_removed_even_when_run_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            with self.assertRaises(RuntimeError), installed_guide(repo, self.FILES):
                raise RuntimeError("client crashed")
            self.assertFalse((repo / ".claude").exists())

    def test_existing_guide_is_never_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / ".claude").mkdir()
            (repo / ".claude/CLAUDE.md").write_text("repo's own")
            with self.assertRaises(RuntimeError), installed_guide(repo, self.FILES):
                pass
            self.assertEqual((repo / ".claude/CLAUDE.md").read_text(), "repo's own")


class OrderingTests(unittest.TestCase):
    ARMS = ("baseline", "gcx-bare", "gcx-guide")

    def test_order_is_deterministic_for_a_seed(self) -> None:
        self.assertEqual(arm_order(7, "t", 1, self.ARMS), arm_order(7, "t", 1, self.ARMS))
        self.assertEqual(sorted(arm_order(7, "t", 1, self.ARMS)), sorted(self.ARMS))

    def test_two_arm_order_alternates_each_round(self) -> None:
        arms = ("baseline", "gcx-bare")
        first = arm_order(1, "task", 1, arms)
        second = arm_order(1, "task", 2, arms)
        self.assertEqual(first, tuple(reversed(second)))
        self.assertEqual(arm_order(1, "task", 3, arms), first)

    def test_every_arm_visits_every_slot_over_a_full_cycle(self) -> None:
        slots = {a: set() for a in self.ARMS}
        for round_number in (1, 2, 3):
            for index, arm in enumerate(arm_order(3, "task", round_number, self.ARMS)):
                slots[arm].add(index)
        self.assertTrue(all(s == {0, 1, 2} for s in slots.values()))

    def test_seed_changes_order_for_some_task(self) -> None:
        orders = {arm_order(seed, "task", 1, self.ARMS) for seed in range(8)}
        self.assertGreater(len(orders), 1)

    def test_plan_covers_every_task_arm_round_once(self) -> None:
        plan = plan_runs(TASKS[:3], self.ARMS, 2, 1)
        self.assertEqual(len(plan), 3 * 3 * 2)
        keys = Counter((p.round, p.task.id, p.arm.name) for p in plan)
        self.assertTrue(all(count == 1 for count in keys.values()))


def fake_runner(cost: float | None, calls: list[list[str]], timed_out: bool = False):
    def run(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
        calls.append(command)
        if cost is None:
            return RawRun("", "", None, 1, timed_out)
        raw = stream(
            init(),
            assistant("m1", usage(10, 0, 1), tool_use("a", GCX, action="find_callers")),
            tool_result("a", json.dumps({"index": {"state": "ready"}})),
            result("command.go and completions.go", cost),
        )
        return RawRun(raw, "", 0, 1234, False)

    return run


class CapEnforcementTests(unittest.TestCase):
    def _execute(self, budget: RunBudget, runner, plan_size: int = 6):
        task = next(t for t in TASKS if t.id == "cobra-impact-add-command")
        plan = plan_runs([task], ("baseline", "gcx-bare"), plan_size, 1)
        config = RunConfig("sonnet", "low", GCX_BIN, 60, 1, "unit")
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            out = []
            try:
                for record, budget in execute_plan(
                    plan, {"cobra": repo}, config, budget, runner, {}, Path(tmp) / "logs"
                ):
                    out.append((record, budget))
            except BudgetExceeded as stop:
                return out, stop
            return out, None

    def test_runner_stops_before_exceeding_the_spend_cap(self) -> None:
        calls: list[list[str]] = []
        budget = RunBudget(max_total_usd=0.30, max_run_usd=0.20)
        out, stop = self._execute(budget, fake_runner(0.12, calls))
        self.assertIsNotNone(stop)
        self.assertEqual(len(calls), 3)
        caps = [float(c[c.index("--max-budget-usd") + 1]) for c in calls]
        self.assertEqual(caps, [0.2, 0.18, 0.06])
        self.assertAlmostEqual(out[-1][1].spent_usd, 0.36)

    def test_run_count_cap_stops_the_runner(self) -> None:
        calls: list[list[str]] = []
        budget = RunBudget(max_total_usd=50.0, max_run_usd=1.0, max_runs=2)
        out, stop = self._execute(budget, fake_runner(0.01, calls))
        self.assertIsNotNone(stop)
        self.assertEqual((len(out), len(calls)), (2, 2))

    def test_crashed_run_is_charged_its_full_cap(self) -> None:
        calls: list[list[str]] = []
        budget = RunBudget(max_total_usd=1.0, max_run_usd=0.4)
        out, stop = self._execute(budget, fake_runner(None, calls, timed_out=True))
        record, after = out[0]
        self.assertEqual(record["status"], "timeout")
        self.assertTrue(record["cost_estimated"])
        self.assertAlmostEqual(after.spent_usd, 0.4)
        self.assertIsNotNone(stop)
        self.assertEqual(len(calls), 3)

    def test_nonzero_client_exit_cannot_report_success_from_a_success_event(self) -> None:
        runner = fake_runner(0.05, [])

        def failed_client(command, cwd, timeout_s):
            return replace(runner(command, cwd, timeout_s), returncode=1)

        out, _ = self._execute(RunBudget(5.0, 1.0), failed_client, plan_size=1)
        self.assertFalse(out[0][0]["success"])
        self.assertEqual(out[0][0]["status"], "client_error")

    def test_record_carries_adoption_grade_and_provenance_fields(self) -> None:
        calls: list[list[str]] = []
        out, _ = self._execute(RunBudget(5.0, 1.0), fake_runner(0.05, calls), plan_size=1)
        record = out[0][0]
        self.assertTrue(record["success"])
        self.assertEqual(record["kind"], "graph")
        self.assertTrue(record["metrics"]["adopted"])
        self.assertEqual(record["metrics"]["first_gcx_position"], 1)
        self.assertEqual(record["wall_ms"], 1234)
        self.assertEqual(len(record["prompt_sha256"]), 64)


class CodexUnforcedTests(unittest.TestCase):
    def test_codex_command_isolation_and_mcp_parity(self) -> None:
        cwd = Path("/tmp/repo")
        base = build_command(ARMS["baseline"], TASKS[0].prompt, "gpt-5.4-mini", "low", GCX_BIN, 1.0, client="codex", cwd=cwd)
        bare = build_command(ARMS["gcx-bare"], TASKS[0].prompt, "gpt-5.4-mini", "low", GCX_BIN, 1.0, client="codex", cwd=cwd)
        guided = build_command(ARMS["gcx-guide"], TASKS[0].prompt, "gpt-5.4-mini", "low", GCX_BIN, 1.0, client="codex", cwd=cwd)
        self.assertEqual(bare, guided)
        self.assertEqual(base[:2], ["codex", "exec"])
        for flag in ("--json", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--dangerously-bypass-approvals-and-sandbox"):
            self.assertIn(flag, base)
            self.assertIn(flag, bare)
        self.assertEqual(base[base.index("-C") + 1], str(cwd))
        self.assertEqual(base[-1], TASKS[0].prompt)
        self.assertEqual(bare[-1], TASKS[0].prompt)
        self.assertFalse(any("mcp_servers.gcx" in arg for arg in base))
        self.assertIn(f"mcp_servers.gcx.command={json.dumps(str(GCX_BIN))}", bare)
        self.assertIn('mcp_servers.gcx.args=["serve"]', bare)
        self.assertIn("mcp_servers.gcx.startup_timeout_sec=30", bare)

    def test_codex_guide_generation_and_install_cleanup(self) -> None:
        from unittest import mock
        import subprocess
        from unforced_client import CODEX_GUIDE_FILES, generate_guide

        commands: list[list[str]] = []

        def fake_gcx_init(cmd, cwd=None, **kwargs):
            commands.append(list(cmd))
            if cmd[:2] == [str(GCX_BIN), "init"]:
                self.assertEqual(cmd, [str(GCX_BIN), "init", "--editor", "codex"])
                root = Path(cwd)
                (root / "AGENTS.md").write_text("# Codex gcx guide\n", encoding="utf-8")
                (root / ".gitcortex").mkdir(parents=True, exist_ok=True)
                (root / ".gitcortex" / "AGENT_GUIDE.md").write_text("Full guide\n", encoding="utf-8")
            return subprocess.CompletedProcess(cmd, 0, "", "")

        with mock.patch("unforced_client.subprocess.run", side_effect=fake_gcx_init):
            files = generate_guide(GCX_BIN, client="codex")
        self.assertIn([str(GCX_BIN), "init", "--editor", "codex"], commands)
        self.assertIn([str(GCX_BIN), "clean"], commands)
        self.assertEqual(tuple(files.keys()), CODEX_GUIDE_FILES)

        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            with installed_guide(repo, files):
                self.assertEqual((repo / "AGENTS.md").read_text(), "# Codex gcx guide\n")
                self.assertTrue((repo / ".gitcortex/AGENT_GUIDE.md").exists())
            self.assertFalse((repo / "AGENTS.md").exists())
            self.assertFalse((repo / ".gitcortex/AGENT_GUIDE.md").exists())

    def test_parse_args_codex_defaults_vs_claude_requirements(self) -> None:
        from unforced_run import DEFAULT_CODEX_BUDGET_USD, parse_args

        codex_args = parse_args(["--client", "codex", "--gcx", str(GCX_BIN), "--label", "codex-smoke"])
        self.assertEqual(codex_args.client, "codex")
        self.assertEqual(codex_args.model, "gpt-5.4-mini")
        self.assertEqual(codex_args.max_total_usd, DEFAULT_CODEX_BUDGET_USD)

        claude_args = parse_args(["--gcx", str(GCX_BIN), "--label", "claude-smoke", "--max-total-usd", "5"])
        self.assertEqual(claude_args.client, "claude")
        self.assertEqual(claude_args.model, "sonnet")
        self.assertEqual(claude_args.max_total_usd, 5.0)

        with self.assertRaises(SystemExit):
            parse_args(["--gcx", str(GCX_BIN), "--label", "missing-cap"])

    def test_execute_plan_codex_end_to_end_and_guide_scoping(self) -> None:
        task = next(t for t in TASKS if t.id == "cobra-impact-add-command")
        plan = plan_runs([task], ("baseline", "gcx-bare", "gcx-guide"), 1, 1)
        config = RunConfig("gpt-5.4-mini", "low", GCX_BIN, 60, 1, "codex-unit", client="codex")
        guide = {"AGENTS.md": "# guide\n", ".gitcortex/AGENT_GUIDE.md": "full\n"}
        saw_guide: dict[str, bool] = {}

        def codex_runner(command: list[str], cwd: Path, timeout_s: int) -> RawRun:
            has_mcp = any("mcp_servers.gcx" in arg for arg in command)
            has_agents = (cwd / "AGENTS.md").exists()
            arm_key = "gcx-guide" if has_agents else ("gcx-bare" if has_mcp else "baseline")
            saw_guide[arm_key] = has_agents
            events = [{"type": "thread.started", "thread_id": f"th-{arm_key}"}, {"type": "turn.started"}]
            if has_mcp:
                events.append({
                    "type": "item.completed",
                    "item": {
                        "type": "mcp_tool_call",
                        "server": "gcx",
                        "tool": "gcx",
                        "arguments": {"action": "find_callers"},
                        "result": {"content": [{"type": "text", "text": json.dumps({"index": {"state": "ready"}})}]},
                        "status": "completed",
                    },
                })
            events.append({
                "type": "item.completed",
                "item": {"type": "agent_message", "text": "command.go and completions.go"},
            })
            events.append({
                "type": "turn.completed",
                "usage": {"input_tokens": 600, "cached_input_tokens": 100, "output_tokens": 40},
            })
            return RawRun(stream(*events), "", 0, 900, False)

        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            rows = [
                rec
                for rec, _ in execute_plan(
                    plan, {"cobra": repo}, config, RunBudget(10000.0, 1.0), codex_runner, guide, Path(tmp) / "logs"
                )
            ]
            self.assertFalse((repo / "AGENTS.md").exists())

        by_arm = {r["arm"]: r for r in rows}
        self.assertEqual(saw_guide, {"baseline": False, "gcx-bare": False, "gcx-guide": True})
        self.assertTrue(all(r["success"] for r in rows))
        self.assertTrue(all(r["cost_estimated"] for r in rows))
        self.assertFalse(by_arm["baseline"]["metrics"]["adopted"])
        self.assertTrue(by_arm["gcx-bare"]["metrics"]["adopted"])
        self.assertTrue(by_arm["gcx-guide"]["metrics"]["adopted"])
        self.assertEqual(by_arm["gcx-guide"]["metrics"]["total_tokens"], 640)
        self.assertEqual(by_arm["gcx-guide"]["metrics"]["uncached_tokens"], 540)

    def test_agent_run_unforced_codex_dry_run_dispatch(self) -> None:
        import sys
        from unittest import mock
        import agent_run

        argv = [
            "agent_run.py",
            "--unforced",
            "--client",
            "codex",
            "--gcx",
            str(GCX_BIN),
            "--label",
            "dry",
            "--dry-run",
        ]
        with mock.patch.object(sys, "argv", argv):
            rc = agent_run.main()
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
