#!/usr/bin/env python3
"""Render CHANGELOG.md into docs/releases.html.

CHANGELOG.md stays the single source of truth for release notes; this script
runs at Pages build time so the published page can never drift from it. Every
release entry renders as a collapsible <details> block with ONLY the latest
release expanded by default.
"""

from __future__ import annotations

import argparse
import html
import re
from dataclasses import dataclass
from pathlib import Path
from string import Template

REPO = "https://github.com/bharath03-a/GitCortex"
REPO_API = "https://api.github.com/repos/bharath03-a/GitCortex"

RELEASE_HEADING = re.compile(r"^## \[(?P<version>[^\]]+)\](?:\s*-\s*(?P<date>\S+))?\s*$")
BULLET = re.compile(r"^-\s+(?P<text>.*)$")
SUBHEADING = re.compile(r"^###\s+(?P<text>.*)$")
CODE_SPAN = re.compile(r"`([^`]+)`")
BOLD = re.compile(r"\*\*([^*]+)\*\*")

#: Sections that carry a colour accent on the page.
SECTION_TONE = {
    "added": "green",
    "changed": "blue",
    "fixed": "accent",
    "removed": "purple",
}


@dataclass(frozen=True)
class Release:
    version: str
    date: str
    body: str


def parse_changelog(text: str) -> list[Release]:
    """Split a changelog into releases, preserving file order (newest first)."""
    releases: list[Release] = []
    version: str | None = None
    date = ""
    body: list[str] = []

    def flush() -> None:
        if version is not None:
            releases.append(
                Release(version=version, date=date, body="\n".join(body).strip())
            )

    for line in text.splitlines():
        heading = RELEASE_HEADING.match(line)
        if heading:
            flush()
            version = heading.group("version")
            date = heading.group("date") or ""
            body = []
        elif version is not None:
            body.append(line)

    flush()
    return releases


#: Sentinels for bold, kept as single characters so escaping leaves them intact
#: and bold can wrap already-stashed code spans without a recursive render.
BOLD_OPEN = "\x01"
BOLD_CLOSE = "\x02"


def render_inline(text: str) -> str:
    """Render bold and code spans, escaping everything else."""
    placeholders: list[str] = []

    def stash(rendered: str) -> str:
        placeholders.append(rendered)
        return f"\x00{len(placeholders) - 1}\x00"

    text = CODE_SPAN.sub(
        lambda m: stash(f"<code>{html.escape(m.group(1))}</code>"), text
    )
    text = BOLD.sub(lambda m: f"{BOLD_OPEN}{m.group(1)}{BOLD_CLOSE}", text)
    escaped = html.escape(text)
    escaped = escaped.replace(BOLD_OPEN, "<strong>").replace(BOLD_CLOSE, "</strong>")
    return re.sub(r"\x00(\d+)\x00", lambda m: placeholders[int(m.group(1))], escaped)


def render_body(markdown: str) -> str:
    """Render the subset of Markdown the changelog actually uses."""
    parts: list[str] = []
    items: list[str] = []
    paragraph: list[str] = []

    def flush_list() -> None:
        if items:
            rendered = "".join(f"<li>{render_inline(item)}</li>" for item in items)
            parts.append(f"<ul>{rendered}</ul>")
            items.clear()

    def flush_paragraph() -> None:
        if paragraph:
            parts.append(f"<p>{render_inline(' '.join(paragraph))}</p>")
            paragraph.clear()

    for line in markdown.splitlines():
        stripped = line.strip()
        heading = SUBHEADING.match(stripped)
        bullet = BULLET.match(stripped)
        if heading:
            flush_list()
            flush_paragraph()
            name = heading.group("text")
            tone = SECTION_TONE.get(name.lower(), "")
            css = f' class="tone-{tone}"' if tone else ""
            parts.append(f"<h3{css}>{render_inline(name)}</h3>")
        elif bullet:
            flush_paragraph()
            items.append(bullet.group("text"))
        elif not stripped:
            flush_paragraph()
        elif items:
            items[-1] = f"{items[-1]} {stripped}"
        else:
            paragraph.append(stripped)

    flush_list()
    flush_paragraph()
    return "".join(parts)


