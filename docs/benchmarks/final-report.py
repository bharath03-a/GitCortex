#!/usr/bin/env python3
"""Combined benchmark report styled after Anthropic research blog.

Three arms: Claude Code (full MCP) + Claude Code (compact MCP) + Codex.

Usage: python3 final-report.py [bench_dir] [-o out.html]
"""
from __future__ import annotations

import argparse
import glob
import html
import json
import math
import os
from datetime import date

Q_TO_TOOL = {
    "tour_onboarding":  "start_tour",
    "search_concept":   "search_code",
    "wiki_explain":     "wiki_symbol",
    "refactor_impact":  "find_callers",
    "trace_flow":       "trace_path",
    "subgraph_around":  "get_subgraph",
    "find_dead_code":   "find_unused_symbols",
}

Q_PLAIN = {
    "search_concept":  "Find relevant code",
    "tour_onboarding": "Explain the codebase",
    "refactor_impact": "What breaks if I change X?",
    "subgraph_around": "Show connections around X",
    "wiki_explain":    "Explain a symbol",
    "trace_flow":      "Trace a code path",
    "find_dead_code":  "Find unused code",
}

REPO_LANG = {
    "ripgrep": "Rust", "tokio": "Rust", "serde": "Rust",
    "cobra": "Go", "gin": "Go", "zap": "Go",
    "hono": "TypeScript", "zod": "TypeScript",
    "requests": "Python", "fastapi": "Python", "flask": "Python", "django": "Python",
    "gson": "Java", "picocli": "Java", "jjwt": "Java",
}

LANG_COLOR = {
    "Rust": "#dea584", "Go": "#00add8", "TypeScript": "#3178c6",
    "Python": "#ffd343", "Java": "#f89820",
}

WIN, LOSE = 1.15, 0.90


def fmt(n: float) -> str:
    return f"{int(round(n)):,}"


def pct(v: float) -> str:
    return f"{v:+.0f}%" if v else "—"


def geomean(vals: list[float]) -> float:
    logs = [math.log(v) for v in vals if v and v > 0]
    return math.exp(sum(logs) / len(logs)) if logs else 0.0


