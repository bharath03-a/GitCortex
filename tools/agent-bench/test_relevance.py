"""Tests for the model-free search relevance scorer."""

from __future__ import annotations

import unittest

from relevance import RelevanceTruth, score_relevance


def evidence(*pairs: tuple[str, str]) -> list[dict[str, str]]:
    """Build ranked search evidence from (file, qualified_name) pairs."""
    return [
        {"file": file, "qualified_name": qualified, "symbol": qualified.split(".")[-1]}
        for file, qualified in pairs
    ]


class MeanReciprocalRankTest(unittest.TestCase):
    def test_first_hit_relevant_scores_one(self) -> None:
        result = score_relevance(
            evidence(("src/requests/sessions.py", "requests.sessions.Session")),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertEqual(result.mrr, 1.0)

    def test_third_hit_relevant_scores_one_third(self) -> None:
        result = score_relevance(
            evidence(
                ("src/requests/adapters.py", "requests.adapters.HTTPAdapter"),
                ("src/requests/models.py", "requests.models.Response"),
                ("src/requests/sessions.py", "requests.sessions.Session"),
            ),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertAlmostEqual(result.mrr, 1 / 3)

    def test_no_relevant_hit_scores_zero(self) -> None:
        result = score_relevance(
            evidence(("src/requests/adapters.py", "requests.adapters.HTTPAdapter")),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertEqual(result.mrr, 0.0)

    def test_empty_evidence_scores_zero(self) -> None:
        result = score_relevance(
            [], RelevanceTruth(files=("src/requests/sessions.py",), symbols=())
        )
        self.assertEqual(result.mrr, 0.0)


class SymbolRelevanceTest(unittest.TestCase):
    def test_qualified_name_match_is_relevant(self) -> None:
        result = score_relevance(
            evidence(("src/requests/sessions.py", "requests.sessions.Session")),
            RelevanceTruth(files=(), symbols=("requests.sessions.Session",)),
        )
        self.assertEqual(result.mrr, 1.0)

    def test_bare_symbol_matches_qualified_name_tail(self) -> None:
        result = score_relevance(
            evidence(("src/hono-base.ts", "Hono#dispatch")),
            RelevanceTruth(files=(), symbols=("dispatch",)),
        )
        self.assertEqual(result.mrr, 1.0)

    def test_symbol_does_not_match_unrelated_suffix(self) -> None:
        """`Session` must not match `MockSession` — tail match needs a separator."""
        result = score_relevance(
            evidence(("tests/test_sessions.py", "tests.helpers.MockSession")),
            RelevanceTruth(files=(), symbols=("Session",)),
        )
        self.assertEqual(result.mrr, 0.0)

    def test_file_or_symbol_match_suffices(self) -> None:
        result = score_relevance(
            evidence(("src/requests/sessions.py", "requests.sessions.Session")),
            RelevanceTruth(files=("nowhere.py",), symbols=("Session",)),
        )
        self.assertEqual(result.mrr, 1.0)


class PrecisionAtFiveTest(unittest.TestCase):
    def test_counts_relevant_hits_in_top_five(self) -> None:
        result = score_relevance(
            evidence(
                ("src/requests/sessions.py", "requests.sessions.Session"),
                ("src/requests/adapters.py", "requests.adapters.HTTPAdapter"),
                ("src/requests/sessions.py", "requests.sessions.SessionRedirectMixin"),
                ("src/requests/models.py", "requests.models.Response"),
                ("src/requests/utils.py", "requests.utils.guess_json_utf"),
            ),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertAlmostEqual(result.precision_at_5, 2 / 5)

    def test_ignores_hits_beyond_rank_five(self) -> None:
        result = score_relevance(
            evidence(
                *[("src/requests/utils.py", f"requests.utils.helper_{index}") for index in range(5)],
                ("src/requests/sessions.py", "requests.sessions.Session"),
            ),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertEqual(result.precision_at_5, 0.0)

    def test_short_result_divides_by_five_not_by_length(self) -> None:
        """A single correct hit is not perfect precision — recall shortfall must show."""
        result = score_relevance(
            evidence(("src/requests/sessions.py", "requests.sessions.Session")),
            RelevanceTruth(files=("src/requests/sessions.py",), symbols=()),
        )
        self.assertAlmostEqual(result.precision_at_5, 1 / 5)

    def test_empty_evidence_scores_zero(self) -> None:
        result = score_relevance([], RelevanceTruth(files=("a.py",), symbols=()))
        self.assertEqual(result.precision_at_5, 0.0)


class FileRecallTest(unittest.TestCase):
    def test_finds_half_of_pinned_files(self) -> None:
        result = score_relevance(
            evidence(("src/requests/sessions.py", "requests.sessions.Session")),
            RelevanceTruth(
                files=("src/requests/sessions.py", "src/requests/adapters.py"),
                symbols=(),
            ),
        )
        self.assertAlmostEqual(result.file_recall, 0.5)

    def test_all_pinned_files_found_scores_one(self) -> None:
        result = score_relevance(
            evidence(
                ("src/requests/sessions.py", "requests.sessions.Session"),
                ("src/requests/adapters.py", "requests.adapters.HTTPAdapter"),
            ),
            RelevanceTruth(
                files=("src/requests/sessions.py", "src/requests/adapters.py"),
                symbols=(),
            ),
        )
        self.assertEqual(result.file_recall, 1.0)

    def test_duplicate_hits_in_one_file_count_once(self) -> None:
        result = score_relevance(
            evidence(
                ("src/requests/sessions.py", "requests.sessions.Session"),
                ("src/requests/sessions.py", "requests.sessions.SessionRedirectMixin"),
            ),
            RelevanceTruth(
                files=("src/requests/sessions.py", "src/requests/adapters.py"),
                symbols=(),
            ),
        )
        self.assertAlmostEqual(result.file_recall, 0.5)

    def test_recall_is_not_capped_by_the_precision_cutoff(self) -> None:
        """A pinned file found at rank 9 still counts — recall spans the whole list."""
        result = score_relevance(
            evidence(
                *[("src/requests/utils.py", f"requests.utils.helper_{index}") for index in range(8)],
                ("src/requests/adapters.py", "requests.adapters.HTTPAdapter"),
            ),
            RelevanceTruth(files=("src/requests/adapters.py",), symbols=()),
        )
        self.assertEqual(result.file_recall, 1.0)

    def test_no_pinned_files_is_vacuously_complete(self) -> None:
        result = score_relevance(
            evidence(("src/hono-base.ts", "Hono#dispatch")),
            RelevanceTruth(files=(), symbols=("dispatch",)),
        )
        self.assertEqual(result.file_recall, 1.0)


if __name__ == "__main__":
    unittest.main()