def summarize_sections(markdown: str) -> str:
    """Return a concise section summary like '3 fixed · 2 added' for collapsed headers."""
    counts: list[tuple[str, int]] = []
    current: str | None = None
    n = 0
    for line in markdown.splitlines():
        stripped = line.strip()
        heading = SUBHEADING.match(stripped)
        bullet = BULLET.match(stripped)
        if heading:
            if current and n:
                counts.append((current, n))
            current = heading.group("text").lower()
            n = 0
        elif bullet and current:
            n += 1
    if current and n:
        counts.append((current, n))
    if not counts:
        return ""
    return " · ".join(f"{cnt} {name}" for name, cnt in counts[:3])


LOGO = (
    '<svg viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg" '
    'width="22" height="22" aria-hidden="true">'
    '<line x1="18" y1="18" x2="46" y2="32" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>'
    '<line x1="18" y1="18" x2="32" y2="50" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>'
    '<line x1="46" y1="32" x2="32" y2="50" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" opacity=".55"/>'
    '<circle cx="18" cy="18" r="7" fill="currentColor"/>'
    '<circle cx="46" cy="32" r="5.5" fill="currentColor" opacity=".72"/>'
    '<circle cx="32" cy="50" r="5.5" fill="currentColor" opacity=".72"/>'
    "</svg>"
)

GH_ICON = (
    '<svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor" aria-hidden="true">'
    '<path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 '
    '0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 '
    '1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 '
    '0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 '
    '2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 '
    '3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>'
    "</svg>"
)

CHEVRON = (
    '<svg class="chev" viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">'
    '<path d="M6 4l4 4-4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>'
    "</svg>"
)