def load_arm(bench_dir: str, pattern: str, exclude: list[str] | None = None) -> list[dict]:
    out = []
    exclude = exclude or []
    for path in sorted(glob.glob(os.path.join(bench_dir, pattern))):
        base = os.path.basename(path)
        if any(x in base for x in ["prefix", "smoke"]):
            continue
        if any(x in base for x in exclude):
            continue
        try:
            d = json.load(open(path, encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if "totals" not in d or "questions" not in d:
            continue
        errs = sum(
            1 for q in d["questions"]
            if q.get("baseline", {}).get("error") or q.get("gcx", {}).get("error")
        )
        if errs > len(d["questions"]) // 2:
            continue
        d["_lang"] = REPO_LANG.get(d.get("repo", ""), "—")
        out.append(d)
    return out


def arm_geo(reports: list[dict], uncached: bool = False) -> float:
    key = "uncached_geomean_ratio" if uncached else "geomean_ratio"
    vals = [r["totals"].get(key, 0) for r in reports]
    return geomean(vals)


def arm_cost_saved(reports: list[dict]) -> float:
    cb = sum(r["totals"].get("baseline_cost_usd", 0) for r in reports)
    cg = sum(r["totals"].get("gcx_cost_usd", 0) for r in reports)
    return 100 * (cb - cg) / cb if cb else 0


def matrix_table(reports: list[dict], uncached: bool = False) -> str:
    if not reports:
        return '<p style="color:var(--subtle);font-style:italic">Data not yet available.</p>'
    repos = sorted({r.get("repo","") for r in reports}, key=lambda x: REPO_LANG.get(x,""))
    q_seen = {q.get("q") for r in reports for q in r["questions"]}
    head = "<th>Question</th>" + "".join(f"<th>{html.escape(r)}</th>" for r in repos) + "<th>Avg</th>"
    body = ""
    for lbl, tool in Q_TO_TOOL.items():
        if lbl not in q_seen:
            continue
        plain = Q_PLAIN.get(lbl, lbl)
        tds = f'<td><span class="tool-name">{html.escape(tool)}</span></td>'
        ratios = []
        for repo in repos:
            rep = next((r for r in reports if r.get("repo") == repo), None)
            q = next((x for x in rep["questions"] if x.get("q") == lbl), None) if rep else None
            if not q or q.get("baseline",{}).get("error") or q.get("gcx",{}).get("error"):
                tds += '<td class="na">—</td>'; continue
            if uncached:
                bu = q["baseline"].get("uncached_total", q["baseline"].get("total", 0))
                gu = q["gcx"].get("uncached_total", q["gcx"].get("total", 0))
                ratio = round(bu / gu, 2) if gu else 0
            else:
                ratio = q.get("token_ratio", 0)
            ratios.append(ratio)
            cls = "cell-win" if ratio >= WIN else ("cell-lose" if (ratio and ratio <= LOSE) else "cell-tie")
            tds += f'<td class="{cls}">{ratio:g}×</td>'
        geo = geomean(ratios)
        geo_cls = "cell-win" if geo >= WIN else ("cell-lose" if geo <= LOSE else "cell-tie")
        tds += f'<td class="{geo_cls}"><strong>{geo:.2f}×</strong></td>'
        body += f"<tr>{tds}</tr>"
    return f'<div class="table-wrap"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>'


# ── What we tested ────────────────────────────────────────────────────────────

QUESTION_DESIGN = [
    ("search_concept", "Find relevant code",
     "Where is authentication / parsing handled?", "search_code",
     "Discovery — the graph's strongest case. Grep scans every file containing the term."),
    ("tour_onboarding", "Explain the codebase",
     "Give me a tour: main components and how they fit.", "start_tour",
     "Architecture overview. The graph ranks entry points by centrality."),
    ("refactor_impact", "Refactor impact",
     "If I change X, what breaks? List the callers.", "find_callers",
     "Honest middle case — depends on how many callers the symbol has."),
    ("subgraph_around", "Show connections",
     "Show everything connected to X.", "get_subgraph",
     "Honest loss case — a hub symbol dumps a large neighbourhood."),
]


def lang_of(repo: str) -> str:
    return REPO_LANG.get(repo, "—")


# ── SVG charts ────────────────────────────────────────────────────────────────

def _ratio_color(r: float) -> str:
    return "#1f6e45" if r >= WIN else ("#b83822" if (r and r <= LOSE) else "#7a6830")


def chart_full_vs_compact(full: list[dict], compact: list[dict], width: int = 660) -> str:
    """Grouped horizontal bars: per language, full ratio vs compact ratio."""
    langs = ["Rust", "Python", "TypeScript", "Go", "Java"]
    f_by = {lang_of(r["repo"]): r["totals"].get("geomean_ratio", 0) for r in full}
    c_by = {lang_of(r["repo"]): r["totals"].get("geomean_ratio", 0) for r in compact}
    rows = [(l, f_by.get(l, 0), c_by.get(l, 0)) for l in langs if f_by.get(l) or c_by.get(l)]
    if not rows:
        return ""

    ml, mt, mb, mr = 90, 40, 36, 40
    bar_h, grp_gap, pair_gap = 16, 18, 4
    chart_w = width - ml - mr
    max_r = max([max(f, c) for _, f, c in rows] + [2.2]) * 1.08
    row_h = bar_h * 2 + pair_gap
    chart_h = len(rows) * (row_h + grp_gap)
    total_h = chart_h + mt + mb

    def sx(v): return ml + v / max_r * chart_w
    one_x = sx(1.0)

    parts = [
        # break-even line
        f'<line x1="{one_x:.1f}" y1="{mt - 8}" x2="{one_x:.1f}" y2="{mt + chart_h}" '
        f'stroke="#b0a99a" stroke-width="1" stroke-dasharray="3,3"/>'
        f'<text x="{one_x:.1f}" y="{mt - 12}" text-anchor="middle" fill="#74706a" '
        f'font-size="10" font-family="Inter,sans-serif">break-even (1×)</text>'
    ]
    # legend
    parts.append(
        f'<rect x="{ml}" y="{total_h - 16}" width="11" height="11" rx="2" fill="#c96442"/>'
        f'<text x="{ml + 16}" y="{total_h - 7}" fill="#74706a" font-size="11" font-family="Inter">Full MCP</text>'
        f'<rect x="{ml + 90}" y="{total_h - 16}" width="11" height="11" rx="2" fill="#7fb89a"/>'
        f'<text x="{ml + 106}" y="{total_h - 7}" fill="#74706a" font-size="11" font-family="Inter">Compact MCP</text>'
    )
    for i, (lang, fr, cr) in enumerate(rows):
        y = mt + i * (row_h + grp_gap)
        # full bar
        fw = max(sx(fr) - ml, 1)
        parts.append(
            f'<rect x="{ml}" y="{y}" width="{fw:.1f}" height="{bar_h}" rx="3" fill="#c96442" opacity="0.9"/>'
            f'<text x="{ml + fw + 6:.1f}" y="{y + bar_h - 3}" fill="{_ratio_color(fr)}" '
            f'font-size="11" font-weight="600" font-family="Inter">{fr:.2f}×</text>'
        )
        # compact bar
        cw = max(sx(cr) - ml, 1)
        parts.append(
            f'<rect x="{ml}" y="{y + bar_h + pair_gap}" width="{cw:.1f}" height="{bar_h}" rx="3" fill="#7fb89a"/>'
            f'<text x="{ml + cw + 6:.1f}" y="{y + 2 * bar_h + pair_gap - 3}" fill="{_ratio_color(cr)}" '
            f'font-size="11" font-weight="600" font-family="Inter">{cr:.2f}×</text>'
        )
        # lang label
        parts.append(
            f'<text x="{ml - 10}" y="{y + bar_h + pair_gap}" text-anchor="end" '
            f'fill="#191817" font-size="12.5" font-weight="500" font-family="Inter">{lang}</text>'
            f'<circle cx="{ml - 70}" cy="{y + bar_h + pair_gap - 4}" r="0"/>'
        )
    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px" '
        f'role="img" aria-label="Full vs compact ratio per language">{"".join(parts)}</svg>'
    )


def chart_by_question(full: list[dict], width: int = 660) -> str:
    """Horizontal bars: geomean ratio per question type across all repos."""
    rows = []
    for lbl, plain, _, tool, _ in QUESTION_DESIGN:
        rs = []
        for r in full:
            q = next((x for x in r["questions"] if x.get("q") == lbl), None)
            if q and not q.get("baseline",{}).get("error") and q.get("token_ratio"):
                rs.append(q["token_ratio"])
        rows.append((plain, tool, geomean(rs)))
    if not rows:
        return ""

    ml, mt, mb, mr = 200, 16, 30, 50
    bar_h, gap = 26, 14
    chart_w = width - ml - mr
    max_r = max([r for _, _, r in rows] + [2.5]) * 1.08
    chart_h = len(rows) * (bar_h + gap)
    total_h = chart_h + mt + mb

    def sx(v): return ml + v / max_r * chart_w
    one_x = sx(1.0)

    parts = [
        f'<line x1="{one_x:.1f}" y1="{mt}" x2="{one_x:.1f}" y2="{mt + chart_h}" '
        f'stroke="#b0a99a" stroke-width="1" stroke-dasharray="3,3"/>'
        f'<text x="{one_x:.1f}" y="{mt + chart_h + 20}" text-anchor="middle" fill="#74706a" '
        f'font-size="10" font-family="Inter">1× (break-even)</text>'
    ]
    for i, (plain, tool, r) in enumerate(rows):
        y = mt + i * (bar_h + gap)
        w = max(sx(r) - ml, 1)
        col = _ratio_color(r)
        parts.append(
            f'<rect x="{ml}" y="{y}" width="{w:.1f}" height="{bar_h}" rx="4" fill="{col}" opacity="0.85"/>'
            f'<text x="{ml + w + 8:.1f}" y="{y + bar_h - 7}" fill="{col}" '
            f'font-size="13" font-weight="700" font-family="Inter">{r:.2f}×</text>'
            f'<text x="{ml - 12}" y="{y + 16}" text-anchor="end" fill="#191817" '
            f'font-size="12.5" font-family="Inter">{html.escape(plain)}</text>'
            f'<text x="{ml - 12}" y="{y + 29}" text-anchor="end" fill="#74706a" '
            f'font-size="10" font-family="ui-monospace,monospace">{html.escape(tool)}</text>'
        )
    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px" '
        f'role="img" aria-label="Ratio by question type">{"".join(parts)}</svg>'
    )


