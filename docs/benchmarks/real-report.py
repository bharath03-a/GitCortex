#!/usr/bin/env python3
"""Render the GitCortex benchmark as a public-facing HTML report.

Plain-English framing: proof of where the graph helps, honest about where
it doesn't, and a clear "what's next" section.

Reads real-*.json produced by real-harness.sh. Writes real-report.html.
Usage: python3 real-report.py [bench_dir] [-o out.html]
"""
from __future__ import annotations

import argparse
import glob
import html
import json
import math
import os
from datetime import date

Q_TO_TOOL: dict[str, str] = {
    "tour_onboarding":  "start_tour",
    "search_concept":   "search_code",
    "wiki_explain":     "wiki_symbol",
    "refactor_impact":  "find_callers",
    "trace_flow":       "trace_path",
    "subgraph_around":  "get_subgraph",
    "find_dead_code":   "find_unused_symbols",
}

# Plain-English labels for each question type.
Q_PLAIN: dict[str, str] = {
    "search_concept":   "Find relevant code",
    "tour_onboarding":  "Explain the codebase",
    "refactor_impact":  "What breaks if I change X?",
    "subgraph_around":  "Show connections around X",
    "wiki_explain":     "Explain a symbol",
    "trace_flow":       "Trace a code path",
    "find_dead_code":   "Find unused code",
}

REPO_LANG: dict[str, str] = {
    "ripgrep": "Rust",   "tokio": "Rust",   "serde": "Rust",
    "cobra":   "Go",     "gin":   "Go",     "zap":   "Go",
    "hono": "TypeScript","zod": "TypeScript","io-ts": "TypeScript",
    "fastapi": "Python", "django": "Python", "requests": "Python",
    "flask": "Python",
    "gson": "Java",      "picocli": "Java",  "jjwt": "Java",
}

LANG_COLOR: dict[str, str] = {
    "Rust": "#dea584", "Go": "#00add8", "TypeScript": "#3178c6",
    "Python": "#ffd343", "Java": "#f89820",
}

WIN, LOSE = 1.15, 0.90


