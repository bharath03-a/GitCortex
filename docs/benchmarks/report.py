#!/usr/bin/env python3
"""Render the GitCortex token-savings benchmark as a self-contained HTML report.

Reads every ``dev-*.json`` produced by ``dev-harness.sh`` in a directory and
writes ``report.html`` next to them. No third-party deps — open the file in any
browser, no server needed.

Usage:
    python3 report.py [bench_dir] [-o out.html]

Tracked per repo and per question:
    - baseline tokens   (raw grep + cat an LLM would burn)
    - gcx tokens        (single graph query output)
    - tokens saved + %  (baseline - gcx)
    - files read        (grep-fed cat calls the graph replaces)
    - gcx calls         (always 1 per question)
    - ratio / geomean   (token compression factor)
"""
from __future__ import annotations

import argparse
import glob
import html
import json
import math
import os
from datetime import date

# Known sweep repos → language label. Unknown repos fall back to "—".
REPO_LANG = {
    "ripgrep": "Rust",
    "tokio": "Rust",
    "serde": "Rust",
    "cobra": "Go",
    "gin": "Go",
    "zap": "Go",
    "hono": "TypeScript",
    "zod": "TypeScript",
    "io-ts": "TypeScript",
    "django": "Python",
    "requests": "Python",
    "flask": "Python",
    "fastapi": "Python",
    "gson": "Java",
    "picocli": "Java",
    "jjwt": "Java",
}

LANG_COLOR = {
    "Rust": "#dea584",
    "Go": "#00add8",
    "TypeScript": "#3178c6",
    "Python": "#ffd343",
    "Java": "#f89820",
    "—": "#888",
}


def load_reports(bench_dir: str) -> list[dict]:
    """Load and validate every dev-*.json in bench_dir, skipping clone errors."""
    reports = []
    for path in sorted(glob.glob(os.path.join(bench_dir, "dev-*.json"))):
        try:
            with open(path, encoding="utf-8") as fh:
                data = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            print(f"skip {os.path.basename(path)}: {exc}")
            continue
        if data.get("error"):
            print(f"skip {os.path.basename(path)}: {data['error']}")
            continue
        if "totals" not in data or "questions" not in data:
            print(f"skip {os.path.basename(path)}: missing totals/questions")
            continue
        data["_lang"] = REPO_LANG.get(data.get("repo", ""), "—")
        reports.append(data)
    return reports


def geomean(values: list[float]) -> float:
    """Geometric mean of positive values; 0 if none qualify."""
    logs = [math.log(v) for v in values if v and v > 0]
    return math.exp(sum(logs) / len(logs)) if logs else 0.0


def fmt(n: float) -> str:
    """Thousands-separated integer string."""
    return f"{int(round(n)):,}"


def pct(part: float, whole: float) -> float:
    return 100.0 * part / whole if whole else 0.0


def aggregate(reports: list[dict]) -> dict:
    """Sum absolute counts across repos; geomean the per-repo geomeans."""
    base = sum(r["totals"].get("baseline_tokens", 0) for r in reports)
    gcx = sum(r["totals"].get("gcx_tokens", 0) for r in reports)
    reads = sum(r["totals"].get("files_read", 0) for r in reports)
    calls = sum(r["totals"].get("gcx_calls", 0) for r in reports)
    geos = [r["totals"].get("geomean_ratio", 0) for r in reports]
    return {
        "repos": len(reports),
        "baseline_tokens": base,
        "gcx_tokens": gcx,
        "saved_tokens": base - gcx,
        "saved_pct": pct(base - gcx, base),
        "files_read": reads,
        "gcx_calls": calls,
        "calls_saved": reads - calls,
        "geomean": geomean(geos),
    }


# ── HTML rendering ──────────────────────────────────────────────────────────