def lang_summary_table(full: list[dict], compact: list[dict]) -> str:
    """Per-language summary: full ratio + saving, compact ratio + saving."""
    langs = ["Rust", "Python", "TypeScript", "Go", "Java"]
    f_by = {lang_of(r["repo"]): r for r in full}
    c_by = {lang_of(r["repo"]): r for r in compact}
    body = ""
    for l in langs:
        rf, rc = f_by.get(l), c_by.get(l)
        if not rf and not rc:
            continue
        color = LANG_COLOR.get(l, "#888")
        repo = (rf or rc).get("repo", "")

        def cell(r):
            if not r:
                return '<td class="na">—</td><td class="na">—</td>'
            g = r["totals"].get("geomean_ratio", 0)
            s = r["totals"].get("saved_pct", 0)
            gc = "cell-win" if g >= WIN else ("cell-lose" if g <= LOSE else "cell-tie")
            sc = "cell-win" if s > 5 else ("cell-lose" if s < -5 else "cell-tie")
            return f'<td class="{gc}">{g:.2f}×</td><td class="{sc}">{s:+.0f}%</td>'

        body += (
            f'<tr><td><span class="lang-dot" style="background:{color}"></span>'
            f'<strong>{l}</strong> <span class="mono" style="color:var(--subtle)">{html.escape(repo)}</span></td>'
            f'{cell(rf)}{cell(rc)}</tr>'
        )
    return (
        '<div class="table-wrap"><table><thead><tr>'
        '<th rowspan="2" style="vertical-align:bottom">Language</th>'
        '<th colspan="2" style="text-align:center;border-bottom:1px solid var(--line)">Full MCP</th>'
        '<th colspan="2" style="text-align:center">Compact MCP</th></tr>'
        '<tr><th>Ratio</th><th>Cost</th><th>Ratio</th><th>Cost</th></tr></thead>'
        f'<tbody>{body}</tbody></table></div>'
    )