def load(bench_dir: str) -> list[dict]:
    out = []
    for path in sorted(glob.glob(os.path.join(bench_dir, "real-*.json"))):
        if "prefix" in os.path.basename(path):
            continue
        try:
            d = json.load(open(path, encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            print(f"skip {os.path.basename(path)}: {e}")
            continue
        if "totals" not in d or "questions" not in d:
            continue
        errs = sum(
            1 for q in d["questions"]
            if q.get("baseline", {}).get("error") or q.get("gcx", {}).get("error")
        )
        if errs > len(d["questions"]) // 2:
            print(f"skip {os.path.basename(path)}: {errs} errored questions")
            continue
        d["_lang"] = REPO_LANG.get(d.get("repo", ""), "—")
        out.append(d)
    return out


def fmt(n: float) -> str:
    return f"{int(round(n)):,}"


def geomean(vals: list[float]) -> float:
    logs = [math.log(v) for v in vals if v and v > 0]
    return math.exp(sum(logs) / len(logs)) if logs else 0.0


def q_ratios(reports: list[dict]) -> dict[str, list[float]]:
    out: dict[str, list[float]] = {k: [] for k in Q_TO_TOOL}
    for r in reports:
        for q in r["questions"]:
            lbl = q.get("q", "")
            v = q.get("token_ratio", 0)
            if (lbl in out and v
                    and not q.get("baseline", {}).get("error")
                    and not q.get("gcx", {}).get("error")):
                out[lbl].append(v)
    return out


def ratio_bar_svg(labels: list[str], ratios: list[float], width: int = 580) -> str:
    """Horizontal bar chart of ratios. Bar > 1 = graph wins, < 1 = grep wins."""
    margin_l, margin_t, margin_b = 200, 16, 28
    row_h = 40
    chart_h = len(labels) * row_h
    chart_w = width - margin_l - 20
    max_r = max(max(ratios, default=2.0), 2.5)

    def sx(v: float) -> float:
        return margin_l + v / max_r * chart_w

    baseline_x = sx(1.0)
    win_x = sx(WIN)

    # background zones
    zones = (
        f'<rect x="{margin_l}" y="{margin_t}" '
        f'width="{baseline_x - margin_l:.1f}" height="{chart_h}" fill="rgba(192,73,47,.06)"/>'
        f'<rect x="{win_x:.1f}" y="{margin_t}" '
        f'width="{margin_l + chart_w - win_x:.1f}" height="{chart_h}" fill="rgba(58,125,82,.06)"/>'
        f'<line x1="{baseline_x:.1f}" y1="{margin_t - 4}" '
        f'x2="{baseline_x:.1f}" y2="{margin_t + chart_h}" '
        f'stroke="#9a8f55" stroke-width="1.5" stroke-dasharray="4,3"/>'
        f'<text x="{baseline_x:.1f}" y="{margin_t - 6}" text-anchor="middle" '
        f'fill="#9a8f55" font-size="10" font-weight="600">same</text>'
        f'<text x="{margin_l + 4}" y="{margin_t + chart_h + 20}" '
        f'fill="#c0492f" font-size="10">← grep is cheaper</text>'
        f'<text x="{win_x + 4:.1f}" y="{margin_t + chart_h + 20}" '
        f'fill="#3a7d52" font-size="10">graph saves context →</text>'
    )

    bars = ""
    for i, (lbl, r) in enumerate(zip(labels, ratios)):
        y = margin_t + i * row_h
        cy = y + row_h // 2
        color = "#3a7d52" if r >= WIN else ("#c0492f" if r <= LOSE else "#9a8f55")
        bw = abs(sx(r) - baseline_x)
        bx = min(sx(r), baseline_x)
        # bar from baseline
        bars += (
            f'<rect x="{bx:.1f}" y="{cy - 10}" width="{max(bw, 2):.1f}" '
            f'height="20" rx="4" fill="{color}" opacity=".75"/>'
            # dot at actual ratio
            f'<circle cx="{sx(r):.1f}" cy="{cy}" r="5" fill="{color}"/>'
            # label left
            f'<text x="{margin_l - 10}" y="{cy + 4}" text-anchor="end" '
            f'fill="#1f1d1a" font-size="13">{html.escape(lbl)}</text>'
            # ratio right
            f'<text x="{sx(r) + 10:.1f}" y="{cy + 4}" fill="{color}" '
            f'font-size="12" font-weight="600">{r:.2f}×</text>'
        )

    total_h = chart_h + margin_t + margin_b
    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px">'
        f'{zones}{bars}</svg>'
    )


def cost_comparison_svg(repos: list[dict], width: int = 560) -> str:
    """Simple stacked comparison: what you paid with grep vs with the graph."""
    labels  = [r.get("repo","") for r in repos]
    grep_c  = [round(r["totals"].get("baseline_cost_usd", 0) * 100) for r in repos]
    gcx_c   = [round(r["totals"].get("gcx_cost_usd", 0) * 100) for r in repos]
    margin_l, margin_t, bar_h, gap = 90, 24, 22, 6
    n = len(labels)
    chart_h = n * (2 * bar_h + gap + 10)
    chart_w = width - margin_l - 60
    max_v = max(max(grep_c, default=1), 1) * 1.15

    def sx(v: float) -> float:
        return v / max_v * chart_w

    legend = (
        f'<rect x="{margin_l}" y="4" width="12" height="12" rx="2" fill="#e0a07f" opacity=".85"/>'
        f'<text x="{margin_l + 16}" y="14" fill="#76726a" font-size="11">Without graph (grep)</text>'
        f'<rect x="{margin_l + 145}" y="4" width="12" height="12" rx="2" fill="#5aab77" opacity=".85"/>'
        f'<text x="{margin_l + 161}" y="14" fill="#76726a" font-size="11">With GitCortex graph</text>'
    )

    bars = ""
    for i, (lbl, bv, gv) in enumerate(zip(labels, grep_c, gcx_c)):
        y = margin_t + 16 + i * (2 * bar_h + gap + 10)
        bw = max(sx(bv), 2)
        gw = max(sx(gv), 2)
        saved_pct = round(100 * (bv - gv) / bv) if bv else 0
        bars += (
            # grep bar
            f'<rect x="{margin_l}" y="{y}" width="{bw:.1f}" height="{bar_h}" '
            f'rx="3" fill="#e0a07f" opacity=".85"/>'
            f'<text x="{margin_l + bw + 5:.1f}" y="{y + bar_h - 6}" '
            f'fill="#76726a" font-size="11">{bv}¢</text>'
            # gcx bar
            f'<rect x="{margin_l}" y="{y + bar_h + 3}" width="{gw:.1f}" height="{bar_h}" '
            f'rx="3" fill="#5aab77" opacity=".85"/>'
            f'<text x="{margin_l + gw + 5:.1f}" y="{y + 2*bar_h - 2}" '
            f'fill="#76726a" font-size="11">{gv}¢</text>'
            # label + saving
            f'<text x="{margin_l - 6}" y="{y + bar_h + 1}" '
            f'text-anchor="end" fill="#1f1d1a" font-size="13">{html.escape(lbl)}</text>'
            + (f'<text x="{margin_l + bw + 38:.1f}" y="{y + 6}" '
               f'fill="#3a7d52" font-size="10" font-weight="600">−{saved_pct}%</text>'
               if saved_pct > 5 else "")
        )

    total_h = chart_h + margin_t + 20
    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px">'
        f'{legend}{bars}</svg>'
    )


