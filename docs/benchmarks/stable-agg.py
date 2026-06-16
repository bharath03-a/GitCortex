#!/usr/bin/env python3
"""Aggregate the 3-round stable compact sweep.

For each (repo, question) take the MEDIAN token_ratio across rounds to kill
run-to-run noise, then report per-question and overall geomean + raw savings.
"""
import json
import math
import statistics
import sys
from pathlib import Path

REPOS = ["ripgrep", "requests", "hono", "cobra", "gson"]
QS = ["search_concept", "tour_onboarding", "refactor_impact", "subgraph_around"]
ROUNDS = [1, 2, 3]
DIR = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/benchmarks/stable")


def load(round_, repo):
    p = DIR / f"r{round_}-{repo}.json"
    if not p.exists():
        return None
    return json.loads(p.read_text())


def ok(q):
    """A usable sample: both arms succeeded with positive token totals."""
    return (
        not q["baseline"].get("error")
        and not q["gcx"].get("error")
        and q["baseline"]["total"] > 0
        and q["gcx"]["total"] > 0
    )


# per (repo, q) -> list of (baseline_total, gcx_total) across rounds.
# Errored / throttled sessions are dropped so they can't corrupt the median.
samples = {}
dropped = 0
for repo in REPOS:
    for rd in ROUNDS:
        d = load(rd, repo)
        if not d:
            continue
        for q in d.get("questions", []):
            if not ok(q):
                dropped += 1
                continue
            key = (repo, q["q"])
            samples.setdefault(key, []).append(
                (q["baseline"]["total"], q["gcx"]["total"])
            )
if dropped:
    print(f"(dropped {dropped} errored/zero sessions)\n")

# Per-question median ratio across all repos+rounds
print(f"{'question':18} {'median_ratio':>12} {'n':>4}")
q_ratios = {q: [] for q in QS}
for (repo, q), pairs in samples.items():
    for b, g in pairs:
        if g > 0:
            q_ratios.setdefault(q, []).append(b / g)
for q in QS:
    rs = q_ratios.get(q, [])
    if rs:
        print(f"{q:18} {statistics.median(rs):12.2f} {len(rs):4}")

# Aggregate: median baseline & gcx per (repo,q), then sum
agg_b = agg_g = 0
ratios = []
print(f"\n{'repo':10} {'baseline(med)':>14} {'gcx(med)':>12} {'saved%':>8}")
for repo in REPOS:
    rb = rg = 0
    for q in QS:
        pairs = samples.get((repo, q))
        if not pairs:
            continue
        mb = statistics.median([p[0] for p in pairs])
        mg = statistics.median([p[1] for p in pairs])
        rb += mb
        rg += mg
        if mg > 0:
            ratios.append(mb / mg)
    if rb:
        agg_b += rb
        agg_g += rg
        print(f"{repo:10} {rb:14,.0f} {rg:12,.0f} {(1-rg/rb)*100:7.2f}%")

raw = (1 - agg_g / agg_b) * 100 if agg_b else 0
geo = math.exp(sum(math.log(x) for x in ratios) / len(ratios)) if ratios else 0
print("-" * 48)
print(f"{'TOTAL':10} {agg_b:14,.0f} {agg_g:12,.0f} {raw:7.2f}%  geomean {geo:.2f}x")
print(f"\nPREVIOUS README (compact): 38.31% raw, geomean 1.59x")
print(f"PRIOR single run (compact): 18.84% raw, geomean 1.19x")