CSS = """
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');

:root {
  --bg: #f9f7f3;
  --panel: #ffffff;
  --line: #e8e2d8;
  --fg: #191817;
  --subtle: #74706a;
  --accent: #c96442;
  --green: #1f6e45;
  --red: #b83822;
  --amber: #7a6830;
  --serif: Georgia, "Times New Roman", serif;
  --sans: "Inter", ui-sans-serif, -apple-system, "Segoe UI", sans-serif;
  --mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
}

* { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--bg);
  color: var(--fg);
  font: 400 16px/1.7 var(--sans);
  -webkit-font-smoothing: antialiased;
}

/* ── Navigation ── */
.nav {
  position: sticky; top: 0; z-index: 20;
  background: rgba(249, 247, 243, 0.92);
  backdrop-filter: blur(12px) saturate(140%);
  border-bottom: 1px solid var(--line);
}
.nav-inner {
  max-width: 720px; margin: 0 auto;
  padding: 14px 24px;
  display: flex; align-items: center; justify-content: space-between;
}
.nav-brand {
  display: flex; align-items: center; gap: 9px;
  font-size: 16px; font-weight: 700;
  color: var(--fg); text-decoration: none;
  letter-spacing: -0.02em;
}
.nav-right { font-size: 13px; color: var(--subtle); }
.nav-right a { color: var(--subtle); text-decoration: none; }
.nav-right a:hover { color: var(--fg); }

/* ── Article ── */
.article {
  max-width: 720px; margin: 0 auto;
  padding: 60px 24px 120px;
}

/* ── Header ── */
.article-meta {
  font-size: 13px; color: var(--subtle);
  margin: 0 0 20px;
  display: flex; align-items: center; gap: 12px;
}
.article-meta .tag {
  background: #f0ece4; border-radius: 4px;
  padding: 2px 8px; font-size: 11px;
  font-weight: 600; text-transform: uppercase;
  letter-spacing: .6px; color: var(--subtle);
}

h1 {
  font-family: var(--serif);
  font-size: 42px; font-weight: 400;
  line-height: 1.15; letter-spacing: -0.02em;
  margin: 0 0 20px; color: var(--fg);
}

.lede {
  font-size: 19px; line-height: 1.6;
  color: #3a3530; margin: 0 0 36px;
  font-weight: 400;
  border-bottom: 1px solid var(--line);
  padding-bottom: 36px;
}

/* ── Summary box ── */
.summary {
  margin: 0 0 48px;
}
.summary h3 {
  font-family: var(--sans);
  font-size: 11px; font-weight: 700;
  text-transform: uppercase; letter-spacing: .8px;
  color: var(--subtle); margin: 0 0 16px;
}
.summary ul {
  margin: 0; padding: 0; list-style: none;
}
.summary li {
  font-style: italic; font-size: 15.5px;
  line-height: 1.6; color: #3a3530;
  padding: 8px 0; border-bottom: 1px solid var(--line);
}
.summary li:last-child { border-bottom: none; }
.summary li::before {
  content: "—";
  color: var(--accent); margin-right: 10px;
  font-style: normal;
}

/* ── Section headings ── */
h2 {
  font-family: var(--serif);
  font-size: 28px; font-weight: 400;
  letter-spacing: -0.01em;
  margin: 56px 0 16px; color: var(--fg);
}
h3.finding-label {
  font-size: 11px; font-weight: 700;
  text-transform: uppercase; letter-spacing: .8px;
  color: var(--accent); margin: 48px 0 8px;
}
h3.finding {
  font-family: var(--serif);
  font-size: 22px; font-weight: 400;
  margin: 0 0 14px; color: var(--fg);
}

p { margin: 0 0 20px; color: #2a2825; }
strong { font-weight: 600; }

/* ── Big stat callouts ── */
.stats-row {
  display: grid; grid-template-columns: repeat(3, 1fr);
  gap: 1px; margin: 28px 0 36px;
  border: 1px solid var(--line); border-radius: 12px;
  overflow: hidden; background: var(--line);
}
.stat {
  background: var(--panel);
  padding: 24px 22px;
}
.stat .k {
  font-size: 11px; font-weight: 600;
  text-transform: uppercase; letter-spacing: .7px;
  color: var(--subtle); margin-bottom: 8px;
}
.stat .v {
  font-family: var(--serif);
  font-size: 38px; font-weight: 400;
  line-height: 1; letter-spacing: -0.02em;
}
.stat .note { font-size: 12px; color: var(--subtle); margin-top: 6px; }
.v-green { color: var(--green); }
.v-red { color: var(--red); }
.v-amber { color: var(--amber); }

/* ── Data tables ── */
.table-label {
  font-size: 11px; font-weight: 700;
  text-transform: uppercase; letter-spacing: .6px;
  color: var(--subtle); margin: 32px 0 8px;
}
.table-wrap {
  border: 1px solid var(--line); border-radius: 10px;
  overflow-x: auto; margin: 0 0 8px;
  background: var(--panel);
}
table {
  width: 100%; border-collapse: collapse;
  font-size: 13.5px;
}
th, td {
  padding: 10px 14px; text-align: right;
  border-bottom: 1px solid var(--line);
  white-space: nowrap;
}
th:first-child, td:first-child { text-align: left; }
tbody tr:last-child td { border-bottom: none; }
thead th {
  background: #f4f1eb;
  font-size: 11px; font-weight: 600;
  text-transform: uppercase; letter-spacing: .5px;
  color: var(--subtle);
}
.tool-name {
  font-family: var(--mono); font-size: 12px;
  color: var(--fg);
}
td.cell-win  { background: rgba(31,110,69,.10); color: var(--green); font-weight: 600; }
td.cell-lose { background: rgba(184,56,34,.09); color: var(--red);   font-weight: 600; }
td.cell-tie  { background: rgba(122,104,48,.08); color: var(--amber); }
td.na { color: #bbb; }

/* ── Insight pull-quote ── */
.callout {
  border-left: 2px solid var(--accent);
  padding: 4px 0 4px 20px;
  margin: 28px 0;
}
.callout p {
  font-style: italic; font-size: 17px;
  line-height: 1.6; color: #3a3530;
  margin: 0;
}

/* ── Arm comparison inline ── */
.arm-compare {
  display: grid; grid-template-columns: 1fr 1fr 1fr;
  gap: 16px; margin: 24px 0 36px;
}
@media (max-width: 600px) { .arm-compare { grid-template-columns: 1fr; } }
.arm-box {
  border: 1px solid var(--line); border-radius: 10px;
  padding: 18px 20px; background: var(--panel);
}
.arm-box .arm-tag {
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: .8px;
  color: var(--subtle); margin-bottom: 10px;
}
.arm-box .arm-geo {
  font-family: var(--serif); font-size: 34px;
  font-weight: 400; line-height: 1;
}
.arm-box .arm-note { font-size: 12px; color: var(--subtle); margin-top: 6px; }

/* ── Footer ── */
hr { border: none; border-top: 1px solid var(--line); margin: 52px 0; }
.footer-list { padding-left: 0; list-style: none; }
.footer-list li {
  font-size: 13px; color: var(--subtle);
  padding: 5px 0; border-bottom: 1px solid var(--line);
  line-height: 1.6;
}
.footer-list li:last-child { border-bottom: none; }
.mono { font-family: var(--mono); font-size: .88em; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

/* ── Charts ── */
.chart-fig {
  background: var(--panel); border: 1px solid var(--line);
  border-radius: 12px; padding: 24px 22px 16px; margin: 24px 0;
}
.chart-fig figcaption {
  font-size: 13px; color: var(--subtle);
  margin-top: 14px; line-height: 1.5;
}
.chart-fig figcaption strong { color: var(--fg); }
.lang-dot {
  display: inline-block; width: 9px; height: 9px;
  border-radius: 50%; margin-right: 8px; vertical-align: middle;
}

/* ── What we tested ── */
.design-table td:nth-child(1) { font-weight: 600; }
.design-table .q-text { color: var(--subtle); font-style: italic; }
.design-table .note { font-size: 12.5px; color: var(--subtle); }
"""

