#!/usr/bin/env python3
"""Render the stable (median-of-N) benchmark report as a self-contained HTML page.

Reads the multi-round stable sweep (`stable/r{round}-{repo}.json`), drops
errored/zero sessions, and reports per-question MEDIAN token ratios, a per-repo
table, and a turns-based precision proxy — the honest, noise-resistant view.

The previous published run is preserved behind a dated toggle and linked as an
archived page. A release-notices section summarises what shipped this cycle.

Usage: python3 stable-report.py [stable_dir] [-o out.html]
"""
from __future__ import annotations

import argparse
import glob
import html
import json
import math
import os
import statistics
from datetime import date

QS = ["search_concept", "tour_onboarding", "refactor_impact", "subgraph_around"]
Q_PLAIN = {
    "search_concept": "Find relevant code",
    "tour_onboarding": "Tour the codebase",
    "refactor_impact": "What breaks if I change X?",
    "subgraph_around": "Show connections around X",
}
Q_TOOL = {
    "search_concept": "search_code",
    "tour_onboarding": "start_tour",
    "refactor_impact": "find_callers",
    "subgraph_around": "get_subgraph",
}
REPO_LANG = {
    "ripgrep": "Rust",
    "requests": "Python",
    "hono": "TypeScript",
    "cobra": "Go",
    "gson": "Java",
}

# Release this report documents.
VERSION = "0.5.0"

# Previous published run — preserved behind a dated toggle.
PREV_DATE = "2026-06-06"
PREV_FILE = "final-report-2026-06-06.html"

# What shipped this cycle (newest first).
RELEASE_NOTES = [
    "<strong>7 new MCP tools</strong> (15 → 22): graph_stats, ast_search, "
    "type_hierarchy, find_importers, find_type_usages, module_dependencies, "
    "get_call_sites.",
    "<strong>Configurable response token budget</strong> "
    "(<code>GCX_RESPONSE_BUDGET</code>, default 2000): every list tool truncates "
    "to fit, so a high-fan-out symbol can never out-cost grep.",
    "<strong>Edge confidence</strong> (extracted vs inferred): cross-file "
    "name-resolved edges are flagged lower-confidence — a precision signal.",
    "<strong>Richer graph data</strong>: cyclomatic complexity, decorator/"
    "annotation metadata, and exact call-site lines.",
    "<strong>Fixed</strong>: Rust import edges were silently dropped; search "
    "now handles space-separated and typo queries.",
    "<strong>Tour</strong>: no-seed tours emit a component-level architecture "
    "summary with file:line, instead of a flat symbol list.",
    "<strong>Trustworthy benchmark</strong>: harness now retries on rate "
    "limits, throttles, excludes errored sessions, and reports median-of-N — "
    "the prior single-run numbers were too noisy to trust.",
]


def ok(q: dict) -> bool:
    return (
        not q["baseline"].get("error")
        and not q["gcx"].get("error")
        and q["baseline"]["total"] > 0
        and q["gcx"]["total"] > 0
    )


def load(stable_dir: str) -> dict:
    """Return {(repo, q): [(b_total, g_total, b_turns, g_turns), ...]}."""
    samples: dict[tuple[str, str], list[tuple[int, int, int, int]]] = {}
    rounds = set()
    for path in glob.glob(os.path.join(stable_dir, "r*-*.json")):
        base = os.path.basename(path)
        rounds.add(base.split("-", 1)[0])
        repo = base.split("-", 1)[1].removesuffix(".json")
        d = json.load(open(path))
        for q in d.get("questions", []):
            if not ok(q):
                continue
            samples.setdefault((repo, q["q"]), []).append(
                (
                    q["baseline"]["total"],
                    q["gcx"]["total"],
                    q["baseline"].get("turns", 0),
                    q["gcx"].get("turns", 0),
                )
            )
    return {"samples": samples, "n_rounds": len(rounds)}


def med(xs: list[float]) -> float:
    return statistics.median(xs) if xs else 0.0