CSS = """
:root{--bg:#faf9f5;--panel:#ffffff;--panel2:#f4f2ec;--line:#e7e2d6;
--fg:#1f1d1a;--muted:#76726a;--accent:#cc785c;--accent-d:#b35f44;
--blue:#5b6f8c;--green:#3a7d52;--warn:#b35f44;
--serif:Georgia,"Times New Roman",ui-serif,serif}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);
font:15px/1.6 ui-sans-serif,-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
-webkit-font-smoothing:antialiased}
.mono{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.92em}
a{color:var(--accent-d);text-decoration:none}a:hover{text-decoration:underline}

/* navbar */
.nav{position:sticky;top:0;z-index:10;background:rgba(250,249,245,.85);
backdrop-filter:saturate(150%) blur(8px);border-bottom:1px solid var(--line)}
.nav-in{max-width:1080px;margin:0 auto;padding:14px 24px;
display:grid;grid-template-columns:1fr auto 1fr;align-items:center}
.brand{justify-self:start;display:flex;align-items:center;gap:10px;
font-family:var(--serif);font-size:19px;font-weight:600;color:var(--fg)}
.brand:hover{text-decoration:none}
.brand svg{display:block}
.nav-tag{justify-self:center;color:var(--muted);font-size:14px;
letter-spacing:.2px}
.nav-right{justify-self:end;display:flex;align-items:center;gap:18px;font-size:14px}
.nav-right a{color:var(--muted)}.nav-right a:hover{color:var(--fg)}
@media(max-width:640px){.nav-tag{display:none}}

.wrap{max-width:1080px;margin:0 auto;padding:48px 24px 90px}
h1{font-family:var(--serif);font-size:38px;font-weight:600;
margin:0 0 10px;letter-spacing:-.5px;line-height:1.15}
h2{font-family:var(--serif);font-size:24px;font-weight:600;
margin:56px 0 16px;padding-bottom:10px;border-bottom:1px solid var(--line)}
.lede{font-size:17px;color:#46433d;max-width:680px;margin:0 0 6px}
.sub{color:var(--muted);margin:0 0 4px;max-width:720px;font-size:14px}

.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(178px,1fr));
gap:14px;margin-top:32px}
.card{background:var(--panel);border:1px solid var(--line);border-radius:14px;
padding:20px 22px;box-shadow:0 1px 2px rgba(60,50,30,.04)}
.card .k{color:var(--muted);font-size:11px;text-transform:uppercase;
letter-spacing:.7px;font-weight:600}
.card .v{font-family:var(--serif);font-size:30px;font-weight:600;
margin-top:8px;line-height:1.1}
.card .v.green{color:var(--green)}.card .v.blue{color:var(--blue)}
.card .v.warn{color:var(--warn)}.card .v.accent{color:var(--accent)}
.card .note{color:var(--muted);font-size:12px;margin-top:6px}

.tablewrap{background:var(--panel);border:1px solid var(--line);
border-radius:14px;overflow:hidden;box-shadow:0 1px 2px rgba(60,50,30,.04)}
table{width:100%;border-collapse:collapse;font-size:14px}
th,td{padding:12px 14px;text-align:right;border-bottom:1px solid var(--line)}
th:first-child,td:first-child{text-align:left}
thead th{background:var(--panel2);color:var(--muted);font-weight:600;
font-size:11px;text-transform:uppercase;letter-spacing:.5px}
tbody tr.repo{cursor:pointer;transition:background .12s}
tbody tr.repo:hover{background:var(--panel2)}
tbody tr.repo:last-child td{border-bottom:none}
.lang{display:inline-block;width:9px;height:9px;border-radius:50%;
margin-right:9px;vertical-align:middle}
.detail{display:none}
.detail.open{display:table-row}
.detail>td{padding:0;background:var(--panel2)}
.qtable{width:100%;border-collapse:collapse;font-size:13px}
.qtable td,.qtable th{border-bottom:1px solid var(--line);padding:9px 14px}
.qtable thead th{background:transparent}
.qtable tr:last-child td{border-bottom:none}
.bar{position:relative;height:16px;background:#ece8dd;border-radius:8px;
overflow:hidden;min-width:80px}
.bar>span{position:absolute;left:0;top:0;bottom:0;
background:linear-gradient(90deg,#e0a07f,var(--accent));border-radius:8px}
.g{color:var(--green);font-weight:500}.muted{color:var(--muted)}
.legend{margin-top:16px;color:var(--muted);font-size:13px}
.legend .lang{margin-left:14px}
.foot{margin-top:8px;color:#56524a;font-size:13.5px;padding-left:18px}
.foot li{margin:6px 0}
"""

