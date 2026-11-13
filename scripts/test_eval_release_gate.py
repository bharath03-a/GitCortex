"""Release gate: claims must cite verified, complete, clean, replicated eval results."""

from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path
from typing import Any

import eval_release_gate as gate

CANDIDATE = "a" * 64
BASELINE = "b" * 64


def paired(uncached: float = 1.4, non_inferior: bool = True) -> dict[str, Any]:
    ratio = {"geomean": uncached, "ci95": [uncached - 0.1, uncached + 0.1], "pairs": 6, "rounds": 3}
    return {
        "ratios": {"total_tokens": ratio, "uncached_tokens": ratio, "cost_usd": ratio},
        "success_delta": 0.0, "non_inferior": non_inferior, "discordant": {},
    }


def block(uncached: float = 1.4, false_safe: int = 0, non_inferior: bool = True) -> dict[str, Any]:
    disc = {k: {"num": 3, "den": 3, "rate": 1.0} for k in ("adoption", "right_tool", "restraint")}
    return {
        "runs": 12,
        "arms": {
            "baseline": {"false_safe": 0, "success": {"num": 6, "den": 6, "rate": 1.0}, "discoverability": None},
            "gcx-guide": {"false_safe": false_safe, "success": {"num": 6, "den": 6, "rate": 1.0}, "discoverability": disc},
        },
        "paired": {"gcx-guide": paired(uncached, non_inferior)},
    }


class World:
    """A temp repo root holding a candidate and a baseline result plus a manifest."""

    def __init__(self, root: Path) -> None:
        self.root = root
        (root / "results").mkdir()
        self.manifest: dict[str, Any] = {
            "candidate": {"gcx_sha256": CANDIDATE}, "baseline": {"gcx_sha256": BASELINE}, "min_rounds": 3,
            "results": [], "claims": [], "docs": [],
        }

    def add(self, result_id: str, role: str, lane: str, binary: str) -> None:
        raw = self.root / "results" / f"{result_id}.jsonl"
        raw.write_text(f"raw trace {result_id}\n")
        logs = self.root / "results" / f"{result_id}-logs"
        logs.mkdir()
        (logs / "r1.jsonl").write_text("client stream")
        trace = {
            "label": result_id, "lane": lane, "client": "claude", "client_version": "2.1.322",
            "model": "claude-sonnet-5-5", "created_at": "2026-11-11T00:00:00+00:00", "gcx_version": "gcx 0.8.0",
            "gcx_sha256": binary, "suite_sha256": "c" * 64, "core_suite_sha256": "d" * 64,
            "repo_commits": {"cobra": "e" * 40}, "harness": {"commit": "f" * 40, "dirty": False},
            "rounds": 3, "planned_runs": 36, "completed_runs": 36, "aborted": None,
            "logs": {"path": f"results/{result_id}-logs", "files": 1, "sha256": gate.logs_digest(logs)},
        }
        summary: dict[str, Any] = {
            "type": "eval-summary", "id": result_id, "min_rounds": 3, "variance_ok": True, "complete": True,
            "release_grade_harness": True, "traces": [trace],
            "groups": {"overall": {"all": block()}, "workflow": {"impact": block(), "restraint": block()},
                       "lane": {lane: block()}},
            "artifacts": [{"path": f"results/{result_id}.jsonl", "sha256": gate.sha256_file(raw)}],
        }
        path = self.root / "results" / f"{result_id}.summary.json"
        path.write_text(json.dumps(summary))
        self.manifest["results"].append({"path": f"results/{result_id}.summary.json", "role": role,
                                         "sha256": gate.sha256_file(path)})

    def edit(self, result_id: str, mutate) -> None:
        """Rewrite a summary and re-pin its checksum so only the intended defect remains."""
        path = self.root / "results" / f"{result_id}.summary.json"
        summary = json.loads(path.read_text())
        mutate(summary)
        path.write_text(json.dumps(summary))
        for entry in self.manifest["results"]:
            if entry["path"] == f"results/{result_id}.summary.json":
                entry["sha256"] = gate.sha256_file(path)

    def validate(self, docs: list[str] | None = None) -> tuple[list[str], list[str]]:
        return gate.validate(self.root, self.manifest, docs)


def valid_world(tmp: str) -> World:
    world = World(Path(tmp))
    world.add("cand-unforced", "candidate", "claude-code-unforced", CANDIDATE)
    world.add("cand-continuous", "candidate", "claude-code-continuous", CANDIDATE)
    world.add("base-unforced", "baseline", "claude-code-unforced", BASELINE)
    world.manifest["claims"] = [{
        "id": "uncached-overall", "result_id": "cand-unforced", "group": "overall/all", "arm": "gcx-guide",
        "metric": "uncached_tokens_ratio", "op": ">=", "value": 1.0,
    }]
    return world


class GateTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.world = valid_world(self._tmp.name)

    def errors(self, docs: list[str] | None = None) -> list[str]:
        return self.world.validate(docs)[0]

    def test_valid_evidence_passes(self) -> None:
        self.assertEqual(self.errors(), [])

    def test_missing_manifest_fails_closed(self) -> None:
        self.assertEqual(gate.main(["--root", self._tmp.name, "--manifest", "nope.json"]), 1)

    def test_summary_checksum_tamper_is_detected(self) -> None:
        path = self.world.root / "results" / "cand-unforced.summary.json"
        path.write_text(path.read_text() + " ")
        self.assertTrue(any("summary checksum mismatch" in e for e in self.errors()))

    def test_raw_trace_and_log_tamper_are_detected(self) -> None:
        (self.world.root / "results" / "cand-unforced.jsonl").write_text("edited")
        (self.world.root / "results" / "base-unforced-logs" / "r1.jsonl").write_text("edited")
        found = " ".join(self.errors())
        self.assertIn("raw trace checksum mismatch", found)
        self.assertIn("raw log checksum mismatch", found)

    def test_missing_raw_artifacts_fail(self) -> None:
        (self.world.root / "results" / "cand-unforced.jsonl").unlink()
        self.assertTrue(any("raw trace missing" in e for e in self.errors()))

    def test_binary_must_match_the_manifest_role(self) -> None:
        self.world.manifest["candidate"]["gcx_sha256"] = "9" * 64
        self.assertTrue(any("gcx_sha256 does not match" in e for e in self.errors()))

    def test_dirty_incomplete_or_under_replicated_runs_fail(self) -> None:
        for field, value, needle in (
            ("harness", {"commit": "f" * 40, "dirty": True}, "clean, pinned commit"),
            ("completed_runs", 30, "incomplete run"),
            ("rounds", 2, "3+ rounds required"),
        ):
            with self.subTest(field=field):
                world = valid_world(tempfile.mkdtemp())
                world.edit("cand-unforced", lambda s, f=field, v=value: s["traces"][0].update({f: v}))
                self.assertTrue(any(needle in e for e in world.validate()[0]), needle)

    def test_missing_provenance_fields_fail(self) -> None:
        def strip(summary: dict[str, Any]) -> None:
            for key in ("suite_sha256", "model", "repo_commits"):
                summary["traces"][0].pop(key)

        self.world.edit("cand-unforced", strip)
        found = " ".join(self.errors())
        for key in ("suite_sha256", "model", "repo_commits"):
            self.assertIn(key, found)

    def test_both_roles_and_lanes_are_required(self) -> None:
        self.world.manifest["results"] = [r for r in self.world.manifest["results"] if "base-" not in r["path"]]
        self.assertTrue(any("missing baseline result for lane claude-code-unforced" in e for e in self.errors()))
        self.world.manifest["results"] = [r for r in self.world.manifest["results"] if "continuous" not in r["path"]]
        self.assertTrue(any("missing candidate result for lane claude-code-continuous" in e for e in self.errors()))

    def test_false_safe_must_be_zero(self) -> None:
        self.world.edit("cand-unforced", lambda s: s["groups"]["overall"]["all"]["arms"]["gcx-guide"].update(false_safe=1))
        self.assertTrue(any("false-safe claims must be 0" in e for e in self.errors()))

    def test_non_inferiority_is_required_on_every_workflow(self) -> None:
        self.world.edit("cand-unforced", lambda s: s["groups"]["workflow"].update(impact=block(non_inferior=False)))
        self.assertTrue(any("workflow=impact" in e and "non-inferior" in e for e in self.errors()))

    def test_overall_uncached_loss_blocks_but_workflow_loss_is_reported(self) -> None:
        self.world.edit("cand-unforced", lambda s: s["groups"]["workflow"].update(restraint=block(uncached=0.8)))
        errors, losses = self.world.validate()
        self.assertEqual(errors, [])
        self.assertTrue(any("workflow=restraint" in loss for loss in losses))
        self.world.edit("cand-unforced", lambda s: s["groups"]["overall"].update(all=block(uncached=0.9)))
        self.assertTrue(any("overall=all" in e and "uncached ratio" in e for e in self.errors()))

    def test_claims_are_recomputed_from_the_cited_result(self) -> None:
        claims = self.world.manifest["claims"]
        claims[0]["value"] = 2.0
        self.assertTrue(any("not >= 2.0" in e for e in self.errors()))
        claims[0].update(value=1.0, result_id="ghost")
        self.assertTrue(any("unknown result id" in e for e in self.errors()))
        claims[0].update(result_id="cand-unforced", group="repo/none")
        self.assertTrue(any("not found" in e for e in self.errors()))
        claims[0].update(group="overall/all", op="~=")
        self.assertTrue(any("needs op" in e for e in self.errors()))

    def test_release_needs_at_least_one_claim(self) -> None:
        self.world.manifest["claims"] = []
        self.assertTrue(any("must list the claims" in e for e in self.errors()))

    def test_claim_on_an_under_replicated_result_is_refused(self) -> None:
        self.world.edit("cand-unforced", lambda s: s.update(variance_ok=False))
        self.assertTrue(any("fewer than 3 rounds" in e for e in self.errors()))

    def test_docs_need_citations_for_performance_claims(self) -> None:
        doc = self.world.root / "page.md"
        doc.write_text(
            "GitCortex cuts tokens by 40% fewer reads.\n"
            "Uses 1.4x fewer uncached tokens [result: cand-unforced].\n"
            "Cites a ghost [result: nope].\n"
            "Plain sentence without numbers.\n"
        )
        found = self.errors(["page.md"])
        self.assertTrue(any("page.md:1" in e and "without a [result: id] citation" in e for e in found))
        self.assertTrue(any("page.md:3" in e and "unknown result id" in e for e in found))
        self.assertFalse(any("page.md:2" in e for e in found))
        self.assertFalse(any("page.md:4" in e for e in found))

    def test_min_rounds_cannot_be_lowered(self) -> None:
        self.world.manifest["min_rounds"] = 1
        self.assertTrue(any("min_rounds must be" in e for e in self.errors()))

    def test_validation_does_not_mutate_the_manifest(self) -> None:
        before = copy.deepcopy(self.world.manifest)
        self.world.validate()
        self.assertEqual(before, self.world.manifest)


if __name__ == "__main__":
    unittest.main()
