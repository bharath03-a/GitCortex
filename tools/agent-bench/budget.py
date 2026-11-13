"""Hard spend and run caps for paid agent lanes.

The ledger is immutable: `charge` returns a new ledger, so a runner cannot
forget to thread the updated state through.
"""

from __future__ import annotations

from dataclasses import dataclass, replace

MIN_RUN_USD = 0.05


class BudgetExceeded(RuntimeError):
    """Raised before a run starts when the ledger cannot afford another run."""


@dataclass(frozen=True)
class RunBudget:
    max_total_usd: float
    max_run_usd: float
    max_runs: int | None = None
    spent_usd: float = 0.0
    runs: int = 0
    estimated_runs: int = 0

    def __post_init__(self) -> None:
        if self.max_total_usd <= 0:
            raise ValueError("max_total_usd must be positive")
        if self.max_run_usd < MIN_RUN_USD:
            raise ValueError(f"max_run_usd must be at least {MIN_RUN_USD}")
        if self.max_runs is not None and self.max_runs <= 0:
            raise ValueError("max_runs must be positive when set")

    @property
    def remaining_usd(self) -> float:
        return max(self.max_total_usd - self.spent_usd, 0.0)

    def next_run_cap(self) -> float:
        """Cap to hand the client for the next run, or raise if none is affordable."""
        if self.max_runs is not None and self.runs >= self.max_runs:
            raise BudgetExceeded(f"run cap reached ({self.runs}/{self.max_runs})")
        cap = min(self.max_run_usd, self.remaining_usd)
        if cap < MIN_RUN_USD:
            raise BudgetExceeded(
                f"spend cap reached (${self.spent_usd:.4f} of ${self.max_total_usd:.2f})"
            )
        return round(cap, 4)

    def charge(self, cost_usd: float | None, run_cap_usd: float) -> RunBudget:
        """Record a finished run.

        A run whose client reported no cost (crash, timeout, killed) is charged
        the full cap it was given: the worst case it could have spent.
        """
        estimated = cost_usd is None
        amount = run_cap_usd if cost_usd is None else max(cost_usd, 0.0)
        return replace(
            self,
            spent_usd=round(self.spent_usd + amount, 6),
            runs=self.runs + 1,
            estimated_runs=self.estimated_runs + int(estimated),
        )

    def snapshot(self) -> dict[str, float | int | None]:
        return {
            "max_total_usd": self.max_total_usd,
            "max_run_usd": self.max_run_usd,
            "max_runs": self.max_runs,
            "spent_usd": self.spent_usd,
            "runs": self.runs,
            "estimated_runs": self.estimated_runs,
        }