JS = """
function toggle(id){var r=document.getElementById(id);if(r)r.classList.toggle('open');}
"""

# Inline brand mark: three connected graph nodes in Claude-clay coral.
LOGO = (
    '<svg width="26" height="26" viewBox="0 0 26 26" fill="none" '
    'aria-hidden="true">'
    '<path d="M7 7L19 13M7 7L13 19M19 13L13 19" stroke="#cc785c" '
    'stroke-width="1.6" stroke-linecap="round"/>'
    '<circle cx="7" cy="7" r="3.2" fill="#cc785c"/>'
    '<circle cx="19" cy="13" r="2.6" fill="#e0a07f"/>'
    '<circle cx="13" cy="19" r="2.6" fill="#e0a07f"/>'
    "</svg>"
)


def render_question_row(q: dict) -> str:
    base = q.get("baseline_tokens", 0)
    gcx = q.get("gcx_tokens", 0)
    ratio = q.get("ratio", 0)
    reads = q.get("files_read")
    # Bar width on a log scale so 6× and 14000× both stay legible.
    width = min(100, 14 + 22 * math.log10(ratio)) if ratio and ratio > 0 else 2
    reads_cell = (
        f'{fmt(reads)} <span class="muted">→ 1</span>' if reads is not None else "—"
    )
    return (
        "<tr>"
        f'<td>{html.escape(str(q.get("question", q.get("q", ""))))}</td>'
        f"<td>{fmt(base)}</td>"
        f'<td class="g">{fmt(gcx)}</td>'
        f"<td>{reads_cell}</td>"
        f'<td><div class="bar"><span style="width:{width:.0f}%"></span></div></td>'
        f'<td class="mono">{ratio:g}×</td>'
        "</tr>"
    )


def render_repo(idx: int, r: dict) -> str:
    t = r["totals"]
    lang = r["_lang"]
    color = LANG_COLOR.get(lang, "#888")
    reads = t.get("files_read")
    calls = t.get("gcx_calls")
    reads_cell = (
        f"{fmt(reads)} <span class='muted'>→ {fmt(calls)}</span>"
        if reads is not None
        else "—"
    )
    qrows = "".join(render_question_row(q) for q in r["questions"])
    did = f"d{idx}"
    return (
        f'<tr class="repo" onclick="toggle(\'{did}\')">'
        f'<td><span class="lang" style="background:{color}"></span>'
        f'<span class="mono">{html.escape(r.get("repo",""))}</span></td>'
        f"<td>{html.escape(lang)}</td>"
        f'<td>{fmt(r.get("nodes",0))}</td>'
        f'<td>{fmt(r.get("edges",0))}</td>'
        f'<td>{fmt(t.get("baseline_tokens",0))}</td>'
        f'<td class="g">{fmt(t.get("gcx_tokens",0))}</td>'
        f'<td class="g">{fmt(t.get("saved_tokens",0))}</td>'
        f'<td>{t.get("saved_pct",0):.1f}%</td>'
        f"<td>{reads_cell}</td>"
        f'<td class="mono">{t.get("geomean_ratio",0):g}×</td>'
        "</tr>"
        f'<tr class="detail" id="{did}"><td colspan="10">'
        '<table class="qtable"><thead><tr>'
        "<th>Question</th><th>Baseline tok</th><th>gcx tok</th>"
        "<th>Reads→calls</th><th>Compression</th><th>Ratio</th>"
        f"</tr></thead><tbody>{qrows}</tbody></table>"
        "</td></tr>"
    )


