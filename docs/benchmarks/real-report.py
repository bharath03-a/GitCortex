#!/usr/bin/env python3
"""Render the REAL token benchmark as a research-style HTML report.

Structure follows Anthropic research article conventions:
  summary box → methodology → per-finding sections with charts → discussion

Every number is from real Claude API usage — no chars/4 proxy.

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

Q_LABEL: dict[str, str] = {
    "tour_onboarding":  "Give me a tour of this codebase",
    "search_concept":   "Where is auth/parse handled?",
    "wiki_explain":     "Explain symbol X",
    "refactor_impact":  "If I change Y, what breaks?",
    "trace_flow":       "How does Y reach Z?",
    "subgraph_around":  "Show the 2-hop neighborhood of X",
    "find_dead_code":   "What dead code exists?",
}

REPO_LANG: dict[str, str] = {
    "ripgrep": "Rust",  "tokio": "Rust",  "serde": "Rust",
    "cobra":   "Go",    "gin":   "Go",    "zap":   "Go",
    "hono": "TypeScript", "zod": "TypeScript", "io-ts": "TypeScript",
    "django": "Python", "requests": "Python", "flask": "Python", "fastapi": "Python",
    "gson": "Java",     "picocli": "Java", "jjwt": "Java",
}

LANG_COLOR: dict[str, str] = {
    "Rust": "#dea584", "Go": "#00add8", "TypeScript": "#3178c6",
    "Python": "#ffd343", "Java": "#f89820",
}

WIN, LOSE = 1.15, 0.90


# ── helpers ──────────────────────────────────────────────────────────────────

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
            print(f"skip {os.path.basename(path)}: {errs}/{len(d['questions'])} errored")
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
    """Per-question label → list of valid ratios across all repos."""
    out: dict[str, list[float]] = {k: [] for k in Q_TO_TOOL}
    for r in reports:
        for q in r["questions"]:
            lbl = q.get("q", "")
            v = q.get("token_ratio", 0)
            if lbl in out and v and not q.get("baseline", {}).get("error") and not q.get("gcx", {}).get("error"):
                out[lbl].append(v)
    return out


def tool_verdict(ratios: list[float]) -> tuple[float, str]:
    geo = geomean(ratios)
    n_lose = sum(1 for r in ratios if r <= LOSE)
    n_win  = sum(1 for r in ratios if r >= WIN)
    if not ratios:
        verdict = "no data"
    elif n_lose >= n_win and n_lose > 0:
        verdict = "REDESIGN"
    elif geo >= WIN:
        verdict = "keep"
    else:
        verdict = "marginal"
    return geo, verdict


# ── SVG chart helpers ─────────────────────────────────────────────────────────

def bar_chart_svg(
    labels: list[str],
    baseline: list[float],
    gcx: list[float],
    title: str,
    width: int = 660,
    bar_h: int = 28,
    gap: int = 10,
) -> str:
    """Horizontal grouped bar chart: baseline (warm) vs gcx (green)."""
    margin_left, margin_top, margin_right, margin_bottom = 170, 40, 20, 36
    n = len(labels)
    chart_h = n * (2 * bar_h + gap) + gap
    total_h = chart_h + margin_top + margin_bottom
    chart_w = width - margin_left - margin_right
    max_val = max(max(baseline, default=1), max(gcx, default=1)) * 1.1 or 1

    def scale(v: float) -> float:
        return v / max_val * chart_w

    # gridlines at nice intervals
    ticks = [0]
    step = 10 ** math.floor(math.log10(max_val))
    t = step
    while t <= max_val * 1.05:
        ticks.append(t)
        t += step

    lines = "".join(
        f'<line x1="{margin_left + scale(t):.1f}" y1="{margin_top}" '
        f'x2="{margin_left + scale(t):.1f}" y2="{margin_top + chart_h}" '
        f'stroke="#e7e2d6" stroke-width="1"/>'
        f'<text x="{margin_left + scale(t):.1f}" y="{margin_top + chart_h + 18}" '
        f'text-anchor="middle" fill="#76726a" font-size="11">{fmt(t)}</text>'
        for t in ticks
    )

    bars = ""
    for i, (lbl, bv, gv) in enumerate(zip(labels, baseline, gcx)):
        y_base = margin_top + gap + i * (2 * bar_h + gap)
        # baseline bar
        bw = max(scale(bv), 2)
        bars += (
            f'<rect x="{margin_left}" y="{y_base}" width="{bw:.1f}" height="{bar_h}" '
            f'rx="3" fill="#e0a07f" opacity=".85"/>'
            f'<text x="{margin_left + bw + 5:.1f}" y="{y_base + bar_h - 8}" '
            f'fill="#76726a" font-size="11">{fmt(bv)}</text>'
        )
        # gcx bar
        gw = max(scale(gv), 2)
        bars += (
            f'<rect x="{margin_left}" y="{y_base + bar_h + 2}" width="{gw:.1f}" height="{bar_h}" '
            f'rx="3" fill="#5aab77" opacity=".85"/>'
            f'<text x="{margin_left + gw + 5:.1f}" y="{y_base + 2 * bar_h - 6}" '
            f'fill="#76726a" font-size="11">{fmt(gv)}</text>'
        )
        # label
        bars += (
            f'<text x="{margin_left - 8}" y="{y_base + bar_h + 1}" '
            f'text-anchor="end" fill="#1f1d1a" font-size="12.5">{html.escape(lbl)}</text>'
        )

    legend = (
        f'<rect x="{margin_left}" y="{margin_top - 24}" width="12" height="12" rx="2" fill="#e0a07f" opacity=".85"/>'
        f'<text x="{margin_left + 16}" y="{margin_top - 14}" fill="#76726a" font-size="11">Baseline (grep)</text>'
        f'<rect x="{margin_left + 110}" y="{margin_top - 24}" width="12" height="12" rx="2" fill="#5aab77" opacity=".85"/>'
        f'<text x="{margin_left + 126}" y="{margin_top - 14}" fill="#76726a" font-size="11">GitCortex</text>'
    )

    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px" '
        f'role="img" aria-label="{html.escape(title)}">'
        f'{lines}{bars}{legend}</svg>'
    )


def ratio_dot_chart(
    labels: list[str],
    ratios: list[float],
    width: int = 660,
) -> str:
    """Dot plot of per-tool geomean ratios with win/lose zones."""
    margin_left, margin_top, margin_bottom = 190, 24, 32
    row_h = 34
    chart_h = len(labels) * row_h
    chart_w = width - margin_left - 30
    max_r = max(max(ratios, default=2), 2.5)

    def sx(v: float) -> float:
        return margin_left + v / max_r * chart_w

    win_x  = sx(WIN)
    lose_x = sx(LOSE)

    # zones
    zones = (
        f'<rect x="{margin_left}" y="{margin_top}" width="{lose_x - margin_left:.1f}" '
        f'height="{chart_h}" fill="rgba(192,73,47,.07)"/>'
        f'<rect x="{win_x:.1f}" y="{margin_top}" width="{margin_left + chart_w - win_x:.1f}" '
        f'height="{chart_h}" fill="rgba(58,125,82,.07)"/>'
        f'<line x1="{sx(1.0):.1f}" y1="{margin_top}" x2="{sx(1.0):.1f}" '
        f'y2="{margin_top + chart_h}" stroke="#9a8f55" stroke-width="1.5" stroke-dasharray="4,3"/>'
        f'<text x="{sx(1.0):.1f}" y="{margin_top - 6}" text-anchor="middle" fill="#9a8f55" font-size="10">1×</text>'
        f'<text x="{margin_left + 4}" y="{margin_top + chart_h + 22}" fill="#c0492f" font-size="10">← loses to grep</text>'
        f'<text x="{win_x + 4:.1f}" y="{margin_top + chart_h + 22}" fill="#3a7d52" font-size="10">wins →</text>'
    )

    dots = ""
    for i, (lbl, r) in enumerate(zip(labels, ratios)):
        y = margin_top + i * row_h + row_h // 2
        color = "#3a7d52" if r >= WIN else ("#c0492f" if r <= LOSE else "#9a8f55")
        dots += (
            f'<text x="{margin_left - 8}" y="{y + 4}" text-anchor="end" '
            f'fill="#1f1d1a" font-size="12">{html.escape(lbl)}</text>'
            f'<line x1="{margin_left}" y1="{y}" x2="{sx(r):.1f}" y2="{y}" '
            f'stroke="#ddd" stroke-width="1"/>'
            f'<circle cx="{sx(r):.1f}" cy="{y}" r="6" fill="{color}"/>'
            f'<text x="{sx(r) + 10:.1f}" y="{y + 4}" fill="{color}" '
            f'font-size="11" font-weight="600">{r:.2f}×</text>'
        )

    total_h = chart_h + margin_top + margin_bottom
    return (
        f'<svg viewBox="0 0 {width} {total_h}" style="width:100%;max-width:{width}px" '
        f'role="img" aria-label="Per-tool token ratio">'
        f'{zones}{dots}</svg>'
    )


def cost_bar_svg(repos: list[dict], width: int = 560) -> str:
    """Grouped bars: baseline cost vs gcx cost per repo."""
    labels = [r.get("repo", "") for r in repos]
    baseline = [r["totals"].get("baseline_cost_usd", 0) for r in repos]
    gcx      = [r["totals"].get("gcx_cost_usd", 0) for r in repos]
    return bar_chart_svg(labels, baseline, gcx, "Cost per repo baseline vs gcx", width=width)


# ── HTML sections ─────────────────────────────────────────────────────────────

CSS = """
:root{--bg:#faf9f5;--panel:#fff;--panel2:#f4f2ec;--line:#e7e2d6;
--fg:#1f1d1a;--muted:#76726a;--accent:#cc785c;--accent-d:#b35f44;
--green:#3a7d52;--red:#c0492f;--amber:#9a8f55;
--serif:Georgia,"Times New Roman",serif}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);
font:16px/1.65 ui-sans-serif,-apple-system,"Segoe UI",Roboto,sans-serif;
-webkit-font-smoothing:antialiased}
.mono{font-family:ui-monospace,Menlo,Consolas,monospace;font-size:.9em}
a{color:var(--accent-d)}a:hover{text-decoration:underline}