PAGE = Template("""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="GitCortex release log — every version, what changed, and when it shipped.">
<title>GitCortex — Release log</title>
<meta property="og:type" content="website">
<meta property="og:url" content="https://bharath03-a.github.io/GitCortex/releases.html">
<meta property="og:title" content="GitCortex — Release log">
<meta property="og:description" content="Every GitCortex version, what changed, and when it shipped.">
<link rel="icon" type="image/svg+xml" href="favicon.svg">
<link rel="apple-touch-icon" href="favicon.svg">
<link rel="canonical" href="https://bharath03-a.github.io/GitCortex/releases.html">
<style>
@font-face {
  font-family: "Space Grotesk"; font-style: normal; font-weight: 400; font-display: swap;
  src: url("assets/fonts/space-grotesk-latin-400-normal.woff2") format("woff2");
}
@font-face {
  font-family: "Space Grotesk"; font-style: normal; font-weight: 500; font-display: swap;
  src: url("assets/fonts/space-grotesk-latin-500-normal.woff2") format("woff2");
}
@font-face {
  font-family: "Space Grotesk"; font-style: normal; font-weight: 600 800; font-display: swap;
  src: url("assets/fonts/space-grotesk-latin-600-normal.woff2") format("woff2");
}
@font-face {
  font-family: "IBM Plex Mono"; font-style: normal; font-weight: 400; font-display: swap;
  src: url("assets/fonts/ibm-plex-mono-latin-400-normal.woff2") format("woff2");
}
@font-face {
  font-family: "IBM Plex Mono"; font-style: normal; font-weight: 500 600; font-display: swap;
  src: url("assets/fonts/ibm-plex-mono-latin-600-normal.woff2") format("woff2");
}
:root {
  --paper:#FCFCFA; --surface:#FFFFFF; --surface-2:#F5F5F1;
  --rule:#E5E4DE; --rule-soft:#EFEEE9;
  --ink:#14141A; --ink-2:#55555F; --ink-3:#8A8A94;
  --brand:#C4633F; --brand-soft:#FBEFE9;
  --k-type:#2F6F5E; --k-fn:#2B5CA8; --k-trait:#6B4CA8; --k-doc:#A03D6B;
  --mono: "IBM Plex Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  --sans: "Space Grotesk", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
  --gut:28px; --max:1120px;
  color-scheme: light;
}
*,*::before,*::after { box-sizing:border-box; margin:0; padding:0; }
html { font-size:16px; scroll-behavior:smooth; }
@media (prefers-reduced-motion: reduce) { html { scroll-behavior:auto; } }
body {
  background:var(--paper); color:var(--ink); font-family:var(--sans);
  line-height:1.6; -webkit-font-smoothing:antialiased; text-rendering:optimizeLegibility;
}
a { color:inherit; text-decoration:none; }
:focus-visible { outline:2px solid var(--brand); outline-offset:3px; border-radius:2px; }
code {
  font-family:var(--mono); font-size:.86em; color:var(--ink);
  background:var(--surface-2); border:1px solid var(--rule-soft);
  border-radius:4px; padding:.08em .36em;
}
.wrap { max-width:var(--max); margin:0 auto; padding:0 var(--gut); }

.nav {
  position:sticky; top:0; z-index:50;
  background:color-mix(in srgb, var(--paper) 90%, transparent);
  backdrop-filter:blur(10px); border-bottom:1px solid var(--rule);
}
.nav-in {
  max-width:var(--max); margin:0 auto; padding:0 var(--gut); height:60px;
  display:flex; align-items:center; justify-content:space-between; gap:20px;
}
.brandmark { display:flex; align-items:center; gap:9px; font-weight:700; letter-spacing:-.02em; }
.brandmark svg { display:block; color:var(--brand); width:22px; height:22px; }
.nav-links { display:flex; align-items:center; gap:22px; list-style:none; }
.nav-links a { font-size:.875rem; color:var(--ink-2); transition:color .14s ease; }
.nav-links a:hover { color:var(--ink); }
.nav-links a[aria-current="page"] { color:var(--ink); font-weight:600; }
.gh-star {
  display:inline-flex; align-items:center; gap:7px;
  font-family:var(--mono); font-size:.76rem; font-weight:600;
  color:var(--ink) !important; background:var(--surface);
  border:1px solid var(--rule); border-radius:999px;
  padding:5px 11px; line-height:1;
  transition:border-color .14s ease, color .14s ease;
}
.gh-star:hover { border-color:var(--brand); color:var(--brand) !important; }
.gh-star-count {
  display:inline-flex; align-items:center; gap:3px;
  padding-left:7px; border-left:1px solid var(--rule);
  color:var(--ink-2); font-variant-numeric:tabular-nums;
}
.gh-star-count:empty { display:none; }
@media (max-width:760px) { .nav-hide { display:none; } .nav-links { gap:14px; } }

.masthead { padding:68px 0 28px; }
.eyebrow {
  font-family:var(--mono); font-size:.69rem; text-transform:uppercase;
  letter-spacing:.145em; font-weight:600; color:var(--ink-3);
  display:block; margin-bottom:14px;
}
.masthead h1 {
  font-size:clamp(2.1rem,4.6vw,3rem); line-height:1.05;
  letter-spacing:-.033em; font-weight:800;
}
.masthead p { color:var(--ink-2); margin-top:14px; max-width:64ch; }
.masthead a { color:var(--brand); text-decoration:underline; text-underline-offset:2px; }

.controls-bar {
  display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between;
  gap:12px; padding:22px 0 24px; border-bottom:1px solid var(--rule);
}
.index { display:flex; flex-wrap:wrap; gap:7px; }
.index a {
  font-family:var(--mono); font-size:.74rem; font-weight:600; color:var(--ink-2);
  border:1px solid var(--rule); border-radius:999px; padding:4px 11px; background:var(--surface);
  transition:border-color .14s ease, color .14s ease;
}
.index a:hover { color:var(--brand); border-color:var(--brand); }
.toggle-all {
  font-family:var(--mono); font-size:.74rem; font-weight:600; color:var(--ink-2);
  background:var(--surface); border:1px solid var(--rule); border-radius:8px;
  padding:6px 12px; cursor:pointer; white-space:nowrap;
}
.toggle-all:hover { color:var(--ink); border-color:var(--brand); }

.release-list { display:flex; flex-direction:column; gap:14px; padding:26px 0 42px; }
details.release {
  background:var(--surface); border:1px solid var(--rule); border-radius:12px;
  scroll-margin-top:80px; overflow:hidden;
  transition:border-color .14s ease;
}
details.release[open] { border-color:color-mix(in srgb, var(--brand) 38%, var(--rule)); }
details.release summary.release-head {
  list-style:none; cursor:pointer; padding:18px 22px;
  display:flex; align-items:center; justify-content:space-between; gap:16px;
  user-select:none; background:var(--surface);
}
details.release summary.release-head::-webkit-details-marker { display:none; }
details.release[open] summary.release-head {
  background:var(--paper); border-bottom:1px solid var(--rule-soft);
}
.release-meta { display:flex; align-items:center; gap:12px; flex-wrap:wrap; }
.release-meta h2 {
  font-size:1.3rem; font-weight:780; letter-spacing:-.022em; line-height:1.1;
}
.release-date { font-family:var(--mono); font-size:.78rem; color:var(--ink-3); }
.release-summary { font-family:var(--mono); font-size:.72rem; color:var(--ink-3); }
.badge {
  font-family:var(--mono); font-size:.64rem; font-weight:700; text-transform:uppercase;
  letter-spacing:.1em; color:var(--brand); background:var(--brand-soft);
  border:1px solid color-mix(in srgb, var(--brand) 28%, transparent);
  border-radius:999px; padding:2px 9px; line-height:1.5;
}
.release-right { display:flex; align-items:center; gap:12px; color:var(--ink-3); }
.chev { transition:transform .16s ease; flex-shrink:0; }
details.release[open] .chev { transform:rotate(90deg); color:var(--brand); }

.release-body { padding:20px 24px 24px; }
.release-body h3 {
  font-family:var(--mono); font-size:.71rem; text-transform:uppercase;
  letter-spacing:.12em; font-weight:700; color:var(--ink-3); margin:20px 0 9px;
}
.release-body h3:first-child { margin-top:0; }
.release-body h3.tone-green { color:var(--k-type); }
.release-body h3.tone-blue { color:var(--k-fn); }
.release-body h3.tone-accent { color:var(--brand); }
.release-body h3.tone-purple { color:var(--k-trait); }
.release-body p { color:var(--ink-2); margin:10px 0; max-width:74ch; }
.release-body ul { list-style:none; }
.release-body li {
  position:relative; padding-left:22px; margin:8px 0;
  color:var(--ink-2); max-width:74ch;
}
.release-body li::before {
  content:""; position:absolute; left:2px; top:.72em;
  width:9px; height:1px; background:var(--ink-3);
}
.release-body strong { color:var(--ink); font-weight:650; }
.tag-link { margin-top:20px; padding-top:14px; border-top:1px solid var(--rule-soft); }
.tag-link a {
  font-family:var(--mono); font-size:.76rem; color:var(--ink-3);
  border-bottom:1px solid var(--rule); padding-bottom:2px;
}
.tag-link a:hover { color:var(--brand); border-color:var(--brand); }

.foot { border-top:1px solid var(--rule); padding:34px 0 44px; margin-top:20px; }
.foot-in { display:flex; justify-content:space-between; gap:20px; flex-wrap:wrap; align-items:center; }
.foot-id { display:flex; align-items:center; gap:9px; font-family:var(--mono); font-size:.78rem; color:var(--ink-3); }
.foot-id svg { width:18px; height:18px; color:var(--brand); }
.foot-links { display:flex; gap:20px; flex-wrap:wrap; }
.foot-links a { font-size:.85rem; color:var(--ink-2); }
.foot-links a:hover { color:var(--ink); }
</style>
</head>
<body>

<nav class="nav" aria-label="Primary">
  <div class="nav-in">
    <a class="brandmark" href="./" aria-label="GitCortex home">${logo} GitCortex</a>
    <ul class="nav-links">
      <li class="nav-hide"><a href="./#walkthrough">Workflow</a></li>
      <li class="nav-hide"><a href="./#install">Install</a></li>
      <li><a href="benchmarks.html">Benchmarks</a></li>
      <li><a href="compare.html">Compare</a></li>
      <li><a href="releases.html" aria-current="page">Releases</a></li>
      <li><a class="gh-star" href="${repo}" aria-label="GitCortex on GitHub" target="_blank" rel="noopener">${gh_icon}<span class="gh-star-count" data-gh-stars></span></a></li>
    </ul>
  </div>
</nav>

<header class="masthead">
  <div class="wrap">
    <span class="eyebrow">Release log</span>
    <h1>Every version, and what changed in it.</h1>
    <p>
      Latest release expanded by default; older versions collapse to keep scanning fast. Generated from
      <a href="${repo}/blob/main/CHANGELOG.md">CHANGELOG.md</a> at build time so the site and changelog never drift.
      Binaries and checksums are on <a href="${repo}/releases">GitHub Releases</a>.
    </p>
    <div class="controls-bar">
      <nav class="index" aria-label="Jump to version">${jump}</nav>
      <button type="button" class="toggle-all" id="toggleAllBtn">Expand all</button>
    </div>
  </div>
</header>

<main class="wrap">
  <div class="release-list">
${releases}
  </div>
</main>

<footer class="foot">
  <div class="wrap foot-in">
    <div class="foot-id">${logo} GitCortex · MIT</div>
    <div class="foot-links">
      <a href="./">Home</a>
      <a href="benchmarks.html">Benchmarks</a>
      <a href="compare.html">Compare</a>
      <a href="${repo}">GitHub</a>
      <a href="${repo}/releases">Downloads</a>
      <a href="${repo}/blob/main/docs/REFERENCE.md">Reference</a>
      <a href="https://crates.io/crates/gitcortex">crates.io</a>
      <a href="https://pypi.org/project/gitcortex/">PyPI</a>
    </div>
  </div>
</footer>

<script>
(function() {
  // Live GitHub star count (cached in sessionStorage for 15 minutes)
  var el = document.querySelector("[data-gh-stars]");
  if (el) {
    var key = "gcx_gh_stars_v1";
    var fmt = function(n) { return n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n); };
    var render = function(n) { el.textContent = "\u2605 " + fmt(n); };
    try {
      var cached = JSON.parse(sessionStorage.getItem(key) || "null");
      if (cached && Date.now() - cached.ts < 900000 && typeof cached.n === "number") {
        render(cached.n);
      } else {
        fetch("${repo_api}", { headers: { Accept: "application/vnd.github+json" } })
          .then(function(r) { return r.ok ? r.json() : null; })
          .then(function(d) {
            if (d && typeof d.stargazers_count === "number") {
              render(d.stargazers_count);
              try { sessionStorage.setItem(key, JSON.stringify({ n: d.stargazers_count, ts: Date.now() })); } catch (_) {}
            }
          }).catch(function() {});
      }
    } catch (_) {}
  }

  // Open target release when navigating via #vX.Y.Z hash or jump pill
  function openHashTarget() {
    var id = (location.hash || "").replace(/^#/, "");
    if (!id) return;
    var target = document.getElementById(id);
    if (target && target.tagName === "DETAILS") {
      target.open = true;
    }
  }
  window.addEventListener("hashchange", openHashTarget);
  openHashTarget();

  // Expand all / Collapse older toggle
  var btn = document.getElementById("toggleAllBtn");
  var items = Array.prototype.slice.call(document.querySelectorAll("details.release"));
  if (btn && items.length > 1) {
    var syncLabel = function() {
      var allOpen = items.every(function(d) { return d.open; });
      btn.textContent = allOpen ? "Collapse older" : "Expand all";
    };
    btn.addEventListener("click", function() {
      var allOpen = items.every(function(d) { return d.open; });
      items.forEach(function(d, idx) { d.open = allOpen ? (idx === 0) : true; });
      syncLabel();
    });
    items.forEach(function(d) { d.addEventListener("toggle", syncLabel); });
    syncLabel();
  }
})();
</script>
</body>
</html>
""")