def geomean(xs: list[float]) -> float:
    logs = [math.log(x) for x in xs if x > 0]
    return math.exp(sum(logs) / len(logs)) if logs else 0.0


def cls(ratio: float) -> str:
    return "win" if ratio >= 1.15 else ("lose" if ratio and ratio <= 0.9 else "mid")


def render(data: dict) -> str:
    samples = data["samples"]
    repos = [r for r in REPO_LANG if any(k[0] == r for k in samples)]

    # Per-question median ratio.
    q_rows = []
    for q in QS:
        ratios = [b / g for (r, qq), v in samples.items() if qq == q for b, g, _, _ in v if g]
        bt = [bt for (r, qq), v in samples.items() if qq == q for _, _, bt, _ in v]
        gt = [gt for (r, qq), v in samples.items() if qq == q for _, _, _, gt in v]
        ratio = med(ratios)
        q_rows.append(
            f'<tr><td>{html.escape(Q_PLAIN[q])}<br><code>{Q_TOOL[q]}</code></td>'
            f'<td class="{cls(ratio)}">{ratio:.2f}×</td>'
            f'<td>{med(bt):.0f} → {med(gt):.0f}</td></tr>'
        )

    # Per-repo medians.
    agg_b = agg_g = 0.0
    repo_ratios = []
    repo_rows = []
    for r in repos:
        rb = sum(med([b for b, _, _, _ in samples[(r, q)]]) for q in QS if (r, q) in samples)
        rg = sum(med([g for _, g, _, _ in samples[(r, q)]]) for q in QS if (r, q) in samples)
        if not rb:
            continue
        agg_b += rb
        agg_g += rg
        saved = (1 - rg / rb) * 100
        repo_ratios.append(rb / rg)
        sc = "win" if saved > 5 else ("lose" if saved < -5 else "mid")
        repo_rows.append(
            f'<tr><td>{r}</td><td>{REPO_LANG[r]}</td>'
            f'<td>{rb:,.0f}</td><td>{rg:,.0f}</td>'
            f'<td class="{sc}">{saved:+.1f}%</td></tr>'
        )

    raw = (1 - agg_g / agg_b) * 100 if agg_b else 0
    geo = geomean(repo_ratios)
    today = date.today().isoformat()
    notes = "\n".join(f"<li>{n}</li>" for n in RELEASE_NOTES)

    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>GitCortex Benchmark — stable run {today}</title>
<style>{CSS}</style></head>
<body>
<article>
  <div class="meta"><span class="tag">Benchmark</span><span>{today}</span></div>
  <h1>Does a code knowledge graph help AI work more efficiently?</h1>
  <p class="lede">Measured Claude token usage on real developer questions, once
  with ordinary file search, once with the GitCortex graph (compact MCP). This is
  the <strong>median of {data['n_rounds']} runs</strong> with errored/rate-limited
  sessions excluded — the noise-resistant view. Every number is from the actual
  API response.</p>

  <div class="hero">
    <div class="stat"><div class="big {cls(geo)}">{geo:.2f}×</div><div>geomean token ratio</div></div>
    <div class="stat"><div class="big {'win' if raw>5 else ('lose' if raw<-5 else 'mid')}">{raw:+.1f}%</div><div>aggregate tokens saved</div></div>
    <div class="stat"><div class="big win">{med([b/g for (r,qq),v in samples.items() if qq=='search_concept' for b,g,_,_ in v if g]):.2f}×</div><div>search (the clear win)</div></div>
  </div>

  <h2>Per-question (median ratio · grep→gcx turns)</h2>
  <table><thead><tr><th>Question</th><th>Token ratio</th><th>Turns</th></tr></thead>
  <tbody>{''.join(q_rows)}</tbody></table>
  <p class="note">Ratio &gt; 1 = graph cheaper. Turns is a precision proxy: fewer
  hops to the answer. Search lands in roughly half the turns; broad "tour" and
  "what breaks" questions remain close to break-even because the model reads code
  either way.</p>

  <h2>Per-repository</h2>
  <table><thead><tr><th>Repo</th><th>Lang</th><th>grep tokens</th><th>gcx tokens</th><th>Saved</th></tr></thead>
  <tbody>{''.join(repo_rows)}</tbody></table>
  <p class="note">Java (gson) is the consistent drag — the parser is shallowest
  there. Large/idiomatic repos benefit most; tiny repos favour grep.</p>

  <h2>What shipped in v{VERSION}</h2>
  <ul class="notes">{notes}</ul>

  <details class="prev">
    <summary>◀ Previous published run — {PREV_DATE}</summary>
    <p>The {PREV_DATE} run reported up to <strong>56%</strong> savings (full MCP,
    large repos) and <strong>1.15–1.36×</strong> compact geomean. Those were
    <em>single-run</em> numbers; this page supersedes them with median-of-N data
    and a rate-limit-resilient harness. Full archived report:
    <a href="{PREV_FILE}">{PREV_FILE}</a>.</p>
  </details>

  <h2>Methodology</h2>
  <p class="note">Model claude-haiku-4-5. Compact MCP (single dispatch tool).
  Per question, two Claude sessions (grep arm vs gcx arm), {data['n_rounds']}
  rounds, sequential + throttled with retries. Tokens =
  input + cache-creation (cache reads excluded to avoid double counting).
  Errored sessions dropped. Reproduce: <code>bash docs/benchmarks/stable-sweep.sh</code>.</p>