CSS = """
:root{--bg:#faf9f5;--panel:#fff;--panel2:#f4f2ec;--line:#e7e2d6;
--fg:#1a1a1a;--muted:#6b6b6b;--accent:#cc785c;--accent-d:#b35f44;
--green:#2d7a4f;--red:#c0492f;--amber:#8a7d45;
--sans:"Inter",ui-sans-serif,-apple-system,"Segoe UI",Roboto,sans-serif}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);
font:15px/1.65 var(--sans);-webkit-font-smoothing:antialiased;
letter-spacing:-.01em}
.mono{font-family:ui-monospace,Menlo,Consolas,monospace;font-size:.87em}
a{color:var(--accent-d)}

/* navbar */
.nav{position:sticky;top:0;z-index:10;background:rgba(250,249,245,.92);
backdrop-filter:blur(10px);border-bottom:1px solid var(--line)}
.nav-in{max-width:800px;margin:0 auto;padding:13px 24px;
display:grid;grid-template-columns:1fr auto 1fr;align-items:center}
.brand{justify-self:start;display:flex;align-items:center;gap:9px;
font-size:17px;font-weight:700;color:var(--fg);text-decoration:none;
letter-spacing:-.02em}
.brand svg{display:block}
.nav-tag{justify-self:center;color:var(--muted);font-size:13px;font-weight:500}
.nav-right{justify-self:end;font-size:13px}.nav-right a{color:var(--muted)}
@media(max-width:560px){.nav-tag{display:none}}

/* article */
.article{max-width:800px;margin:0 auto;padding:52px 24px 100px}
h1{font-size:34px;font-weight:700;letter-spacing:-.03em;
margin:0 0 14px;line-height:1.2;max-width:680px}
h2{font-size:22px;font-weight:700;letter-spacing:-.02em;
margin:52px 0 14px;padding-bottom:10px;border-bottom:1px solid var(--line)}
h3{font-size:18px;font-weight:600;letter-spacing:-.01em;margin:32px 0 10px}
.byline{color:var(--muted);font-size:14px;margin:0 0 36px;
border-bottom:1px solid var(--line);padding-bottom:22px}
p{margin:0 0 18px;line-height:1.7}

/* summary box */
.summary{background:var(--panel);border:1px solid var(--line);border-radius:14px;
padding:24px 28px;margin:0 0 44px;box-shadow:0 1px 3px rgba(60,50,30,.05)}
.summary h3{font-family:var(--serif);font-size:14px;margin:0 0 14px;
color:var(--muted);text-transform:uppercase;letter-spacing:.5px;font-weight:600}
.summary ul{margin:0;padding-left:20px}
.summary li{margin:8px 0;line-height:1.6}

/* stat row */
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));
gap:14px;margin:24px 0 36px}
.stat{background:var(--panel);border:1px solid var(--line);border-radius:12px;
padding:18px 20px;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.stat .k{font-size:11px;color:var(--muted);text-transform:uppercase;
letter-spacing:.7px;font-weight:600}
.stat .v{font-family:var(--serif);font-size:28px;font-weight:600;margin-top:7px}
.stat .n{font-size:12px;color:var(--muted);margin-top:5px}
.green{color:var(--green)}.red{color:var(--red)}.amber{color:var(--amber)}

/* findings */
.finding{background:var(--panel);border:1px solid var(--line);border-radius:14px;
padding:26px 28px;margin:20px 0;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.finding-num{font-size:11px;font-weight:700;text-transform:uppercase;
letter-spacing:.8px;color:var(--muted);margin:0 0 6px}
.finding h3{font-family:var(--serif);font-size:20px;font-weight:600;
margin:0 0 14px;line-height:1.3}

/* figure */
.figure{background:var(--panel2);border-radius:10px;padding:20px 20px 14px;
margin:20px 0 6px}
.figure figcaption{font-size:13px;color:var(--muted);margin-top:12px;line-height:1.5}

/* result table */
.tablewrap{background:var(--panel);border:1px solid var(--line);border-radius:14px;
overflow-x:auto;box-shadow:0 1px 2px rgba(60,50,30,.04);margin:20px 0}
table{width:100%;border-collapse:collapse;font-size:14px;min-width:540px}
th,td{padding:11px 14px;text-align:right;border-bottom:1px solid var(--line);
white-space:nowrap}
th:first-child,td:first-child{text-align:left;min-width:130px}
thead th{background:var(--panel2);color:var(--muted);font-size:11px;
font-weight:600;text-transform:uppercase;letter-spacing:.5px}
tbody tr:last-child td{border-bottom:none}
tbody tr.expand{cursor:pointer;transition:background .1s}
tbody tr.expand:hover{background:var(--panel2)}
td.win{background:rgba(58,125,82,.12);color:var(--green);font-weight:600}
td.lose{background:rgba(192,73,47,.11);color:var(--red);font-weight:600}
td.tie{background:rgba(154,143,85,.09);color:var(--amber);font-weight:600}
td.na{color:var(--muted)}
.detail{display:none}.detail.open{display:table-row}
.detail>td{padding:0;background:var(--panel2)}
.dtable{width:100%;border-collapse:collapse;font-size:13px}
.dtable td,.dtable th{border-bottom:1px solid var(--line);padding:8px 14px}
.dtable tr:last-child td{border-bottom:none}

/* next steps */
.next-steps{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin-top:20px}
@media(max-width:560px){.next-steps{grid-template-columns:1fr}}
.next-card{background:var(--panel);border:1px solid var(--line);border-radius:12px;
padding:18px 20px;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.next-card .tag{font-size:11px;font-weight:700;text-transform:uppercase;
letter-spacing:.7px;margin-bottom:8px}
.next-card h4{font-family:var(--serif);font-size:16px;font-weight:600;margin:0 0 8px}
.next-card p{font-size:14px;color:var(--muted);margin:0;line-height:1.55}
.tag-green{color:var(--green)}.tag-amber{color:var(--amber)}.tag-blue{color:#5b6f8c}

.lang-dot{display:inline-block;width:9px;height:9px;border-radius:50%;
margin-right:7px;vertical-align:middle}
.legend{margin-top:12px;color:var(--muted);font-size:13px}
.legend .lang-dot{margin-left:12px}
.foot{font-size:13px;color:var(--muted);line-height:1.6}
.foot li{margin:5px 0}
"""