def worst_cases(reports: list[dict], n: int = 6) -> list[tuple]:
    """Lowest-ratio questions across all repos — where the graph helps least."""
    rows = []
    for r in reports:
        for q in r["questions"]:
            rows.append(
                (r.get("repo", ""), r["_lang"], q.get("question", q.get("q", "")),
                 q.get("baseline_tokens", 0), q.get("gcx_tokens", 0),
                 q.get("ratio", 0))
            )
    rows.sort(key=lambda x: x[5])
    return rows[:n]


def render_worst(reports: list[dict]) -> str:
    body = ""
    for repo, lang, q, base, gcx, ratio in worst_cases(reports):
        color = LANG_COLOR.get(lang, "#888")
        loses = ratio and ratio < 1
        cls = "warn" if loses else "muted"
        tag = " <span class='warn'>(worse than grep)</span>" if loses else ""
        body += (
            "<tr>"
            f'<td><span class="lang" style="background:{color}"></span>'
            f'<span class="mono">{html.escape(repo)}</span></td>'
            f"<td>{html.escape(q)}{tag}</td>"
            f"<td>{fmt(base)}</td>"
            f'<td>{fmt(gcx)}</td>'
            f'<td class="mono {cls}">{ratio:g}×</td>'
            "</tr>"
        )
    return body


def card(k: str, v: str, cls: str = "", note: str = "") -> str:
    note_html = f'<div class="note">{html.escape(note)}</div>' if note else ""
    return (
        f'<div class="card"><div class="k">{html.escape(k)}</div>'
        f'<div class="v {cls}">{v}</div>{note_html}</div>'
    )


def render(reports: list[dict]) -> str:
    agg = aggregate(reports)
    meta = reports[0] if reports else {}
    branch = meta.get("branch", "—")
    legend = "".join(
        f'<span class="lang" style="background:{c}"></span>{l}'
        for l, c in LANG_COLOR.items()
        if l != "—"
    )
    cards = "".join(
        [
            card("Repos", fmt(agg["repos"]), note="one per language"),
            card("Baseline tokens", fmt(agg["baseline_tokens"]), "warn",
                 "raw grep + cat"),
            card("GitCortex tokens", fmt(agg["gcx_tokens"]), "blue",
                 "graph queries"),
            card("Tokens saved", fmt(agg["saved_tokens"]), "green",
                 f"{agg['saved_pct']:.1f}% less context"),
            card("File reads saved", fmt(agg["calls_saved"]), "green",
                 f"{fmt(agg['files_read'])} reads → {fmt(agg['gcx_calls'])} calls"),
            card("Geomean", f"{agg['geomean']:.0f}×", "accent",
                 "typical compression"),
        ]
    )
    repo_rows = "".join(render_repo(i, r) for i, r in enumerate(reports))
    worst_rows = render_worst(reports)
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>GitCortex — Token-Savings Benchmark</title>
<style>{CSS}</style></head>
<body>
<nav class="nav"><div class="nav-in">
<a class="brand" href="https://github.com/bharath03-a/GitCortex">{LOGO}GitCortex</a>
<span class="nav-tag">Token-Savings Benchmark</span>
<div class="nav-right">
<a href="https://github.com/bharath03-a/GitCortex">GitHub ↗</a>
</div></div></nav>
<div class="wrap">
<h1>How much context does a code graph save?</h1>
<p class="lede">GitCortex answers codebase questions with a single graph query
instead of grepping and reading raw files. Across {agg['repos']} repositories
and 7 developer questions each, that swap cut retrieval context by
<b>{agg['saved_pct']:.0f}%</b>.</p>
<p class="sub">Generated {date.today().isoformat()} · branch
<span class="mono">{html.escape(branch)}</span> ·
baseline = files an LLM would <span class="mono">grep</span> +
<span class="mono">cat</span> by hand · token proxy = chars / 4.</p>