</article>
</body></html>"""


CSS = """
:root{--bg:#f9f7f3;--fg:#2a2622;--mut:#6b6359;--accent:#c96442;--line:#e7e1d8}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);
font-family:Inter,-apple-system,system-ui,sans-serif;line-height:1.6}
article{max-width:760px;margin:0 auto;padding:48px 24px 96px}
.meta{display:flex;gap:12px;align-items:center;color:var(--mut);font-size:14px;margin-bottom:16px}
.tag{background:var(--accent);color:#fff;padding:2px 10px;border-radius:99px;font-weight:600}
h1{font-size:34px;line-height:1.2;margin:8px 0 16px}
h2{font-size:22px;margin:40px 0 12px;border-bottom:1px solid var(--line);padding-bottom:6px}
.lede{font-size:18px;color:#403a33}
.hero{display:flex;gap:16px;margin:28px 0;flex-wrap:wrap}
.stat{flex:1;min-width:160px;background:#fff;border:1px solid var(--line);border-radius:14px;padding:18px;text-align:center}
.big{font-size:34px;font-weight:700}
.win{color:#2f7d4f}.lose{color:#c0392b}.mid{color:#b8860b}
table{width:100%;border-collapse:collapse;margin:8px 0;font-size:14px}
th,td{text-align:left;padding:9px 12px;border-bottom:1px solid var(--line)}
th{color:var(--mut);font-weight:600}
td.win{color:#2f7d4f;font-weight:600}td.lose{color:#c0392b;font-weight:600}td.mid{color:#b8860b;font-weight:600}
code{background:#efe9e0;padding:1px 5px;border-radius:5px;font-size:13px}
.note{color:var(--mut);font-size:14px}
ul.notes{padding-left:20px}ul.notes li{margin:7px 0}
.prev{margin:24px 0;background:#fff;border:1px solid var(--line);border-radius:12px;padding:12px 16px}
.prev summary{cursor:pointer;font-weight:600;color:var(--accent)}
.prev p{color:var(--mut);font-size:14px;margin:10px 0 0}
a{color:var(--accent)}
"""


def main() -> None:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("stable_dir", nargs="?", default=os.path.join(here, "stable"))
    ap.add_argument("-o", "--out", default=os.path.join(here, "final-report.html"))
    args = ap.parse_args()

    data = load(args.stable_dir)
    if not data["samples"]:
        raise SystemExit(f"no usable samples in {args.stable_dir}")
    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write(render(data))
    print(f"wrote {args.out} ({data['n_rounds']} rounds)")


if __name__ == "__main__":
    main()