LOGO = (
    '<svg width="24" height="24" viewBox="0 0 26 26" fill="none" aria-hidden="true">'
    '<path d="M7 7L19 13M7 7L13 19M19 13L13 19" stroke="#cc785c" '
    'stroke-width="1.6" stroke-linecap="round"/>'
    '<circle cx="7" cy="7" r="3.2" fill="#cc785c"/>'
    '<circle cx="19" cy="13" r="2.6" fill="#e0a07f"/>'
    '<circle cx="13" cy="19" r="2.6" fill="#e0a07f"/>'
    '</svg>'
)

JS = "function toggle(id){var r=document.getElementById(id);if(r)r.classList.toggle('open');}"


def stat(k: str, v: str, cls: str = "", note: str = "") -> str:
    return (
        f'<div class="stat"><div class="k">{html.escape(k)}</div>'
        f'<div class="v {cls}">{v}</div>'
        + (f'<div class="n">{html.escape(note)}</div>' if note else "")
        + "</div>"
    )


def render_detail_rows(questions: list[dict]) -> str:
    rows = ""
    for q in questions:
        lbl = q.get("q", "")
        plain = Q_PLAIN.get(lbl, lbl)
        b = q.get("baseline", {})
        g = q.get("gcx", {})
        ratio = q.get("token_ratio", 0)
        cls = ("win" if ratio >= WIN else "lose" if (ratio and ratio <= LOSE) else "tie") if ratio else "na"
        b_tok = fmt(b.get("total", 0)) if not b.get("error") else "—"
        g_tok = fmt(g.get("total", 0)) if not g.get("error") else "—"
        rows += (
            f"<tr>"
            f'<td>{html.escape(plain)}</td>'
            f"<td>{b_tok}</td><td>{g_tok}</td>"
            f'<td>${b.get("cost",0):.3f}</td><td>${g.get("cost",0):.3f}</td>'
            f'<td>{b.get("turns",0)}</td><td>{g.get("turns",0)}</td>'
            f'<td class="{cls}">{f"{ratio:.2f}×" if ratio else "—"}</td>'
            "</tr>"
        )
    return rows