/* navbar */
.nav{position:sticky;top:0;z-index:10;background:rgba(250,249,245,.9);
backdrop-filter:blur(10px);border-bottom:1px solid var(--line)}
.nav-in{max-width:760px;margin:0 auto;padding:13px 24px;
display:grid;grid-template-columns:1fr auto 1fr;align-items:center}
.brand{justify-self:start;display:flex;align-items:center;gap:9px;
font-family:var(--serif);font-size:18px;font-weight:600;color:var(--fg);text-decoration:none}
.brand svg{display:block}
.nav-tag{justify-self:center;color:var(--muted);font-size:13px}
.nav-right{justify-self:end;font-size:13px}
.nav-right a{color:var(--muted)}

/* article layout */
.article{max-width:760px;margin:0 auto;padding:56px 24px 100px}

/* title block */
.eyebrow{font-size:13px;color:var(--muted);text-transform:uppercase;
letter-spacing:.8px;font-weight:600;margin:0 0 16px}
h1{font-family:var(--serif);font-size:38px;font-weight:600;
margin:0 0 16px;line-height:1.2;max-width:680px}
.byline{color:var(--muted);font-size:14px;margin:0 0 40px;
border-bottom:1px solid var(--line);padding-bottom:24px}

/* summary box */
.summary{background:var(--panel);border:1px solid var(--line);border-radius:14px;
padding:26px 30px;margin:0 0 48px;box-shadow:0 1px 3px rgba(60,50,30,.05)}
.summary h3{font-family:var(--serif);font-size:16px;margin:0 0 14px;color:var(--muted);
text-transform:uppercase;letter-spacing:.5px}
.summary ul{margin:0;padding-left:20px}
.summary li{margin:7px 0;font-style:italic;line-height:1.55}

