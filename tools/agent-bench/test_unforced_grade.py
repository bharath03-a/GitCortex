"""Grading tests: required evidence, forbidden evidence, false-safe claims."""

from __future__ import annotations

import unittest

from unforced_grade import grade_answer, no_dependents_claims


class GradeAnswerTests(unittest.TestCase):
    def test_all_required_found_is_success(self) -> None:
        grade = grade_answer("See command.go and completions.go", ("command.go", "completions.go"))
        self.assertTrue(grade.success)
        self.assertEqual(grade.quality_score, 1.0)
        self.assertEqual(grade.missing, ())

    def test_partial_evidence_is_not_success(self) -> None:
        grade = grade_answer("Only command.go matters", ("command.go", "completions.go"))
        self.assertFalse(grade.success)
        self.assertEqual(grade.quality_score, 0.5)
        self.assertEqual(grade.missing, ("completions.go",))

    def test_forbidden_evidence_fails_even_with_all_required(self) -> None:
        grade = grade_answer("command.go and tests/integration", ("command.go",), ("tests/integration",))
        self.assertFalse(grade.success)
        self.assertEqual(grade.forbidden_found, ("tests/integration",))

    def test_empty_answer_never_succeeds(self) -> None:
        self.assertFalse(grade_answer("   ", ()).success)
        self.assertFalse(grade_answer("", ("a",)).success)

    def test_escaped_slashes_are_normalized(self) -> None:
        grade = grade_answer("src\\/requests\\/sessions.py", ("src/requests/sessions.py",))
        self.assertTrue(grade.success)

    def test_exact_string_evidence(self) -> None:
        grade = grade_answer('fmt.Errorf("unknown command %q for %q%s")', ("unknown command %q for %q",))
        self.assertTrue(grade.success)


class FalseSafeTests(unittest.TestCase):
    def test_detects_no_callers_claim(self) -> None:
        self.assertTrue(no_dependents_claims("There are no callers of this function."))
        self.assertTrue(no_dependents_claims("Nothing else calls it, so it is fine."))
        self.assertTrue(no_dependents_claims("Safe to refactor."))

    def test_negated_safe_is_not_a_claim(self) -> None:
        self.assertEqual(no_dependents_claims("It is not safe to change without updating callers."), ())

    def test_claims_recorded_only_when_dependents_exist(self) -> None:
        text = "There are no callers."
        self.assertEqual(grade_answer(text, (), dependents_exist=False).claims_no_dependents, ())
        self.assertTrue(grade_answer(text, (), dependents_exist=True).claims_no_dependents)

    def test_listing_callers_is_not_a_claim(self) -> None:
        self.assertEqual(no_dependents_claims("Callers: command.go, completions.go."), ())


if __name__ == "__main__":
    unittest.main()