def render_page(releases: list[Release]) -> str:
    """Render the full releases page with collapsible entries (latest open by default)."""
    jump = "".join(
        f'<a href="#v{html.escape(r.version)}">{html.escape(r.version)}</a>'
        for r in releases
    )
    blocks = []
    for index, release in enumerate(releases):
        version = html.escape(release.version)
        date = (
            f'<span class="release-date">{html.escape(release.date)}</span>'
            if release.date
            else ""
        )
        latest = '<span class="badge">Latest</span>' if index == 0 else ""
        summary = summarize_sections(release.body)
        summary_html = (
            f'<span class="release-summary">{html.escape(summary)}</span>'
            if summary
            else ""
        )
        open_attr = " open" if index == 0 else ""
        blocks.append(
            f'<details class="release" id="v{version}"{open_attr}>'
            f'<summary class="release-head">'
            f'<div class="release-meta"><h2>v{version}</h2>{date}{latest}</div>'
            f'<div class="release-right">{summary_html}{CHEVRON}</div>'
            f"</summary>"
            f'<div class="release-body">'
            f"{render_body(release.body)}"
            f'<p class="tag-link"><a href="{REPO}/releases/tag/v{version}">'
            f"Downloads and checksums for v{version} &rarr;</a></p>"
            f"</div>"
            f"</details>"
        )
    return PAGE.substitute(
        logo=LOGO,
        gh_icon=GH_ICON,
        repo=REPO,
        repo_api=REPO_API,
        jump=jump,
        releases="\n".join(blocks),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    root = Path(__file__).resolve().parent.parent
    parser.add_argument("--changelog", type=Path, default=root / "CHANGELOG.md")
    parser.add_argument("--output", type=Path, default=root / "docs" / "releases.html")
    args = parser.parse_args()

    releases = parse_changelog(args.changelog.read_text(encoding="utf-8"))
    if not releases:
        raise SystemExit(f"no releases parsed from {args.changelog}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(render_page(releases), encoding="utf-8")
    print(f"wrote {args.output} ({len(releases)} releases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