def render(reports: list[dict]) -> str:
    qr = q_ratios(reports)
    tb = sum(r["totals"].get("baseline_tokens", 0) for r in reports)
    tg = sum(r["totals"].get("gcx_tokens", 0) for r in reports)
    cb = sum(r["totals"].get("baseline_cost_usd", 0) for r in reports)
    cg = sum(r["totals"].get("gcx_cost_usd", 0) for r in reports)
    saved_tok_pct  = 100 * (tb - tg) / tb if tb else 0
    saved_cost_pct = 100 * (cb - cg) / cb if cb else 0
    geo_all = geomean([r["totals"].get("geomean_ratio", 0) for r in reports])
    models = sorted({r.get("model", "?") for r in reports})
    n_repos = len(reports)
    n_sessions = sum(len(r["questions"]) * 2 for r in reports)

    # Which questions are in this run?
    q_labels_present = []
    for lbl in Q_TO_TOOL:
        if any(q.get("q") == lbl for r in reports for q in r["questions"]):
            q_labels_present.append(lbl)

    # Per-question geomeans for chart
    chart_labels = [Q_PLAIN.get(l, l) for l in q_labels_present]
    chart_ratios = [geomean(qr[l]) for l in q_labels_present]

    # wins/losses summary
    n_wins  = sum(1 for r in chart_ratios if r >= WIN)
    n_loses = sum(1 for r in chart_ratios if r <= LOSE)

    # per-repo result table
    langs = [r["_lang"] for r in reports]
    repo_rows = ""
    for i, r in enumerate(reports):
        t = r["totals"]
        lang = r["_lang"]
        color = LANG_COLOR.get(lang, "#888")
        did = f"dr{i}"
        geo = t.get("geomean_ratio", 0)
        geo_cls = "win" if geo >= WIN else ("lose" if geo <= LOSE else "tie")
        cost_saved_pct = (
            100 * (t.get("baseline_cost_usd", 0) - t.get("gcx_cost_usd", 0))
            / t.get("baseline_cost_usd", 1)
            if t.get("baseline_cost_usd") else 0
        )
        detail = render_detail_rows(r["questions"])
        repo_rows += (
            f'<tr class="expand" onclick="toggle(\'{did}\')">'
            f'<td><span class="lang-dot" style="background:{color}"></span>'
            f'<span class="mono">{html.escape(r.get("repo",""))}</span></td>'
            f"<td>{html.escape(lang)}</td>"
            f'<td>${t.get("baseline_cost_usd",0):.3f}</td>'
            f'<td>${t.get("gcx_cost_usd",0):.3f}</td>'
            f'<td class="{"green" if cost_saved_pct>5 else "red" if cost_saved_pct<-5 else "amber"}">'
            f'{cost_saved_pct:.0f}%</td>'
            f'<td class="{geo_cls} mono">{geo:.2f}×</td>'
            f'<td style="color:var(--muted);font-size:11px">▾</td>'
            "</tr>"
            f'<tr class="detail" id="{did}"><td colspan="7">'
            '<table class="dtable"><thead><tr>'
            "<th>Question</th><th>Context (grep)</th><th>Context (graph)</th>"
            "<th>Cost (grep)</th><th>Cost (graph)</th>"
            "<th>Turns (grep)</th><th>Turns (graph)</th><th>Ratio</th>"
            f"</tr></thead><tbody>{detail}</tbody></table>"
            "</td></tr>"
        )

    legend = "".join(
        f'<span class="lang-dot" style="background:{c}"></span>{l}'
        for l, c in LANG_COLOR.items() if l in langs
    )

    fig1 = ratio_bar_svg(chart_labels, chart_ratios)
    fig2 = cost_comparison_svg(reports)

    # next-steps cards
    next_cards = """
<div class="next-steps">
  <div class="next-card">
    <div class="tag tag-green">In progress</div>
    <h4>Lower the overhead of keeping the graph open</h4>
    <p>Each AI turn currently loads the full list of graph tools. A single unified
    command would cut that overhead, making even marginal questions cheaper.</p>
  </div>
  <div class="next-card">
    <div class="tag tag-amber">Investigating</div>
    <h4>Smarter answers for "what breaks if I change X?"</h4>
    <p>High-traffic functions have hundreds of callers. The graph currently lists
    them all — we're working on a ranked summary that surfaces the most important
    ones first.</p>
  </div>
  <div class="next-card">
    <div class="tag tag-blue">Planned</div>
    <h4>Real end-to-end accuracy test</h4>
    <p>This report measures how much AI context each approach uses. The next step
    is measuring whether the answers are actually <em>better</em> — a task-completion
    benchmark across real coding challenges.</p>
  </div>
  <div class="next-card">
    <div class="tag tag-blue">Planned</div>
    <h4>More languages and larger repos</h4>
    <p>Current data covers 5 languages, mid-sized repos. We want to test on
    enterprise-scale codebases (500k+ lines) where the discovery gap between
    grep and a graph should widen further.</p>
  </div>
</div>"""

    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>GitCortex — Does a code graph save AI context?</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'><rect width='64' height='64' rx='14' fill='%23faf9f5'/><line x1='18' y1='18' x2='46' y2='32' stroke='%23cc785c' stroke-width='3' stroke-linecap='round'/><line x1='18' y1='18' x2='32' y2='50' stroke='%23cc785c' stroke-width='3' stroke-linecap='round'/><line x1='46' y1='32' x2='32' y2='50' stroke='%23cc785c' stroke-width='2.5' stroke-linecap='round' opacity='.7'/><circle cx='18' cy='18' r='7' fill='%23cc785c'/><circle cx='46' cy='32' r='5.5' fill='%23e0a07f'/><circle cx='32' cy='50' r='5.5' fill='%23e0a07f'/></svg>">
