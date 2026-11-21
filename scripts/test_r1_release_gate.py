import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import subprocess
import shutil
import unittest


spec = importlib.util.spec_from_file_location("r1_release_gate", Path(__file__).with_name("r1_release_gate.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "suite.toml").write_text('suite="fixture"\n[[repos]]\nname="fixture"\ncommit="' + 'd' * 40 + '"\n[[tasks]]\nid="task"\n')
        (self.root / "gcx").write_bytes(b"fixture candidate")
        competitors = {name: {"url": url, "commit": "e" * 40, "install_command": "fixture-install", "runtime_versions": {"runtime": "fixture-version"}}
                       for name, url in gate.COMPETITOR_URLS.items()}
        (self.root / "competitors.json").write_text(json.dumps(competitors))
        self.manifest = {"candidate_commit": "a" * 40, "binary_sha256": gate.sha256(self.root / "gcx"),
                         "suite_sha256": gate.sha256(self.root / "suite.toml"),
                         "suite_path": "suite.toml", "binary_path": "gcx",
                         "source_tree_sha256": "f" * 64,
                         "competitors_path": "competitors.json", "competitors_sha256": gate.sha256(self.root / "competitors.json"),
                         "required_lanes": ["retrieval", "codex-graph-cli", "claude-code-mcp", "graphify", "codegraph"],
                         "results": [], "claims": []}
        for lane in self.manifest["required_lanes"]:
            path = self.root / (lane + ".json")
            raw = self.root / (lane + ".jsonl")
            raw.write_text('{"type":"fixture"}\n')
            data = {"id": lane + "-candidate", "lane": lane,
                    "candidate_commit": "a" * 40, "binary_sha256": self.manifest["binary_sha256"],
                    "suite_sha256": self.manifest["suite_sha256"], "repo_commits": {"fixture": "d" * 40},
                    "client": {"codex-graph-cli":"codex", "claude-code-mcp":"claude"}.get(lane, "none"),
                    "model": "none" if lane == "retrieval" else "pinned-model",
                    "rounds": 1 if lane == "retrieval" else 3, "cache_state": "warm-index",
                    "tasks": [{"id": "task", "valid": True, "quality_non_inferior": True,
                               "samples": [{"round": n, "valid": True, "quality_non_inferior": True} for n in range(1, 2 if lane == "retrieval" else 4)]}],
                    "relevance_non_inferior": True,
                    "artifacts": [{"path": raw.name, "sha256": gate.sha256(raw)}]}
            if lane in competitors:
                data["competitor"] = competitors[lane]
            path.write_text(json.dumps(data))
            self.manifest["results"].append({"path": path.name, "sha256": gate.sha256(path)})

    def check(self, dirty=False, versions=None):
        return gate.validate(self.root, self.manifest, versions or {"cargo": "0.8.0", "npm": "0.8.0"}, dirty)

    def mutate(self, lane, change):
        entry = self.manifest["results"][lane]
        path = self.root / entry["path"]
        data = json.loads(path.read_text())
        change(data)
        path.write_text(json.dumps(data))
        entry["sha256"] = gate.sha256(path)

    def test_valid_evidence(self):
        self.assertEqual(self.check(), [])

    def test_missing_file_or_lane(self):
        (self.root / self.manifest["results"][0]["path"]).unlink()
        self.assertTrue(self.check())
        self.manifest["results"] = []
        self.assertTrue(self.check())

    def test_every_provenance_field_is_required(self):
        original = copy.deepcopy(self.manifest)
        original_data = (self.root / self.manifest["results"][1]["path"]).read_text()
        for field in ["binary_sha256", "suite_sha256", "candidate_commit", "repo_commits", "client", "model", "rounds", "cache_state"]:
            with self.subTest(field=field):
                self.manifest = copy.deepcopy(original)
                (self.root / self.manifest["results"][1]["path"]).write_text(original_data)
                self.mutate(1, lambda d: d.pop(field, None))
                self.assertTrue(self.check())

    def test_invalid_task_quality_and_rounds(self):
        self.mutate(0, lambda d: d["tasks"][0].update(valid=False))
        self.assertTrue(self.check())
        self.mutate(0, lambda d: d["tasks"][0].update(valid=True, quality_non_inferior=False))
        self.assertTrue(self.check())
        self.mutate(1, lambda d: d.update(rounds=1))
        self.assertTrue(self.check())

    def test_claim_requires_matching_result_and_task(self):
        self.manifest["claims"] = [{"id": "claim", "result_id": "absent", "task_ids": ["task"]}]
        self.assertTrue(self.check())
        self.manifest["claims"][0]["result_id"] = "retrieval-candidate"
        self.assertEqual(self.check(), [])
        self.manifest["claims"][0]["task_ids"] = ["invented"]
        self.assertTrue(self.check())

    def test_version_mismatch_and_dirty_worktree(self):
        self.assertTrue(self.check(versions={"cargo": "0.8.0", "npm": "0.7.6"}))
        self.assertTrue(self.check(dirty=True))

    def test_hash_tampering_and_path_escape(self):
        entry = self.manifest["results"][0]
        entry["sha256"] = "e" * 64
        self.assertTrue(self.check())
        entry["path"] = "../outside.json"
        self.assertTrue(self.check())

    def test_no_empty_or_duplicate_evidence(self):
        self.mutate(0, lambda d: d.update(tasks=[]))
        self.assertTrue(self.check())

    def test_raw_evidence_required_and_checksummed(self):
        self.mutate(0, lambda d: d.pop("artifacts"))
        self.assertTrue(self.check())

    def test_cannot_remove_lanes_or_task_samples(self):
        self.manifest["required_lanes"] = ["retrieval"]
        self.assertTrue(self.check())
        self.mutate(1, lambda d: d["tasks"][0].update(samples=[]))
        self.assertTrue(self.check())

    def test_pinned_inputs_and_repository_identity(self):
        self.mutate(0, lambda d: d.update(repo_commits={"fixture": "e" * 40}))
        self.assertTrue(self.check())

    def test_competitor_losses_are_retained_and_unpinned_tools_fail(self):
        self.mutate(3, lambda d: d["tasks"][0].update(quality_non_inferior=False, outcome="loss"))
        self.assertEqual(self.check(), [])
        self.mutate(3, lambda d: d.pop("competitor"))
        self.assertTrue(self.check())

    def test_excessive_rounds_and_malformed_claim_fail_closed(self):
        self.mutate(1, lambda d: d.update(rounds=10**20))
        self.manifest["claims"] = [{"result_id": {}, "task_ids": ["task"]}]
        self.assertTrue(self.check())

    def test_package_inventory_and_cargo_dependency_requirements(self):
        source = Path(__file__).resolve().parent.parent
        paths = [Path("Cargo.toml"), Path("Cargo.lock"), Path("python/src/gitcortex/__init__.py")]
        paths += [path.relative_to(source) for path in (source / "crates").glob("*/Cargo.toml")]
        paths += [path.relative_to(source) for path in (source / "npm/packages").glob("*/package.json")]
        for path in paths:
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / path, self.root / path)
        self.assertTrue(gate.package_versions(self.root))
        cargo = self.root / "Cargo.toml"
        original = cargo.read_text()
        cargo.write_text(original.replace('version = "0.7"', 'version = "0.6"'))
        with self.assertRaises(ValueError):
            gate.package_versions(self.root)
        cargo.write_text(original)
        (self.root / "npm/packages/gitcortex-linux-arm64/package.json").unlink()
        with self.assertRaises(OSError):
            gate.package_versions(self.root)

    def test_source_tree_identity_survives_squash_but_detects_code_changes(self):
        def git(*args):
            return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()
        git("init", "-b", "candidate")
        (self.root / "app.py").write_text("print('candidate')\n")
        git("add", "app.py")
        git("-c", "user.name=Test", "-c", "user.email=t@t", "commit", "-m", "candidate")
        candidate = gate.source_tree_hash(self.root)
        git("checkout", "--orphan", "squashed")
        git("-c", "user.name=Test", "-c", "user.email=t@t", "commit", "-m", "squashed")
        self.assertEqual(gate.source_tree_hash(self.root), candidate)
        (self.root / "docs").mkdir()
        (self.root / "docs/report.md").write_text("evidence")
        git("add", "docs/report.md")
        git("-c", "user.name=Test", "-c", "user.email=t@t", "commit", "-m", "evidence")
        self.assertEqual(gate.source_tree_hash(self.root), candidate)
        (self.root / "app.py").write_text("print('changed')\n")
        git("add", "app.py")
        git("-c", "user.name=Test", "-c", "user.email=t@t", "commit", "-m", "changed")
        self.assertNotEqual(gate.source_tree_hash(self.root), candidate)

    def test_suite_input_hash_change_fails(self):
        self.assertEqual(self.check(), [])
        (self.root / "suite.toml").write_text("changed suite")
        self.assertTrue(self.check())

    def test_raw_artifact_checksum_mismatch_fails(self):
        self.mutate(1, lambda d: d["artifacts"][0].update(sha256="f" * 64))
        self.assertTrue(self.check())

    def test_duplicate_result_ids_fail(self):
        self.manifest["results"].append(self.manifest["results"][1])
        self.assertTrue(self.check())

    def test_native_lane_needs_three_rounds(self):
        self.mutate(1, lambda d: d.update(rounds=1))
        self.assertTrue(self.check())

    def test_native_identity_cannot_be_none(self):
        self.mutate(1, lambda d: d.update(model="none"))
        self.assertTrue(self.check())

    def test_malformed_native_model_fails_without_exception(self):
        self.mutate(1, lambda d: d.update(model={"unexpected":"object"}))
        self.assertTrue(self.check())

    def test_empty_competitor_runtime_is_not_a_pin(self):
        path = self.root / "competitors.json"
        pins = json.loads(path.read_text())
        pins["graphify"]["runtime_versions"] = {"python":""}
        path.write_text(json.dumps(pins))
        self.manifest["competitors_sha256"] = gate.sha256(path)
        self.mutate(3, lambda d: d.update(competitor=pins["graphify"]))
        self.assertTrue(self.check())

    def test_malformed_claim_fails_without_other_errors(self):
        self.manifest["claims"] = [{"id": "claim", "result_id": {}, "task_ids": ["task"]}]
        self.assertTrue(self.check())

    def test_competitor_unsupported_is_accounted_for_and_excluded_from_claims(self):
        path = self.root / "suite.toml"
        path.write_text(path.read_text() + '\n[[tasks]]\nid="task2"\n')
        self.manifest["suite_sha256"] = gate.sha256(path)
        for index in range(len(self.manifest["results"])):
            def add_task(data):
                data["suite_sha256"] = self.manifest["suite_sha256"]
                task = copy.deepcopy(data["tasks"][0])
                task["id"] = "task2"
                data["tasks"].append(task)
            self.mutate(index, add_task)
        self.mutate(3, lambda d: d["tasks"][1].update(outcome="unsupported", reason="No documented counterpart", valid=False, samples=[]))
        self.assertEqual(self.check(), [])
        self.manifest["claims"] = [{"id":"bad", "result_id":"graphify-candidate", "task_ids":["task2"]}]
        self.assertTrue(self.check())


if __name__ == "__main__":
    unittest.main()
