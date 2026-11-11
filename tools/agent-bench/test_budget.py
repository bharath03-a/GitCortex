"""Cost cap enforcement tests."""

from __future__ import annotations

import unittest

from budget import MIN_RUN_USD, BudgetExceeded, RunBudget


class RunBudgetTests(unittest.TestCase):
    def test_run_cap_is_bounded_by_remaining_spend(self) -> None:
        budget = RunBudget(max_total_usd=1.0, max_run_usd=0.8)
        self.assertEqual(budget.next_run_cap(), 0.8)
        budget = budget.charge(0.7, 0.8)
        self.assertAlmostEqual(budget.next_run_cap(), 0.3)

    def test_refuses_when_remaining_below_minimum_run(self) -> None:
        budget = RunBudget(max_total_usd=1.0, max_run_usd=1.0).charge(0.97, 1.0)
        with self.assertRaises(BudgetExceeded):
            budget.next_run_cap()

    def test_refuses_when_spend_exceeds_total(self) -> None:
        budget = RunBudget(max_total_usd=1.0, max_run_usd=1.0).charge(1.2, 1.0)
        self.assertEqual(budget.remaining_usd, 0.0)
        with self.assertRaises(BudgetExceeded):
            budget.next_run_cap()

    def test_run_count_cap(self) -> None:
        budget = RunBudget(max_total_usd=100.0, max_run_usd=1.0, max_runs=2)
        budget = budget.charge(0.1, 1.0).charge(0.1, 1.0)
        with self.assertRaises(BudgetExceeded):
            budget.next_run_cap()

    def test_unknown_cost_is_charged_at_full_cap(self) -> None:
        budget = RunBudget(max_total_usd=5.0, max_run_usd=1.0).charge(None, 1.0)
        self.assertEqual(budget.spent_usd, 1.0)
        self.assertEqual(budget.estimated_runs, 1)

    def test_charge_does_not_mutate_original(self) -> None:
        original = RunBudget(max_total_usd=5.0, max_run_usd=1.0)
        original.charge(0.5, 1.0)
        self.assertEqual(original.spent_usd, 0.0)
        self.assertEqual(original.runs, 0)

    def test_invalid_limits_rejected(self) -> None:
        with self.assertRaises(ValueError):
            RunBudget(max_total_usd=0, max_run_usd=1.0)
        with self.assertRaises(ValueError):
            RunBudget(max_total_usd=1.0, max_run_usd=MIN_RUN_USD / 2)
        with self.assertRaises(ValueError):
            RunBudget(max_total_usd=1.0, max_run_usd=1.0, max_runs=0)

    def test_negative_reported_cost_never_refunds(self) -> None:
        budget = RunBudget(max_total_usd=5.0, max_run_usd=1.0).charge(-3.0, 1.0)
        self.assertEqual(budget.spent_usd, 0.0)


if __name__ == "__main__":
    unittest.main()