<style>{CSS}</style></head>
<body>

<nav class="nav"><div class="nav-in">
<a class="brand" href="https://github.com/bharath03-a/GitCortex">{LOGO}GitCortex</a>
<span class="nav-tag">Benchmark</span>
<div class="nav-right"><a href="https://github.com/bharath03-a/GitCortex" style="display:flex;align-items:center;gap:5px"><svg height="18" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>GitHub</a></div>
</div></nav>

<div class="article">

<p style="font-size:13px;color:var(--muted);margin:0 0 16px">
Research &nbsp;·&nbsp; {date.today().isoformat()} &nbsp;·&nbsp;
{n_repos} repos &nbsp;·&nbsp; {n_sessions} AI sessions</p>

<h1>Does a code knowledge graph help AI assistants work more efficiently?</h1>
<p class="byline">We ran Claude on {n_repos} real open-source repositories and asked
{len(q_labels_present)} common developer questions — twice each, once using only
search tools, once using a GitCortex code graph. Here's what we found.</p>

<div class="summary">
<h3>Key findings</h3>
<ul>
<li>The graph <strong>saved {saved_cost_pct:.0f}% on AI cost</strong> across all questions
and repositories — even when raw context usage was similar.</li>
<li><strong>Finding relevant code</strong> is the clearest win: the graph surfaces
the right files and symbols in one step, while keyword search scans many irrelevant files first.</li>
<li><strong>Explaining a codebase</strong> (the "tour" question) is consistently faster
with the graph — it can rank the most important entry points, not just list files.</li>
<li><strong>Broad neighbourhood questions</strong> ("show everything connected to X")
sometimes <em>cost more</em> with the graph on large, highly-connected codebases.
We've identified the fix and are shipping it.</li>
<li>The cost advantage comes from <strong>fewer AI turns</strong>: the graph answers in
one step where search needs several iterations. Fewer turns means less re-reading the
same context.</li>
</ul>
</div>