<div class="cards">{cards}</div>

<h2>Per-repository results</h2>
<p class="sub">Click a row to expand the 7 per-question metrics. Geomean is the
typical per-question token compression; sums are the whole-session budget.</p>
<div class="tablewrap"><table><thead><tr>
<th>Repo</th><th>Lang</th><th>Nodes</th><th>Edges</th>
<th>Baseline tok</th><th>gcx tok</th><th>Saved tok</th><th>Saved %</th>
<th>Reads→calls</th><th>Geomean</th>
</tr></thead><tbody>{repo_rows}</tbody></table></div>
<div class="legend">Languages:{legend}</div>

<h2>Where GitCortex loses</h2>
<p class="sub">The graph is not a universal win. These are the lowest-ratio
questions in the run. The pattern is consistent:
<b>2-hop neighborhood dumps on a high-degree node</b> — a hub with hundreds of
neighbors serialises into more tokens than reading the file itself. Giant
frameworks (Django) amplify it because common symbols fan out everywhere.
Lesson: use <span class="mono">find-callers</span> /
<span class="mono">trace-path</span> / <span class="mono">--depth 1</span> for
targeted questions; reach for plain grep when you genuinely want a broad sweep.</p>
<div class="tablewrap"><table><thead><tr>
<th>Repo</th><th>Question</th><th>Baseline tok</th><th>gcx tok</th><th>Ratio</th>
</tr></thead><tbody>{worst_rows}</tbody></table></div>

<h2>How to read this</h2>
<ul class="foot">
<li><b>Baseline tokens</b> — the context an LLM burns reading raw files via
grep + cat to answer the question manually.</li>
<li><b>gcx tokens</b> — the context the GitCortex graph query returns instead.</li>
<li><b>Reads → calls</b> — how many file reads the baseline needs vs. the single
graph query GitCortex runs. This is the "grep calls" the graph eliminates.</li>
<li><b>Ratio / Geomean</b> — token compression factor. Geomean (geometric mean)
is the typical question; half do better, half worse — robust to outliers like
trace-path (thousands×) and dead-code (whole-codebase reads).</li>
<li><b>These are not measured Claude tokens.</b> No LLM runs in this harness. It
is an offline estimate: baseline = chars/4 of the file content a grep + cat would
pull into context; gcx = chars/4 of the query output. A real benchmark would run
Claude both ways and diff the actual <span class="mono">usage.input_tokens</span>
the API reports. Treat these as directional, not ground truth.</li>
<li><b>Other caveats</b> — chars/4 proxy (±30% on absolutes, ratio order holds),
one run per repo, indexer cost (0.3–4 s first index) not counted.</li>
</ul>
</div><script>{JS}</script></body></html>"""


def main() -> None:
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bench_dir", nargs="?", default=here,
                    help="directory holding dev-*.json (default: script dir)")
    ap.add_argument("-o", "--out", default=None,
                    help="output html path (default: <bench_dir>/report.html)")
    args = ap.parse_args()

    reports = load_reports(args.bench_dir)
    if not reports:
        raise SystemExit(f"no usable dev-*.json found in {args.bench_dir}")

    out = args.out or os.path.join(args.bench_dir, "report.html")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(render(reports))
    agg = aggregate(reports)
    print(f"wrote {out}")
    print(f"  {agg['repos']} repos · saved {fmt(agg['saved_tokens'])} tokens "
          f"({agg['saved_pct']:.1f}%) · {fmt(agg['calls_saved'])} reads eliminated "
          f"· geomean {agg['geomean']:.0f}x")


if __name__ == "__main__":
    main()
