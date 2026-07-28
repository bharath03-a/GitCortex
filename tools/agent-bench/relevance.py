"""Model-free relevance scoring for ranked search evidence.

The retrieval lane must be able to say whether a ranking is *better*, not merely
smaller. Payload bytes and evidence presence cannot distinguish a correct top hit
from a correct tenth hit, so ranking changes (lexical vs hybrid RRF) are invisible
to the byte-oriented gate. These metrics close that gap and stay deterministic,
so they cost nothing to run.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Sequence


@dataclass(frozen=True)
class RelevanceTruth:
    """Pinned ground truth for one search task."""

    files: tuple[str, ...]
    symbols: tuple[str, ...]


#: Cut-off for precision. Fixed by the plan's `precision@5` requirement.
PRECISION_CUTOFF = 5


@dataclass(frozen=True)
class RelevanceScore:
    """Ranking quality for one ranked evidence list."""

    mrr: float
    precision_at_5: float
    file_recall: float


def _matches_file(candidate: str, truth_path: str) -> bool:
    """Match a repo-relative path exactly, or as a whole trailing path segment.

    Suffix matching lets ground truth pin `command.go` without spelling out a
    full path, while the separator guard keeps `command.go` from matching
    `subcommand.go`.
    """
    return candidate == truth_path or candidate.endswith("/" + truth_path)


#: Qualified-name separators across the supported languages: Python/Java dots,
#: Rust paths, and the TypeScript private-member form GitCortex emits (`Hono#dispatch`).
SYMBOL_SEPARATORS = (".", "::", "#")


def _matches_symbol(candidate: str, truth_symbol: str) -> bool:
    """Match a symbol exactly, or as a whole trailing segment of a qualified name."""
    if candidate == truth_symbol:
        return True
    return any(
        candidate.endswith(separator + truth_symbol) for separator in SYMBOL_SEPARATORS
    )


def is_relevant(item: Mapping[str, Any], truth: RelevanceTruth) -> bool:
    """A hit is relevant when its file or its symbol identity is pinned."""
    if any(_matches_file(str(item.get("file", "")), path) for path in truth.files):
        return True
    names = (str(item.get("qualified_name", "")), str(item.get("symbol", "")))
    return any(
        _matches_symbol(name, symbol) for name in names for symbol in truth.symbols
    )


def score_relevance(
    evidence: Sequence[Mapping[str, Any]], truth: RelevanceTruth
) -> RelevanceScore:
    """Score one ranked evidence list against pinned ground truth."""
    flags = [is_relevant(item, truth) for item in evidence]
    mrr = 0.0
    for rank, relevant in enumerate(flags, start=1):
        if relevant:
            mrr = 1 / rank
            break
    # Divide by the fixed cut-off, not by the number of results returned, so a
    # short list cannot buy perfect precision by withholding candidates.
    precision_at_5 = sum(flags[:PRECISION_CUTOFF]) / PRECISION_CUTOFF
    # Recall scans the whole ranked list: a pinned file found late is still found.
    returned_files = [str(item.get("file", "")) for item in evidence]
    covered = sum(
        any(_matches_file(candidate, path) for candidate in returned_files)
        for path in truth.files
    )
    file_recall = covered / len(truth.files) if truth.files else 1.0
    return RelevanceScore(
        mrr=mrr, precision_at_5=precision_at_5, file_recall=file_recall
    )
