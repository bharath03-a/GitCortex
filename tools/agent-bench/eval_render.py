"""Render an eval summary as the section 4A/4B markdown tables."""

from __future__ import annotations

from typing import Any

DIM_TITLES = {
    "overall": "Overall",
    "repo": "By repo",
    "language": "By language",
    "workflow": "By workflow",
    "client": "By client",
    "model": "By model",
    "lane": "By lane",
}


def fmt_rate(value: dict[str, Any] | None) -> str:
    if not value or not value["den"]:
        return "n/a"
    return f"{value['num']}/{value['den']} ({value['rate'] * 100:.0f}%)"


def fmt_num(value: float | None, places: int = 1) -> str:
    return "n/a" if value is None else f"{value:.{places}f}"


def fmt_ratio(stats: dict[str, Any]) -> str:
    if stats["geomean"] is None:
        return "n/a"
    text = f"{stats['geomean']:.2f}x"
    if stats["ci95"]:
        text += f" [{stats['ci95'][0]:.2f}-{stats['ci95'][1]:.2f}]"
    return text


def _table(header: list[str], rows: list[list[str]]) -> list[str]:
    lines = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    return lines + ["| " + " | ".join(row) + " |" for row in rows]


def _each_group(summary: dict[str, Any]):
    for dim, title in DIM_TITLES.items():
        buckets = summary["groups"].get(dim, {})
        if buckets:
            yield title, buckets


def _provenance(summary: dict[str, Any]) -> list[str]:
    header = ["Trace", "Lane", "Client", "Model", "gcx", "Suite", "Rounds", "Runs", "Spent", "Harness"]
    rows = []
    for t in summary["traces"]:
        harness = t["harness"] or {}
        rows.append([
            t["label"], t["lane"], f"{t['client']} {t['client_version'] or ''}".strip(), str(t["model"]),
            f"{t['gcx_version']} `{(t['gcx_sha256'] or '')[:12]}`", f"`{(t['suite_sha256'] or '')[:12]}`",
            str(t["rounds"]), f"{t['completed_runs']}/{t['planned_runs']}",
            "n/a" if t["spent_usd"] is None else f"${t['spent_usd']:.2f}",
            f"`{(harness.get('commit') or '')[:8]}`{' dirty' if harness.get('dirty') else ''}",
        ])
    return _table(header, rows)


def _outcome(summary: dict[str, Any]) -> list[str]:
    out: list[str] = ["## 4A. Task outcome"]
    header = ["Group", "Arm", "Runs", "Success", "False-safe", "Uncached tok", "Cost/success", "Wall s", "Repeat reads", "Patch lines"]
    for title, buckets in _each_group(summary):
        rows = []
        for key, block in buckets.items():
            for arm, m in block["arms"].items():
                rows.append([
                    key, arm, str(m["runs"]), fmt_rate(m["success"]), str(m["false_safe"]),
                    fmt_num(m["uncached_tokens_mean"], 0),
                    "n/a" if m["cost_per_success"] is None else f"${m['cost_per_success']:.4f}",
                    fmt_num(m["wall_s_mean"]), fmt_num(m["repeat_reads_mean"], 2), fmt_num(m["patch_lines_mean"]),
                ])
        out += ["", f"### {title}", ""] + _table(header, rows)
    return out


def _paired(summary: dict[str, Any]) -> list[str]:
    out = ["## 4A. Paired against baseline (baseline/arm; above 1.0x means the arm used less)"]
    header = ["Group", "Arm", "Pairs", "Total tok", "Uncached tok", "Cost", "Success delta", "Non-inferior", "Rounds (stdev)"]
    for title, buckets in _each_group(summary):
        rows = []
        for key, block in buckets.items():
            for arm, p in block["paired"].items():
                r = p["ratios"]["uncached_tokens"]
                delta = p["success_delta"]
                rows.append([
                    key, arm, str(r["pairs"]), fmt_ratio(p["ratios"]["total_tokens"]), fmt_ratio(r),
                    fmt_ratio(p["ratios"]["cost_usd"]), "n/a" if delta is None else f"{delta:+.2f}",
                    {True: "yes", False: "NO", None: "n/a"}[p["non_inferior"]],
                    f"{r['rounds']} ({fmt_num(r['round_stdev'], 2)})",
                ])
        if rows:
            out += ["", f"### {title}", ""] + _table(header, rows)
    return out


def _discoverability(summary: dict[str, Any]) -> list[str]:
    out = ["## 4B. Discoverability"]
    header = ["Group", "Arm", "Adoption", "Right-tool", "Restraint", "1st call pos", "Uncached tok before 1st", "Error rate", "Not-ready ok", "Connected"]
    for title, buckets in _each_group(summary):
        rows = []
        for key, block in buckets.items():
            for arm, m in block["arms"].items():
                d = m["discoverability"]
                if d:
                    rows.append([
                        key, arm, fmt_rate(d["adoption"]), fmt_rate(d["right_tool"]), fmt_rate(d["restraint"]),
                        fmt_num(d["first_position_median"]), fmt_num(d["tokens_before_first_uncached_median"], 0),
                        fmt_rate(d["gcx_error_rate"]), fmt_rate(d["not_ready_correct"]), fmt_rate(d["connection_success"]),
                    ])
        if rows:
            out += ["", f"### {title}", ""] + _table(header, rows)
    return out


def _negative(summary: dict[str, Any]) -> list[str]:
    items = summary["negative_results"]
    out = ["## Negative and inconclusive results (never filtered)", ""]
    if not items:
        return out + ["None found in this data."]
    return out + [f"- `{i['group']}` / `{i['arm']}` / {i['metric']} = {i['value']}: {i['detail']}" for i in items]


def _banners(summary: dict[str, Any]) -> list[str]:
    notes = []
    if not summary["variance_ok"]:
        notes.append(
            f"**Not release evidence:** fewer than {summary['min_rounds']} rounds. "
            "Single-round ratios are noise-dominated and must not be cited."
        )
    if not summary["complete"]:
        notes.append("**Incomplete:** a trace was aborted or has fewer completed runs than planned.")
    if not summary["release_grade_harness"]:
        notes.append("**Harness provenance is dirty** (uncommitted changes when the run started); not release-grade.")
    return notes


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [f"# Eval report: {summary['id']}", ""]
    lines += _banners(summary) + [""]
    lines += ["## Provenance", ""] + _provenance(summary) + [""]
    lines += [
        f"Non-inferiority margin (success rate): {summary['non_inferiority_margin']}. "
        f"Intervals are 95% bootstrap over paired tasks and rounds (seed {summary['bootstrap_seed']}).",
        "",
    ]
    for section in (_outcome(summary), _paired(summary), _discoverability(summary), _negative(summary)):
        lines += section + [""]
    return "\n".join(lines).rstrip() + "\n"
