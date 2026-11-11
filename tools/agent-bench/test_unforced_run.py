"""Unforced lane wiring: neutral prompts, arm isolation, ordering, cap enforcement."""

from __future__ import annotations

import json
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
