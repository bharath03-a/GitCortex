"""Grade an agent's final answer against source-grounded required evidence.

Ground truth comes from the pinned repository source, never from gcx output.
"""

from __future__ import annotations

import re
from dataclasses import asdict, dataclass

# Phrases that assert nothing depends on the target. Matches are surfaced for
# human review; the flag is a heuristic, not a verdict.
NO_DEPENDENTS_PATTERNS = (
    r"\bno\s+(?:direct\s+|other\s+|known\s+|external\s+)?(?:callers|dependents|usages|references|call sites)\b",
    r"\bnothing\s+(?:else\s+)?(?:calls|depends on|uses|references)\b",
    r"\bnot\s+(?:called|used|referenced)\s+(?:anywhere|by\s+anything)\b",
    r"\b(?:has|have)\s+no\s+(?:callers|dependents)\b",
    r"\bsafe\s+to\s+(?:change|modify|refactor|rename|remove|delete)\b",
    r"\bzero\s+(?:callers|dependents)\b",
)
_NO_DEPENDENTS = re.compile("|".join(NO_DEPENDENTS_PATTERNS), re.IGNORECASE)
_NEGATED_SAFE = re.compile(r"\b(?:not|isn't|is not|n't)\s+(?:\w+\s+){0,2}safe\s+to\b", re.IGNORECASE)


@dataclass(frozen=True)
class Grade:
    found: tuple[str, ...]
    missing: tuple[str, ...]
    forbidden_found: tuple[str, ...]
    quality_score: float
    success: bool
    claims_no_dependents: tuple[str, ...]

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


def normalize_answer(answer: str) -> str:
    """Clients escape slashes in markdown or emit absolute paths; evidence is repo-relative."""
    return answer.replace("\\/", "/").replace("\\\\", "/")


def no_dependents_claims(answer: str) -> tuple[str, ...]:
    claims = []
    for match in _NO_DEPENDENTS.finditer(answer):
        start = max(match.start() - 12, 0)
        window = answer[start : match.end()]
        if _NEGATED_SAFE.search(window):
            continue
        claims.append(match.group(0))
    return tuple(claims)


def grade_answer(
    answer: str,
    required: tuple[str, ...],
    forbidden: tuple[str, ...] = (),
    dependents_exist: bool = False,
) -> Grade:
    """Score `answer`. `success` needs every required item and no forbidden one.

    `claims_no_dependents` is only populated when dependents are known to exist
    (graph-suited tasks), so a hit there is a false-safe candidate.
    """
    text = normalize_answer(answer)
    found = tuple(item for item in required if item in text)
    missing = tuple(item for item in required if item not in text)
    forbidden_found = tuple(item for item in forbidden if item in text)
    quality = len(found) / len(required) if required else 1.0
    claims = no_dependents_claims(text) if dependents_exist else ()
    return Grade(
        found=found,
        missing=missing,
        forbidden_found=forbidden_found,
        quality_score=round(quality, 4),
        success=bool(text.strip()) and not missing and not forbidden_found,
        claims_no_dependents=claims,
    )