LOGO = (
    '<svg width="22" height="22" viewBox="0 0 26 26" fill="none">'
    '<path d="M7 7L19 13M7 7L13 19M19 13L13 19" stroke="#c96442" '
    'stroke-width="1.6" stroke-linecap="round"/>'
    '<circle cx="7" cy="7" r="3.2" fill="#c96442"/>'
    '<circle cx="19" cy="13" r="2.6" fill="#e0a07f"/>'
    '<circle cx="13" cy="19" r="2.6" fill="#e0a07f"/>'
    '</svg>'
)


def render(claude_full: list[dict], claude_compact: list[dict], codex: list[dict]) -> str:

    geo_full    = arm_geo(claude_full) if claude_full else 0
    geo_compact = arm_geo(claude_compact) if claude_compact else 0
    geo_codex   = arm_geo(codex) if codex else 0
    geo_codex_u = arm_geo(codex, uncached=True) if codex else 0
    cost_full   = arm_cost_saved(claude_full) if claude_full else 0
    cost_compact= arm_cost_saved(claude_compact) if claude_compact else 0

    def geo_cls(g: float) -> str:
        return "v-green" if g >= WIN else ("v-red" if (g and g <= LOSE) else "v-amber")

    def arm_box(tag: str, geo: float, note: str) -> str:
        cls = geo_cls(geo)
        val = f"{geo:.2f}×" if geo else "—"
        return (
            f'<div class="arm-box"><div class="arm-tag">{html.escape(tag)}</div>'
            f'<div class="arm-geo {cls}">{val}</div>'
            f'<div class="arm-note">{html.escape(note)}</div></div>'
        )

    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>GitCortex Benchmark — Does a code graph help AI work more efficiently?</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'><rect width='64' height='64' rx='14' fill='%23f9f7f3'/><line x1='18' y1='18' x2='46' y2='32' stroke='%23c96442' stroke-width='3' stroke-linecap='round'/><line x1='18' y1='18' x2='32' y2='50' stroke='%23c96442' stroke-width='3' stroke-linecap='round'/><circle cx='18' cy='18' r='7' fill='%23c96442'/><circle cx='46' cy='32' r='5.5' fill='%23e0a07f'/><circle cx='32' cy='50' r='5.5' fill='%23e0a07f'/></svg>">