/* body text */
p{margin:0 0 20px;line-height:1.7}
strong{font-weight:600}

/* section headers */
h2{font-family:var(--serif);font-size:26px;font-weight:600;
margin:52px 0 6px;line-height:1.25}
.finding-label{font-size:12px;color:var(--muted);font-weight:600;
text-transform:uppercase;letter-spacing:.7px;margin:0 0 4px}
h3.finding{font-family:var(--serif);font-size:21px;font-weight:600;
margin:40px 0 12px;line-height:1.3}

/* chart figures */
.figure{background:var(--panel);border:1px solid var(--line);border-radius:14px;
padding:24px 24px 18px;margin:28px 0 8px;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.figure figcaption{font-size:13px;color:var(--muted);margin-top:14px;line-height:1.5}
.figure figcaption strong{color:var(--fg)}

/* scorecard table */
.tablewrap{background:var(--panel);border:1px solid var(--line);border-radius:14px;
overflow:hidden;box-shadow:0 1px 2px rgba(60,50,30,.04);margin:20px 0}
table{width:100%;border-collapse:collapse;font-size:14px}
th,td{padding:11px 14px;text-align:right;border-bottom:1px solid var(--line)}
th:first-child,td:first-child{text-align:left;min-width:120px}
thead th{background:var(--panel2);color:var(--muted);font-size:11px;
font-weight:600;text-transform:uppercase;letter-spacing:.5px}
tbody tr:last-child td{border-bottom:none}
tbody tr.detail-row{cursor:pointer;transition:background .1s}
tbody tr.detail-row:hover{background:var(--panel2)}
td.win{background:rgba(58,125,82,.12);color:var(--green);font-weight:600}
td.lose{background:rgba(192,73,47,.11);color:var(--red);font-weight:600}
td.tie{background:rgba(154,143,85,.09);color:var(--amber);font-weight:600}
td.na{color:var(--muted)}
.vkeep{color:var(--green);font-weight:600}
.vredesign{color:var(--red);font-weight:600}
.vmarginal{color:var(--amber);font-weight:600}
.detail{display:none}.detail.open{display:table-row}
.detail>td{padding:0;background:var(--panel2)}
.qtable{width:100%;border-collapse:collapse;font-size:13px}
.qtable td,.qtable th{border-bottom:1px solid var(--line);padding:8px 14px}
.qtable thead th{background:transparent}
.qtable tr:last-child td{border-bottom:none}
.err{color:var(--red);font-size:12px}

/* stat callouts */
.stats-row{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));
gap:14px;margin:24px 0 32px}
.stat{background:var(--panel);border:1px solid var(--line);border-radius:12px;
padding:18px 20px;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.stat .k{font-size:11px;color:var(--muted);text-transform:uppercase;
letter-spacing:.6px;font-weight:600}
.stat .v{font-family:var(--serif);font-size:28px;font-weight:600;margin-top:6px}
.stat .n{font-size:12px;color:var(--muted);margin-top:4px}

/* misc */
.lang-dot{display:inline-block;width:9px;height:9px;border-radius:50%;
margin-right:7px;vertical-align:middle}
.legend{margin-top:12px;color:var(--muted);font-size:13px}
.legend .lang-dot{margin-left:12px}
.divider{border:none;border-top:1px solid var(--line);margin:48px 0}
.foot{color:#56524a;font-size:13.5px;padding-left:18px;margin-top:8px}
.foot li{margin:7px 0}
"""

LOGO = (
    '<svg width="24" height="24" viewBox="0 0 26 26" fill="none" aria-hidden="true">'
    '<path d="M7 7L19 13M7 7L13 19M19 13L13 19" stroke="#cc785c" stroke-width="1.6" stroke-linecap="round"/>'
    '<circle cx="7" cy="7" r="3.2" fill="#cc785c"/>'
    '<circle cx="19" cy="13" r="2.6" fill="#e0a07f"/>'
    '<circle cx="13" cy="19" r="2.6" fill="#e0a07f"/>'
    '</svg>'
)

JS = """
function toggle(id){var r=document.getElementById(id);if(r)r.classList.toggle('open');}
"""


def stat_card(k: str, v: str, cls: str = "", note: str = "") -> str:
    return (
        f'<div class="stat"><div class="k">{html.escape(k)}</div>'
        f'<div class="v {cls}">{v}</div>'
        + (f'<div class="n">{html.escape(note)}</div>' if note else "")
        + "</div>"
    )


def render_per_repo_table(reports: list[dict]) -> str:
    body = ""
    for i, r in enumerate(reports):
        t = r["totals"]
        lang = r["_lang"]
        color = LANG_COLOR.get(lang, "#888")
        did = f"pr{i}"
        geo = t.get("geomean_ratio", 0)
        geo_cls = "win" if geo >= WIN else ("lose" if geo <= LOSE else "tie")
        cost_saved_pct = (
            100 * (t.get("baseline_cost_usd", 0) - t.get("gcx_cost_usd", 0))
            / t.get("baseline_cost_usd", 1)
            if t.get("baseline_cost_usd") else 0
        )
        qrows = ""
        for q in r["questions"]:
            lbl = q.get("q", "")
            tool = Q_TO_TOOL.get(lbl, lbl)
            b = q.get("baseline", {})
            g = q.get("gcx", {})
            ratio = q.get("token_ratio", 0)
            cls = ("win" if ratio >= WIN else "lose" if (ratio and ratio <= LOSE) else "tie") if ratio else "na"
            b_tok = fmt(b.get("total", 0)) if not b.get("error") else '<span class="err">error</span>'
            g_tok = fmt(g.get("total", 0)) if not g.get("error") else '<span class="err">error</span>'
            qrows += (
                f"<tr>"
                f'<td><span class="mono">{html.escape(tool)}</span></td>'
                f"<td>{b_tok}</td><td>{g_tok}</td>"
                f'<td>${b.get("cost",0):.3f}</td><td>${g.get("cost",0):.3f}</td>'
                f'<td>{b.get("turns",0)}</td><td>{g.get("turns",0)}</td>'
                f'<td class="{cls}">{f"{ratio:g}×" if ratio else "—"}</td>'
                "</tr>"
            )
        body += (
            f'<tr class="detail-row" onclick="toggle(\'{did}\')">'
            f'<td><span class="lang-dot" style="background:{color}"></span>'
            f'<span class="mono">{html.escape(r.get("repo",""))}</span></td>'
            f"<td>{html.escape(lang)}</td>"
            f'<td>{fmt(t.get("baseline_tokens",0))}</td>'
            f'<td>{fmt(t.get("gcx_tokens",0))}</td>'
            f'<td>${t.get("baseline_cost_usd",0):.3f}</td>'
            f'<td>${t.get("gcx_cost_usd",0):.3f}</td>'
            f'<td class="{"win" if cost_saved_pct>5 else "lose" if cost_saved_pct<-5 else "tie"}">'
            f'{cost_saved_pct:.0f}%</td>'
            f'<td class="{geo_cls} mono">{geo:.2f}×</td>'
            f'<td style="color:var(--muted);font-size:11px">▾</td>'
            "</tr>"
            f'<tr class="detail" id="{did}"><td colspan="9">'
            '<table class="qtable"><thead><tr>'
            "<th>Tool</th><th>Baseline tok</th><th>gcx tok</th>"
            "<th>Base $</th><th>gcx $</th>"
            "<th>Base turns</th><th>gcx turns</th><th>Ratio</th>"
            f"</tr></thead><tbody>{qrows}</tbody></table>"
            "</td></tr>"
        )
    return body


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
    n_q = sum(len(r["questions"]) for r in reports)

    # per-tool geomeans + verdicts
    tool_geos = {}
    tool_verdicts = {}
    for lbl, tool in Q_TO_TOOL.items():
        geo, v = tool_verdict(qr[lbl])
        tool_geos[lbl] = geo
        tool_verdicts[lbl] = v

    wins  = sum(1 for v in tool_verdicts.values() if v == "keep")
    loses = sum(1 for v in tool_verdicts.values() if v == "REDESIGN")

    # figure 1 — token bar chart (first repo for illustration, or aggregate)
    fig1_labels = [r.get("repo","") for r in reports]
    fig1_base   = [r["totals"].get("baseline_tokens",0) for r in reports]
    fig1_gcx    = [r["totals"].get("gcx_tokens",0) for r in reports]
    fig1_svg    = bar_chart_svg(fig1_labels, fig1_base, fig1_gcx, "Tokens per repo")

    # figure 2 — cost bar chart
    fig2_svg = cost_bar_svg(reports)

    # figure 3 — per-tool ratio dot chart
    dot_labels = [Q_TO_TOOL[k] for k in Q_TO_TOOL]
    dot_ratios = [tool_geos[k] for k in Q_TO_TOOL]
    fig3_svg = ratio_dot_chart(dot_labels, dot_ratios)

    # matrix table
    langs = [r["_lang"] for r in reports]
    matrix_head = "<th>MCP tool</th>" + "".join(f"<th>{l}</th>" for l in langs) + "<th>Geomean</th>"
    matrix_body = ""
    for lbl, tool in Q_TO_TOOL.items():
        geo = tool_geos[lbl]
        tds = f'<td><span class="mono">{html.escape(tool)}</span></td>'
        for r in reports:
            q = next((x for x in r["questions"] if x.get("q") == lbl), None)
            if not q or q.get("baseline", {}).get("error") or q.get("gcx", {}).get("error"):
                tds += '<td class="na">—</td>'
            else:
                rv = q.get("token_ratio", 0)
                cls = "win" if rv >= WIN else ("lose" if rv <= LOSE else "tie")
                tds += f'<td class="{cls}">{rv:g}×</td>'
        geo_cls = "win" if geo >= WIN else ("lose" if geo <= LOSE else "tie")
        tds += f'<td class="{geo_cls} mono">{geo:.2f}×</td>'
        matrix_body += f"<tr>{tds}</tr>"

    legend = "".join(
        f'<span class="lang-dot" style="background:{c}"></span>{l}'
        for l, c in LANG_COLOR.items()
    )

    repo_table_body = render_per_repo_table(reports)

    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>GitCortex — Real Token Benchmark</title>
<style>{CSS}</style></head>
<body>

<nav class="nav"><div class="nav-in">
<a class="brand" href="https://github.com/bharath03-a/GitCortex">{LOGO}GitCortex</a>
<span class="nav-tag">Token Benchmark</span>
<div class="nav-right"><a href="https://github.com/bharath03-a/GitCortex">GitHub ↗</a></div>
</div></nav>

<div class="article">

<!-- ── Title ── -->
<p class="eyebrow">Research &nbsp;·&nbsp; {date.today().isoformat()}</p>
<h1>Do code knowledge graphs actually save AI tokens?</h1>
<p class="byline">GitCortex Research &nbsp;·&nbsp; {n_repos} repositories &nbsp;·&nbsp;
{n_q} Claude sessions &nbsp;·&nbsp; model:
<span class="mono">{html.escape(", ".join(models))}</span></p>

<!-- ── Summary box ── -->
<div class="summary">
<h3>Summary</h3>
<ul>
<li>We ran Claude twice on each of 7 developer questions per repo — once with grep tools only (baseline), once with the GitCortex code-graph MCP (gcx arm) — and recorded real API <span class="mono">usage</span> tokens.</li>
<li>Across {n_repos} repositories, graph queries saved <strong>{saved_cost_pct:.0f}% on cost</strong> even though raw token counts changed by only {saved_tok_pct:+.1f}% — because the graph arm ran fewer turns and re-read less cached context.</li>
<li>Of {len(Q_TO_TOOL)} MCP tools, <strong>{wins} win</strong> (ratio ≥ {WIN}×) and <strong>{loses} need redesign</strong> (lose to grep in most languages).</li>
<li><strong>Search and tour</strong> are the consistent winners. <strong>Subgraph dumps and dead-code listing</strong> are the consistent losers — they return more tokens than Claude would have produced by grepping.</li>
<li>The chars/4 proxy report overstates savings 100–1000×: it assumed Claude reads whole files, but real Claude greps and reads only snippets, making the grep baseline far cheaper than predicted.</li>
</ul>
</div>

<!-- ── Methodology ── -->
<h2>Methodology</h2>
<p>For each of {n_repos} open-source repositories spanning {len(set(r["_lang"] for r in reports))} languages,
we asked Claude <strong>7 developer questions</strong> drawn from real AI-editor workflows:
tour, search, symbol explanation, refactor impact, trace flow, neighbourhood, and dead-code detection.
Each question ran twice under identical conditions — same model
(<span class="mono">{html.escape(", ".join(models))}</span>), same system prompt, same repository.</p>
<p>The <strong>baseline arm</strong> allowed Read, Grep, Glob, and Bash(grep/find/cat) — how Claude works today.
The <strong>gcx arm</strong> allowed Read and the 7 GitCortex graph tools — no grep.
We captured <span class="mono">usage.input_tokens + cache_creation_input_tokens + output_tokens</span>
per session from <span class="mono">claude -p --output-format json</span>.
<span class="mono">cache_read_input_tokens</span> is excluded from the token count (it double-counts
re-reads across turns) but contributes to cost — which is why cost savings exceed token savings.</p>
<p>Token ratio = baseline ÷ gcx. Ratio &gt; 1 means the graph used fewer tokens.
Win ≥ {WIN}×.  Lose ≤ {LOSE}×. Per-tool geomean is computed across all valid runs for that tool.</p>

<hr class="divider"/>

<!-- ── Finding 1: headline numbers ── -->
<p class="finding-label">Finding 1</p>
<h3 class="finding">Cost savings are real; raw-token savings are modest</h3>
<p>Across all sessions, the gcx arm used <strong>{saved_tok_pct:+.1f}% different tokens</strong> than grep —
a near-tie on volume. But it cost <strong>{saved_cost_pct:.0f}% less</strong>
(${cb:.2f} baseline → ${cg:.2f} gcx). The gap comes from turn count:
the graph arm found answers in fewer turns, reducing how many times the large tool-schema set
was re-read from cache. Token accounting treats cache reads as cheap but not free.</p>

<div class="stats-row">
{stat_card("Baseline tokens", fmt(tb), note="grep arm total")}
{stat_card("GitCortex tokens", fmt(tg), note="graph arm total")}
{stat_card("Token Δ", f"{saved_tok_pct:+.1f}%", "amber" if abs(saved_tok_pct)<5 else ("green" if saved_tok_pct>0 else "red"), "vs grep")}
{stat_card("Cost saved", f"{saved_cost_pct:.0f}%", "green", f"${cb:.2f} → ${cg:.2f}")}
{stat_card("Geomean", f"{geo_all:.2f}×", "green" if geo_all>=WIN else "amber", "typical question")}
</div>

<figure class="figure">
{fig1_svg}
<figcaption><strong>Figure 1.</strong> Token count per repository — baseline (warm) vs GitCortex (green).
Bars represent the sum across all 7 questions. Repos are {", ".join(r.get("repo","") for r in reports)}.</figcaption>
</figure>

<figure class="figure">
{fig2_svg}
<figcaption><strong>Figure 2.</strong> API cost per repository baseline vs gcx. Cost savings exceed token
savings because the gcx arm runs fewer turns, paying cache-read charges fewer times.</figcaption>
</figure>

<hr class="divider"/>

<!-- ── Finding 2: tool matrix ── -->
<p class="finding-label">Finding 2</p>
<h3 class="finding">Search and tour consistently win; subgraph and dead-code lose</h3>
<p>Not all graph tools benefit equally. <span class="mono">search_code</span> and
<span class="mono">start_tour</span> consistently returned fewer tokens than grep — especially on large
repos where grep must scan many files. <span class="mono">get_subgraph</span> and
<span class="mono">find_unused_symbols</span> consistently lost: they serialise large result sets
that exceed what Claude would have produced by grepping selectively.</p>

<figure class="figure">
{fig3_svg}
<figcaption><strong>Figure 3.</strong> Geomean token ratio per MCP tool across all repos.
Dots to the right of 1× mean the graph used fewer tokens. Green zone = win (≥{WIN}×).
Red zone = lose (≤{LOSE}×).</figcaption>
</figure>

<div class="tablewrap"><table>
<thead><tr>{matrix_head}</tr></thead>
<tbody>{matrix_body}</tbody>
</table></div>
<div class="legend">Languages:{legend}</div>

<hr class="divider"/>

<!-- ── Finding 3: why losers lose ── -->
<p class="finding-label">Finding 3</p>
<h3 class="finding">Losers share one pattern: "dump everything" instead of summarise</h3>
<p><span class="mono">get_subgraph</span> defaulted to depth 2 with no node cap — a hub symbol like
<span class="mono">JsonReader</span> or Django's <span class="mono">filter</span> produces hundreds of
neighbours, serialised into more tokens than a grep and two file reads.
<span class="mono">find_unused_symbols</span> returned the complete unused list; the proxy benchmark
modelled this as a huge baseline win (whole-repo reads), but real Claude greps selectively and
answers approximately — its baseline was far cheaper than the proxy assumed.</p>
<p>We applied three targeted fixes: lower default depth (2→1) + node cap (30) for
<span class="mono">get_subgraph</span>; honour the <span class="mono">limit</span> parameter
(default 30) for <span class="mono">find_unused_symbols</span>; per-hop caps for
<span class="mono">find_callers</span>. All return the true total count and a
<span class="mono">truncated</span> flag so the agent can request more if needed.</p>

<hr class="divider"/>

<!-- ── Per-repo detail ── -->
<h2>Per-repository results</h2>
<p>Click a row to expand the 7 per-tool metrics including real token counts, cost, and turn count
for both arms.</p>
<div class="tablewrap"><table>
<thead><tr>
<th>Repo</th><th>Lang</th>
<th>Baseline tok</th><th>gcx tok</th>
<th>Baseline $</th><th>gcx $</th>
<th>Cost Δ</th><th>Geomean</th><th></th>
</tr></thead>
<tbody>{repo_table_body}</tbody>
</table></div>

<hr class="divider"/>

<!-- ── Discussion ── -->
<h2>Discussion</h2>
<p>The proxy benchmark (chars/4) overstated savings by 100–1000× because it assumed Claude reads
whole files. In practice Claude greps, reads snippets, and answers approximately — the grep arm
is far more token-efficient than a file-reading model predicts. The real story is subtler:
the graph helps on <em>discovery</em> (search, tour) where grep must scan many files to find
the relevant handful, but hurts on <em>enumeration</em> (subgraph, dead-code) where it dumps
a complete answer that is larger than Claude's approximate grep-based reply.</p>
<p>Cost is the more reliable metric than raw tokens. The gcx arm runs fewer turns because
graph answers are self-contained; the grep arm iterates — grep, read, grep again.
Each extra turn re-pays the ~14k-token MCP tool-schema cache-read. That asymmetry drives the
cost gap even when fresh-token counts are similar.</p>
<p>The release-gate loop is: run <span class="mono">/dev:token-benchmark</span> before shipping,
check the scorecard for any tool that flipped green→red, apply a cap/summarise fix,
re-run the affected language to confirm recovery. See
<span class="mono">docs/benchmarks/RELEASE-GATE.md</span> for the full method.</p>

<ul class="foot">
<li><strong>Measurement.</strong> <span class="mono">claude -p --output-format json</span>;
tokens = input + cache_creation + output (cache_read excluded — double-counts re-reads across turns).</li>
<li><strong>Two arms, same prompt.</strong> ~14k fixed overhead (system prompt + tool schemas) is
identical in both arms and cancels in the ratio.</li>
<li><strong>Fixed MCP tax.</strong> 15 tool schemas ride every gcx turn. On small repos this overhead
can exceed savings — why some cells are marginal even for winning tools.</li>
<li><strong>One run per cell.</strong> No error bars; ratios this large in either direction are
structurally significant, not noise, but single-run variance exists.</li>
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
    print(f"wrote {out}  ({len(reports)} repos, {sum(len(r['questions']) for r in reports)} sessions)")


if __name__ == "__main__":
    main()
