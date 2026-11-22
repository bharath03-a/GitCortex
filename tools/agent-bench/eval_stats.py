"""Paired comparison statistics for the eval report. Pure functions, deterministic."""

from __future__ import annotations

import math
import random
import statistics
from collections import defaultdict
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass

from eval_load import Row

BASELINE = "baseline"
BOOTSTRAP_SAMPLES = 2000
METRICS: dict[str, Callable[[Row], float | None]] = {
    "total_tokens": lambda r: r.total_tokens,
    "uncached_tokens": lambda r: r.uncached_tokens,
    "cost_usd": lambda r: r.cost_usd,
    "wall_ms": lambda r: r.wall_ms,
    "tool_calls": lambda r: r.tool_calls,
}


@dataclass(frozen=True)
class Pair:
    trace: str
    task: str
    round: int
    ratio: float


def geomean(values: Iterable[float]) -> float | None:
    positive = [v for v in values if v > 0]
    return math.exp(sum(map(math.log, positive)) / len(positive)) if positive else None


def mean(values: Sequence[float]) -> float | None:
    return statistics.fmean(values) if values else None


def median(values: Sequence[float]) -> float | None:
    return statistics.median(values) if values else None


def stdev(values: Sequence[float]) -> float | None:
    return statistics.stdev(values) if len(values) > 1 else None


def rate(num: int, den: int) -> dict[str, float | int | None]:
    return {"num": num, "den": den, "rate": round(num / den, 4) if den else None}


def paired_ratios(rows: Sequence[Row], metric: str, arm: str, baseline: str = BASELINE) -> list[Pair]:
    """baseline/arm per (trace, task, round): above 1.0 means the arm used less."""
    read = METRICS[metric]
    by_key: dict[tuple[str, str, int], dict[str, float]] = defaultdict(dict)
    for row in rows:
        value = read(row)
        if value is not None and row.arm in {arm, baseline}:
            by_key[(row.trace, row.task, row.round)][row.arm] = float(value)
    pairs = []
    for (trace, task, round_number), arms in sorted(by_key.items()):
        if arms.get(baseline, 0) > 0 and arms.get(arm, 0) > 0:
            pairs.append(Pair(trace, task, round_number, arms[baseline] / arms[arm]))
    return pairs


def bootstrap_ci(ratios: Sequence[float], seed: int = 0) -> tuple[float, float] | None:
    """95% percentile interval of the geometric mean, resampling pairs. Needs 2+ pairs."""
    logs = [math.log(r) for r in ratios if r > 0]
    if len(logs) < 2:
        return None
    rng = random.Random(seed)
    means = sorted(
        statistics.fmean(rng.choices(logs, k=len(logs))) for _ in range(BOOTSTRAP_SAMPLES)
    )
    low, high = means[int(0.025 * BOOTSTRAP_SAMPLES)], means[int(0.975 * BOOTSTRAP_SAMPLES) - 1]
    return math.exp(low), math.exp(high)


def summarize_pairs(pairs: Sequence[Pair], seed: int = 0) -> dict[str, object]:
    """Geomean ratio, interval, spread across tasks, and variance across rounds."""
    ratios = [p.ratio for p in pairs]
    by_round: dict[int, list[float]] = defaultdict(list)
    for pair in pairs:
        by_round[pair.round].append(pair.ratio)
    round_means = {k: geomean(v) for k, v in sorted(by_round.items())}
    spread = stdev([g for g in round_means.values() if g is not None])
    ci = bootstrap_ci(ratios, seed)
    gm = geomean(ratios)
    return {
        "pairs": len(pairs),
        "geomean": round(gm, 4) if gm is not None else None,
        "ci95": [round(ci[0], 4), round(ci[1], 4)] if ci else None,
        "min": round(min(ratios), 4) if ratios else None,
        "max": round(max(ratios), 4) if ratios else None,
        "rounds": len(by_round),
        "round_geomeans": {str(k): round(g, 4) for k, g in round_means.items() if g is not None},
        "round_stdev": round(spread, 4) if spread is not None else None,
    }
