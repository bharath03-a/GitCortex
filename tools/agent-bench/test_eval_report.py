"""Report generation: normalization, grouping, variance, negative results, provenance."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from typing import Any

from eval_load import load_trace, workflow_of_task
from eval_render import render_markdown
from eval_report import build_summary, main
from eval_stats import bootstrap_ci, geomean, paired_ratios, summarize_pairs


def run(task="cobra-impact-add-command", repo="cobra", kind="graph", arm="baseline", rnd=1, success=True,
        total=1000, uncached=100, cost=0.05, adopted=False, calls=0, errors=0, first=None, status="ok",
        claims=(), not_ready=None, connected=None) -> dict[str, Any]:
    return {
        "type": "run", "task_id": task, "repo": repo, "kind": kind, "arm": arm, "round": rnd,
        "success": success, "status": status, "wall_ms": 2000,
        "metrics": {
            "cost_usd": cost, "total_tokens": total, "uncached_tokens": uncached, "turns": 3, "tool_calls": 2,
            "gcx_calls": calls, "gcx_errors": errors, "adopted": adopted, "first_gcx_position": first,
            "tokens_before_first_gcx_total": 500 if first else None,
            "tokens_before_first_gcx_uncached": 50 if first else None,
            "gcx_connected": connected, "not_ready": not_ready, "repeat_reads": 0, "model": "claude-sonnet-5-5",
        },
        "grade": {"claims_no_dependents": list(claims)},
    }


def write_trace(folder: Path, runs: list[dict[str, Any]], label="t1", planned=None, aborted=None,
                dirty=False, lane="claude-code-unforced") -> Path:
    meta = {
        "type": "meta", "lane": lane, "label": label, "client": "claude", "client_version": "2.1.322",
        "model_requested": "sonnet", "created_at": "2026-11-11T00:00:00+00:00", "gcx_sha256": "a" * 64,
        "gcx_version": "gcx 0.7.6", "suite_sha256": "b" * 64, "core_suite_sha256": "c" * 64,
        "repo_commits": {"cobra": "d" * 40}, "harness": {"commit": "e" * 40, "dirty": dirty}, "seed": 1,
        "planned_runs": len(runs) if planned is None else planned,
    }
    path = folder / f"{label}.unforced.jsonl"
    lines = [meta, *runs, {"type": "summary", "aborted": aborted, "budget": {"spent_usd": 1.5}}]
    path.write_text("\n".join(json.dumps(x) for x in lines) + "\n")
    return path


def three_round_runs(arm_total: int = 500) -> list[dict[str, Any]]:
    rows = []
    for rnd in (1, 2, 3):
        for task, kind in (("cobra-impact-add-command", "graph"), ("requests-restraint-redirect-limit", "restraint")):
            repo = "cobra" if task.startswith("cobra") else "requests"
            rows.append(run(task, repo, kind, "baseline", rnd, total=1000, uncached=100 * rnd))
            rows.append(run(task, repo, kind, "gcx-guide", rnd, total=arm_total, uncached=50,
                            adopted=kind == "graph", calls=1 if kind == "graph" else 0,
                            first=2 if kind == "graph" else None, connected=True))
    return rows


class LoadTests(unittest.TestCase):
    def test_workflow_and_language_are_derived(self) -> None:
        self.assertEqual(workflow_of_task("hono-impact-compose", "graph"), "impact")
        self.assertEqual(workflow_of_task("cobra-callers-add-command", "graph"), "callers")
        self.assertEqual(workflow_of_task("x", "restraint"), "restraint")

    def test_rows_keep_failed_and_timed_out_runs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = write_trace(Path(tmp), [run(success=False, status="timeout"), run(arm="gcx-bare")])
            trace = load_trace(path)
            self.assertEqual(len(trace.rows), 2)
            self.assertEqual(trace.rows[0].language, "go")
            self.assertEqual(trace.rows[0].model, "claude-sonnet-5-5")
            self.assertEqual(trace.rows[0].status, "timeout")


class StatsTests(unittest.TestCase):
    def test_geomean_ignores_non_positive(self) -> None:
        self.assertAlmostEqual(geomean([2, 8, 0, -1]), 4.0)
        self.assertIsNone(geomean([0]))

    def test_pairs_need_both_arms_and_use_baseline_over_arm(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rows = load_trace(write_trace(Path(tmp), [
                run(arm="baseline", total=1000), run(arm="gcx-bare", total=500),
                run(task="other-task", arm="baseline", total=10),
            ])).rows
            pairs = paired_ratios(rows, "total_tokens", "gcx-bare")
            self.assertEqual([p.ratio for p in pairs], [2.0])

    def test_bootstrap_is_deterministic_and_needs_two_pairs(self) -> None:
        self.assertIsNone(bootstrap_ci([1.5]))
        self.assertEqual(bootstrap_ci([1.2, 1.5, 0.9, 2.0], 3), bootstrap_ci([1.2, 1.5, 0.9, 2.0], 3))

    def test_round_variance_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rows = load_trace(write_trace(Path(tmp), three_round_runs())).rows
            stats = summarize_pairs(paired_ratios(rows, "uncached_tokens", "gcx-guide"))
            self.assertEqual(stats["rounds"], 3)
            self.assertEqual(sorted(stats["round_geomeans"]), ["1", "2", "3"])
            self.assertGreater(stats["round_stdev"], 0)


class SummaryTests(unittest.TestCase):
    def _summary(self, runs, **kwargs):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        return build_summary([load_trace(write_trace(Path(tmp.name), runs, **kwargs))], "unit")

    def test_groups_by_repo_language_workflow_arm_client_model(self) -> None:
        summary = self._summary(three_round_runs())
        groups = summary["groups"]
        self.assertEqual(set(groups["repo"]), {"cobra", "requests"})
        self.assertEqual(set(groups["language"]), {"go", "python"})
        self.assertEqual(set(groups["workflow"]), {"impact", "restraint"})
        self.assertEqual(set(groups["client"]), {"claude"})
        self.assertEqual(set(groups["model"]), {"claude-sonnet-5-5"})
        self.assertEqual(set(groups["overall"]["all"]["arms"]), {"baseline", "gcx-guide"})

    def test_discoverability_metrics(self) -> None:
        disc = self._summary(three_round_runs())["groups"]["overall"]["all"]["arms"]["gcx-guide"]["discoverability"]
        self.assertEqual(disc["adoption"]["num"], 3)
        self.assertEqual(disc["right_tool"]["rate"], 1.0)
        self.assertEqual(disc["restraint"]["rate"], 1.0)
        self.assertEqual(disc["first_position_median"], 2)
        self.assertEqual(disc["connection_success"]["rate"], 1.0)

    def test_gcx_error_rate_and_not_ready_handling(self) -> None:
        runs = [
            run(arm="baseline"),
            run(arm="gcx-bare", adopted=True, calls=4, errors=1, first=1,
                not_ready={"correct": True, "fell_back": True, "disclosed": True}),
            run(task="b", arm="baseline"),
            run(task="b", arm="gcx-bare", adopted=True, calls=1, first=1,
                not_ready={"correct": False, "fell_back": False, "disclosed": False}),
        ]
        disc = self._summary(runs)["groups"]["overall"]["all"]["arms"]["gcx-bare"]["discoverability"]
        self.assertEqual((disc["gcx_error_rate"]["num"], disc["gcx_error_rate"]["den"]), (1, 5))
        self.assertEqual((disc["not_ready_correct"]["num"], disc["not_ready_correct"]["den"]), (1, 2))

    def test_variance_flag_requires_three_rounds(self) -> None:
        self.assertTrue(self._summary(three_round_runs())["variance_ok"])
        self.assertFalse(self._summary([r for r in three_round_runs() if r["round"] == 1])["variance_ok"])

    def test_losses_failures_and_unused_tool_are_kept_as_negative_results(self) -> None:
        runs = [
            run(arm="baseline", total=500, success=True),
            run(arm="gcx-bare", total=1000, success=False, status="timeout", claims=("no callers",)),
        ]
        summary = self._summary(runs)
        metrics = {(n["metric"], n["arm"]) for n in summary["negative_results"]}
        self.assertIn(("total_tokens_ratio", "gcx-bare"), metrics)
        self.assertIn(("success_delta", "gcx-bare"), metrics)
        self.assertIn(("non_inferior", "gcx-bare"), metrics)
        self.assertIn(("false_safe", "gcx-bare"), metrics)
        self.assertIn(("run_status", "gcx-bare"), metrics)
        self.assertIn(("adoption", "gcx-bare"), metrics)
        arm = summary["groups"]["overall"]["all"]["arms"]["gcx-bare"]
        self.assertEqual(arm["success"], {"num": 0, "den": 1, "rate": 0.0})

    def test_incomplete_and_dirty_traces_are_flagged(self) -> None:
        summary = self._summary([run(), run(arm="gcx-bare")], planned=4, aborted="spend cap reached", dirty=True)
        self.assertFalse(summary["complete"])
        self.assertFalse(summary["release_grade_harness"])

    def test_artifact_and_log_checksums_are_recorded(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = write_trace(Path(tmp), [run(), run(arm="gcx-bare")])
            logs = path.with_name(path.stem + "-logs")
            logs.mkdir()
            (logs / "r1.jsonl").write_text("raw")
            block = build_summary([load_trace(path)], "unit")["traces"][0]
            self.assertEqual(len(block["sha256"]), 64)
            self.assertEqual(block["logs"]["files"], 1)
            self.assertEqual(len(block["logs"]["sha256"]), 64)

    def test_episode_rows_carry_patch_quality(self) -> None:
        episode = {
            "type": "episode", "episode": "requests-auth-redirect", "repo": "requests", "language": "python",
            "family": "cross-module-debug", "arm": "gcx-guide", "round": 1, "success": True, "aborted": None,
            "false_no_dependents": True,
            "steps": [{"metrics": {"gcx_connected": True, "model": "claude-sonnet-5-5"}}],
            "patch": {"insertions": 3, "deletions": 1, "unrelated_files": ["a.py"]},
            "metrics": {"cost_usd": 0.5, "total_tokens": 9, "uncached_tokens": 5, "wall_ms": 9, "turns": 4,
                        "tool_calls": 6, "gcx_calls": 1, "gcx_errors": 0, "adopted": True,
                        "first_gcx_position": 3, "repeat_reads": 2},
        }
        with tempfile.TemporaryDirectory() as tmp:
            row = load_trace(write_trace(Path(tmp), [episode], lane="claude-code-continuous")).rows[0]
        self.assertEqual((row.patch_lines, row.unrelated_files, row.language), (4, 1, "python"))
        self.assertTrue(row.false_safe)
        self.assertEqual(row.workflow, "cross-module-debug")


class RenderTests(unittest.TestCase):
    def test_markdown_has_tables_banner_and_negative_section(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = write_trace(Path(tmp), [r for r in three_round_runs() if r["round"] == 1])
            text = render_markdown(build_summary([load_trace(path)], "unit"))
        for needle in ("## 4A. Task outcome", "## 4B. Discoverability", "### By repo", "### By language",
                       "### By workflow", "### By client", "### By model", "Not release evidence",
                       "Negative and inconclusive results"):
            self.assertIn(needle, text)

    def test_cli_writes_summary_and_report(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = write_trace(Path(tmp), three_round_runs())
            out = Path(tmp) / "out"
            self.assertEqual(main([str(path), "--id", "cli", "--output-dir", str(out)]), 0)
            summary = json.loads((out / "cli.summary.json").read_text())
            self.assertEqual(summary["id"], "cli")
            self.assertTrue((out / "cli.report.md").exists())
            self.assertEqual(main([str(Path(tmp) / "missing.jsonl"), "--id", "x", "--output-dir", str(out)]), 2)


if __name__ == "__main__":
    unittest.main()
