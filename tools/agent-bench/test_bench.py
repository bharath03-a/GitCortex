"""Tests for suite parsing and relevance wiring in the retrieval lane."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from bench import compare_traces, load_suite, score_task, summarize, write_trace

SUITE = """
suite = "test-suite"
version = 1

[[repos]]
name = "requests"
url = "https://github.com/psf/requests"
commit = "f361ead047be5cb873174218582f7d8b9fcd9f49"

[[tasks]]
id = "requests-search-session"
repo = "requests"
action = "search"
query = "Session"
required = ["src/requests/sessions.py"]
max_bytes = 5000
relevant_files = ["src/requests/sessions.py", "src/requests/adapters.py"]
relevant_symbols = ["Session"]

[[tasks]]
id = "requests-callers-send"
repo = "requests"
action = "callers"
query = "SessionRedirectMixin.send"
required = ["src/requests/sessions.py"]
max_bytes = 4000
"""


def write_suite(body: str) -> Path:
    handle = tempfile.NamedTemporaryFile(
        "w", suffix=".toml", delete=False, encoding="utf-8"
    )
    handle.write(body)
    handle.close()
    return Path(handle.name)


#: Symbol reported for each file, so only pinned files/symbols score as relevant.
SYMBOL_BY_FILE = {
    "src/requests/sessions.py": "requests.sessions.Session",
    "src/requests/adapters.py": "requests.adapters.HTTPAdapter",
    "src/requests/models.py": "requests.models.Response",
}


def search_payload(*files: str) -> str:
    return json.dumps(
        {
            "status": "ok",
            "evidence": [
                {
                    "symbol": SYMBOL_BY_FILE[file].rsplit(".", 1)[-1],
                    "qualified_name": SYMBOL_BY_FILE[file],
                    "file": file,
                    "line": 1,
                }
                for file in files
            ],
        }
    )


class SuiteRelevanceParsingTest(unittest.TestCase):
    def setUp(self) -> None:
        self.path = write_suite(SUITE)
        _, _, tasks = load_suite(self.path)
        self.tasks = {task.id: task for task in tasks}

    def tearDown(self) -> None:
        self.path.unlink(missing_ok=True)

    def test_parses_relevance_ground_truth(self) -> None:
        task = self.tasks["requests-search-session"]
        self.assertEqual(
            task.relevant_files,
            ("src/requests/sessions.py", "src/requests/adapters.py"),
        )
        self.assertEqual(task.relevant_symbols, ("Session",))

    def test_tasks_without_ground_truth_default_to_empty(self) -> None:
        task = self.tasks["requests-callers-send"]
        self.assertEqual(task.relevant_files, ())
        self.assertEqual(task.relevant_symbols, ())


class ScoreTaskRelevanceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.path = write_suite(SUITE)
        _, _, tasks = load_suite(self.path)
        self.tasks = {task.id: task for task in tasks}

    def tearDown(self) -> None:
        self.path.unlink(missing_ok=True)

    def score(self, task_id: str, stdout: str):
        return score_task(
            self.tasks[task_id],
            ["gcx", "query", "search"],
            0,
            12,
            stdout,
            "",
            require_contract=True,
        )

    def test_reports_relevance_for_search_tasks(self) -> None:
        result = self.score(
            "requests-search-session",
            search_payload("src/requests/sessions.py", "src/requests/models.py"),
        )
        self.assertEqual(result.mrr, 1.0)
        self.assertAlmostEqual(result.precision_at_5, 1 / 5)
        self.assertAlmostEqual(result.file_recall, 0.5)

    def test_ranking_change_moves_mrr(self) -> None:
        """Same files, worse order — bytes identical, ranking metric must react."""
        good = self.score(
            "requests-search-session",
            search_payload("src/requests/sessions.py", "src/requests/models.py"),
        )
        bad = self.score(
            "requests-search-session",
            search_payload("src/requests/models.py", "src/requests/sessions.py"),
        )
        self.assertEqual(good.payload_bytes, bad.payload_bytes)
        self.assertGreater(good.mrr, bad.mrr)

    def test_tasks_without_ground_truth_report_no_relevance(self) -> None:
        result = self.score("requests-callers-send", search_payload("src/requests/sessions.py"))
        self.assertIsNone(result.mrr)
        self.assertIsNone(result.precision_at_5)
        self.assertIsNone(result.file_recall)


class SummarizeRelevanceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.path = write_suite(SUITE)
        _, _, tasks = load_suite(self.path)
        self.tasks = {task.id: task for task in tasks}

    def tearDown(self) -> None:
        self.path.unlink(missing_ok=True)

    def score(self, task_id: str, stdout: str):
        return score_task(
            self.tasks[task_id], ["gcx"], 0, 12, stdout, "", require_contract=True
        )

    def test_reports_mean_relevance_for_scored_actions(self) -> None:
        summary = summarize(
            [
                self.score(
                    "requests-search-session",
                    search_payload("src/requests/sessions.py"),
                ),
                self.score(
                    "requests-search-session",
                    search_payload("src/requests/models.py", "src/requests/sessions.py"),
                ),
            ]
        )
        search = summary["by_action"]["search"]
        self.assertAlmostEqual(search["mrr_mean"], 0.75)
        self.assertEqual(search["relevance_tasks"], 2)

    def test_omits_relevance_for_actions_without_ground_truth(self) -> None:
        summary = summarize(
            [self.score("requests-callers-send", search_payload("src/requests/sessions.py"))]
        )
        self.assertNotIn("mrr_mean", summary["by_action"]["callers"])


class CompareRelevanceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.path = write_suite(SUITE)
        _, _, tasks = load_suite(self.path)
        self.tasks = {task.id: task for task in tasks}
        self.directory = tempfile.TemporaryDirectory()

    def tearDown(self) -> None:
        self.path.unlink(missing_ok=True)
        self.directory.cleanup()

    def trace(self, name: str, *files: str) -> Path:
        result = score_task(
            self.tasks["requests-search-session"],
            ["gcx"],
            0,
            12,
            search_payload(*files),
            "",
            require_contract=True,
        )
        path = Path(self.directory.name) / f"{name}.jsonl"
        write_trace(path, {"label": name, "suite_sha256": "test"}, [result])
        return path

    def test_reports_ranking_improvement(self) -> None:
        base = self.trace("main", "src/requests/models.py", "src/requests/sessions.py")
        head = self.trace("head", "src/requests/sessions.py", "src/requests/models.py")
        report = compare_traces(base, head)
        row = report["comparisons"][0]
        self.assertAlmostEqual(row["mrr_before"], 0.5)
        self.assertEqual(row["mrr_after"], 1.0)
        self.assertTrue(report["relevance_non_inferior"])

    def test_flags_ranking_regression(self) -> None:
        base = self.trace("main", "src/requests/sessions.py", "src/requests/models.py")
        head = self.trace("head", "src/requests/models.py", "src/requests/sessions.py")
        report = compare_traces(base, head)
        self.assertFalse(report["relevance_non_inferior"])


if __name__ == "__main__":
    unittest.main()