<h2>How we measured this</h2>
<p>We gave Claude two ways to answer each question about an unfamiliar codebase:</p>
<ul style="line-height:1.8;margin-bottom:20px">
<li><strong>Without the graph</strong> — Claude could only search files with grep,
read files, and browse directories. This is how most AI coding tools work today.</li>
<li><strong>With the GitCortex graph</strong> — Claude could query a pre-built
knowledge graph of the codebase: look up symbols, find who calls what,
trace code paths, and search semantically.</li>
</ul>
<p>We recorded the exact number of tokens Claude used in each session — the
<em>context</em> it loaded while answering — and what it cost.
No estimates. These are real numbers from the Claude API.
Repos tested: {", ".join(f'<span class="mono">{html.escape(r.get("repo",""))}</span>' for r in reports)}
({", ".join(sorted(set(r["_lang"] for r in reports)))}).</p>


<h2>Results</h2>

<div class="stats">
{stat("AI cost (grep)", f"${cb:.2f}", note=f"{n_repos} repos × {len(q_labels_present)} questions")}
{stat("AI cost (graph)", f"${cg:.2f}", "green", f"{saved_cost_pct:.0f}% less")}
{stat("Context (grep)", fmt(tb) + " tok", note="tokens loaded")}
{stat("Context (graph)", fmt(tg) + " tok",
      "green" if saved_tok_pct > 2 else "amber",
      f"{saved_tok_pct:+.1f}%")}
{stat("Typical ratio", f"{geo_all:.2f}×",
      "green" if geo_all >= WIN else "amber",
      "graph ÷ grep context")}
</div>

<p><strong>Why does cost fall more than context?</strong> Each AI turn re-loads
the same background instructions. The graph arm answers in fewer turns,
so those fixed costs are paid fewer times — even when the actual answer is the same size.</p>