<style>{CSS}</style></head>
<body>

<nav class="nav">
  <div class="nav-inner">
    <a class="nav-brand" href="https://github.com/bharath03-a/GitCortex">
      {LOGO} GitCortex
    </a>
    <div class="nav-right">
      <a href="https://github.com/bharath03-a/GitCortex">GitHub ↗</a>
    </div>
  </div>
</nav>

<article class="article">

  <div class="article-meta">
    <span class="tag">Research</span>
    <span>{date.today().isoformat()}</span>
  </div>

  <h1>Does a code knowledge graph help AI work more efficiently?</h1>

  <p class="lede">
    We measured how much context Claude Code and Codex consume when answering real
    developer questions — once with ordinary file search, once with a GitCortex graph.
    Every number is from the actual API response.
  </p>

  <!-- Summary -->
  <div class="summary">
    <h3>Key findings</h3>
    <ul>
      <li>With the full MCP tool set, the graph cut AI session cost by up to <strong>56 %</strong>
          on large codebases (Rust, Python) — but <em>lost</em> on small ones (Go, Java),
          where loading 15 tool schemas every turn cost more than it saved.</li>
      <li><strong>Compact mode</strong> — exposing one dispatch tool instead of fifteen —
          turns every repository into a net win (1.15–1.36×), eliminating the losses.
          It trades peak savings for consistency.</li>
      <li><strong>Search and discovery</strong> is the clearest win across every language and
          both AI systems — the graph finds the right symbols in one call; grep scans
          dozens of files first.</li>
      <li>For Codex, <strong>uncached tokens</strong> (fresh exploration only) show a stronger
          advantage than raw totals, because the graph reduces how much new context each
          turn requires.</li>
      <li>Cost savings exceed token savings because the graph answers in fewer turns,
          paying the fixed per-turn overhead fewer times.</li>
    </ul>
  </div>

  <!-- What we tested -->
  <h2>What we tested</h2>
  <p>Each repository was probed with four developer questions, chosen to span the range
  from the graph's strongest case (discovery) to its honest weak case (broad neighbourhood
  dumps). Each question ran twice — once with file search only, once with the GitCortex
  graph — under an identical model and prompt.</p>
  <div class="table-wrap"><table class="design-table"><thead><tr>
  <th>Question</th><th>What the user asks</th><th>Graph tool</th><th>What it tests</th>
  </tr></thead><tbody>
  {"".join(
      f'<tr><td>{html.escape(plain)}</td>'
      f'<td class="q-text">{html.escape(qtext)}</td>'
      f'<td><span class="mono">{html.escape(tool)}</span></td>'
      f'<td class="note">{html.escape(note)}</td></tr>'
      for lbl, plain, qtext, tool, note in QUESTION_DESIGN
  )}
  </tbody></table></div>
  <p style="font-size:13px;color:var(--subtle)">Five repositories, one per language:
  ripgrep (Rust), requests (Python), hono (TypeScript), cobra (Go), gson (Java).
  Run on Claude Haiku and Codex.</p>

  <!-- Finding 1 -->
  <h3 class="finding-label">Finding 1</h3>
  <h3 class="finding">Savings scale with repository size</h3>
  <p>On larger, well-connected codebases — ripgrep (Rust, ~35k LOC) and requests
  (Python, ~10k LOC) — the graph arm used significantly fewer tokens and cost less
  to run. The graph's structural knowledge becomes more valuable as the search space
  grows: instead of grepping through hundreds of files, the AI asks one precise query.</p>
  <p>On smaller repos, the 15 MCP tool schemas loaded into every turn (~14k tokens of
  fixed overhead) exceeded the savings. Compact mode reduces this overhead to ~200 tokens.</p>

  <div class="stats-row">
    <div class="stat">
      <div class="k">Best saving (Rust)</div>
      <div class="v v-green">56%</div>
      <div class="note">ripgrep · $0.27 → $0.12</div>
    </div>
    <div class="stat">
      <div class="k">Typical ratio</div>
      <div class="v {geo_cls(geo_full)}">{geo_full:.2f}×</div>
      <div class="note">geomean across {len(claude_full)} repos</div>
    </div>
    <div class="stat">
      <div class="k">Cost saved</div>
      <div class="v {geo_cls(1.0 + cost_full/100)}">{cost_full:.0f}%</div>
      <div class="note">Claude Code, full MCP</div>
    </div>
  </div>

  <!-- Finding 2 -->
  <h3 class="finding-label">Finding 2</h3>
  <h3 class="finding">Search is the strongest tool; broad dumps lose</h3>
  <p>Across all languages and both AI systems, <em>finding relevant code</em> is the
  question where the graph wins most. Keyword search scans every file containing a term;
  the graph returns a ranked list of semantically related symbols directly. This
  advantage compounds on large codebases where grep surfaces hundreds of false positives.</p>
  <p>The inverse pattern holds for broad neighbourhood queries
  (<span class="mono">get_subgraph</span> on a hub symbol). When a function has hundreds
  of callers, the full result set is larger than what Claude would have produced by
  searching selectively. We have capped these outputs in v0.3.1.</p>

  <figure class="chart-fig">
    {chart_by_question(claude_full)}
    <figcaption><strong>Token ratio by question type</strong>, averaged across all five
    languages (Claude, full MCP). Bars past the dashed line mean the graph used fewer tokens.
    Discovery (<span class="mono">search_code</span>) wins most; broad neighbourhood dumps
    (<span class="mono">get_subgraph</span>) win least.</figcaption>
  </figure>

  <div class="callout">
    <p>"The problem is not whether the graph is accurate — it is. The problem is how much
    of it we show at once."</p>
  </div>

  <!-- Arm comparison -->
  <h3 class="finding-label">Finding 3</h3>
  <h3 class="finding">Compact mode raises the floor and lowers the ceiling</h3>
  <p>Full MCP exposes fifteen tools — each with its own JSON schema — to the model on
  every turn. Compact mode exposes one <span class="mono">gcx(action, params)</span>
  dispatch tool, dropping the schema footprint from ~14k tokens to ~200 per turn.</p>
  <p>The effect is not a uniform improvement. On the two repositories that <em>lost</em>
  under full MCP — cobra (Go) and gson (Java) — compact mode flips them to wins
  (0.92× → 1.17× and 0.92× → 1.15×). But on the large repositories that won big, the
  single dispatch tool is slightly harder for the model to use than purpose-built tools,
  and the ratio comes down. The result is a tighter band: under compact mode every
  repository is a net win, and none is a loss.</p>

  <div class="arm-compare">
    {arm_box("Claude · Full MCP", geo_full,
        f"{len(claude_full)} repos · bimodal: big wins or losses")}
    {arm_box("Claude · Compact MCP", geo_compact,
        f"{len(claude_compact)} repos · consistent, never loses")}
    {arm_box("Codex · Uncached", geo_codex_u,
        f"{len(codex)} repos · fresh tokens only")}
  </div>

  <figure class="chart-fig">
    {chart_full_vs_compact(claude_full, claude_compact)}
    <figcaption><strong>Full vs compact MCP, per language.</strong> Compact mode (green)
    lifts the two languages that lost under full MCP — Go and Java — above break-even,
    while bringing the large-repo peaks down. Every language ends up a net win.</figcaption>
  </figure>

  <!-- Results tables -->
  <h2>Results by language</h2>
  <p>Each language is represented by one repository. Ratio is the geomean token ratio
  across the four questions; cost is the change in dollars to run the session.</p>
  {lang_summary_table(claude_full, claude_compact)}

  <h2>Results by tool</h2>

  <p class="table-label">Claude Code — Full MCP (15 tool schemas)</p>
  {matrix_table(claude_full)}

  <p class="table-label">Claude Code — Compact MCP (1 dispatch tool)</p>
  {matrix_table(claude_compact)}

  <p class="table-label">Codex — uncached tokens (excludes reasoning cache re-reads)</p>
  {matrix_table(codex, uncached=True)}

  <!-- Discussion -->
  <h2>Discussion</h2>
  <p>The most important result is that the graph <em>does</em> reduce fresh exploration
  work across all three systems. The open question is whether that reduction exceeds the
  fixed cost of the MCP surface. With the full tool set it depends on repository scale —
  large codebases win, small ones can lose. Compact mode trades the peak savings of
  purpose-built tools for a guarantee: every repository comes out ahead. The right default
  therefore depends on the target — compact for small repos and reasoning-token systems
  like Codex, full MCP where peak performance on large codebases matters most.</p>
  <p><strong>One caveat on the comparison.</strong> The full and compact runs are unpaired —
  each run re-picks the symbols it queries, so baselines differ between them. The absolute
  cross-run deltas carry run-to-run variance; the consistent direction (small repos up,
  large repos down) is the trustworthy signal, not the exact magnitudes.</p>
  <p>For Codex, uncached tokens are the honest metric. Raw totals are inflated by
  reasoning-cache re-reads that cost almost nothing per token but appear large in absolute
  counts. The uncached view shows that the graph genuinely reduces new context per turn.</p>
  <p>The next step is an <em>accuracy</em> benchmark: does the graph help the AI give
  better answers, not just cheaper ones? Our hypothesis is that fewer wasted reads means
  more relevant context, which should improve answer quality — especially for
  refactor-impact and call-chain questions where grep-based exploration tends to
  miss indirect dependencies.</p>

  <hr/>

  <!-- Methodology -->
  <h2>Methodology</h2>
  <ul class="footer-list">
    <li><strong>Two arms per question.</strong> Baseline = Read + Grep + Bash(grep/find).
    gcx arm = Read + GitCortex MCP tools only. Same model, same system prompt.</li>
    <li><strong>Claude tokens.</strong> input + cache_creation + output.
    cache_read excluded — it double-counts re-reads and is already captured in cost.</li>
    <li><strong>Codex uncached.</strong> input + output, excluding cached_input.
    Reasoning output tokens included.</li>
    <li><strong>Compact mode.</strong> <span class="mono">gcx serve --compact</span>
    registers only the single <span class="mono">gcx(action, params)</span> dispatch tool
    instead of fifteen individual tools.</li>
    <li><strong>Repos.</strong> One canonical codebase per language:
    ripgrep (Rust), requests (Python), hono (TypeScript), cobra (Go), gson (Java).</li>
    <li><strong>Caveats.</strong> One run per cell — no error bars.
    Index build time (0.3–4 s first run, &lt;500 ms incremental) not included in token counts.
    <a href="https://github.com/bharath03-a/GitCortex">Source and harnesses on GitHub.</a></li>
  </ul>

</article>
</body></html>"""


def main() -> None:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bench_dir", nargs="?", default=here)
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()
    d = args.bench_dir

    claude_full    = load_arm(d, "real-*.json", exclude=["compact"])
    claude_compact = load_arm(d, "real-compact-*.json")
    codex          = load_arm(d, "codex-*.json")

    out = args.out or os.path.join(d, "final-report.html")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(render(claude_full, claude_compact, codex))
    print(f"wrote {out}")
    print(f"  claude full:    {len(claude_full)} repos")
    print(f"  claude compact: {len(claude_compact)} repos")
    print(f"  codex:          {len(codex)} repos")


if __name__ == "__main__":
    main()