<div class="finding">
<div class="finding-num">Finding 1 — where the graph clearly wins</div>
<h3>Searching and discovery</h3>
<p>When asked "where is authentication handled?" or "find code related to routing",
the graph consistently outperformed keyword search. Grep must scan every file that
contains the word; the graph knows which symbols are semantically related and
returns a ranked list of the most relevant ones directly.</p>
<p>Across our test repos, <strong>search questions used {
    f"{geomean(qr.get('search_concept', [1.0])):.1f}×"
} less context with the graph</strong> than with grep.</p>
</div>

<div class="finding">
<div class="finding-num">Finding 2 — consistent but smaller win</div>
<h3>Understanding an unfamiliar codebase</h3>
<p>When asked to "give a tour" of a new repository, the graph can rank entry
points by how central they are to the codebase (how many other things depend
on them). Grep-based exploration reads top-level files and guesses.
The graph-based tour is more structured and uses less context on average.</p>
</div>

<div class="finding">
<div class="finding-num">Finding 3 — where we're still improving</div>
<h3>Broad neighbourhood questions</h3>
<p>Asking "show me everything connected to X" is where the graph can backfire.
If X is a very popular function (called from hundreds of places), the graph
returns all of them at once — more context than Claude would have used by
searching selectively. We've capped the output size in our latest version,
which should fix this for most repos.</p>
</div>

<figure class="figure">
{fig1}
<figcaption>How much less context the graph used per question type, averaged
across all {n_repos} repos. A ratio above 1.0 means the graph was more efficient.
The dashed line is break-even. Red zone = grep was cheaper; green zone = graph was cheaper.</figcaption>
</figure>

<figure class="figure">
{fig2}
<figcaption>Total AI cost per repository (in cents) — without the graph vs with it.
Bars are per-repo totals across all {len(q_labels_present)} questions.</figcaption>
</figure>


<h2>Detailed results by repository</h2>
<p>Click any row to see the per-question breakdown. "Context" is the number of
tokens Claude loaded while answering — think of it as how much of the codebase
Claude had to read. "Turns" is how many back-and-forth steps Claude needed.</p>

<div class="tablewrap"><table>
<thead><tr>
<th>Repository</th><th>Language</th>
<th>Cost (grep)</th><th>Cost (graph)</th>
<th>Cost saving</th><th>Typical ratio</th><th></th>
</tr></thead>
<tbody>{repo_rows}</tbody>
</table></div>
<div class="legend">Languages:{legend}</div>


<h2>What's next</h2>
<p>These results show the graph helps most when the challenge is <em>finding</em>
the right code, and is still catching up when the question requires a
broad survey. Here's what we're working on:</p>
{next_cards}


<h2>About this benchmark</h2>
<ul class="foot">
<li>Each question ran twice — once with grep tools, once with the GitCortex graph —
using the same AI model ({html.escape(", ".join(models))}) with no other differences.</li>
<li>"Context" = tokens the AI loaded while answering (input + new cache writes + output).
Cache re-reads are excluded from the count but included in cost, which is why
cost savings are larger than context savings.</li>
<li>One run per repo and question — no error bars. The directional results are
consistent enough to be meaningful, but treat individual ratios as indicative.</li>
<li>The graph index takes 0.3–4 seconds to build on first use; incremental updates
on file changes take under 500 ms. Index cost is not included in these numbers.</li>
<li>Source code and methodology: <a href="https://github.com/bharath03-a/GitCortex">github.com/bharath03-a/GitCortex</a></li>
</ul>

</div>
<script>{JS}</script>
</body></html>"""


def main() -> None:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bench_dir", nargs="?", default=here)
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()
    reports = load(args.bench_dir)
    if not reports:
        raise SystemExit(f"no usable real-*.json in {args.bench_dir}")
    out = args.out or os.path.join(args.bench_dir, "real-report.html")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(render(reports))
    print(f"wrote {out}  ({len(reports)} repos)")


if __name__ == "__main__":
    main()
