#!/usr/bin/env python3
"""Render CHANGELOG.md into docs/releases.html.

CHANGELOG.md stays the single source of truth for release notes; this script
runs at Pages build time so the published page can never drift from it. The
changelog uses a small, known Markdown subset, so it is rendered directly rather
than pulling in a Markdown dependency — the Pages build stays hermetic.
"""

from __future__ import annotations

import argparse
import html
import re
from dataclasses import dataclass
from pathlib import Path

REPO = "https://github.com/bharath03-a/GitCortex"

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

    # Code spans are taken first so `**text**` inside code stays literal.
    text = CODE_SPAN.sub(
        lambda m: stash(f"<code>{html.escape(m.group(1))}</code>"), text
    )
    # Bold becomes sentinels rather than finished HTML: its inner text still
    # needs escaping below, and it may already contain code placeholders.
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
            # An indented continuation line of the preceding bullet.
            items[-1] = f"{items[-1]} {stripped}"
        else:
            paragraph.append(stripped)

    flush_list()
    flush_paragraph()
    return "".join(parts)


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

PAGE = """<!DOCTYPE html>
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
:root {{
  --paper:#FCFCFA; --surface:#FFFFFF; --surface-2:#F5F5F1;
  --rule:#E5E4DE; --rule-soft:#EFEEE9;
  --ink:#14141A; --ink-2:#55555F; --ink-3:#8A8A94;
  --brand:#C4633F; --brand-soft:#FBEFE9;
  --k-type:#2F6F5E; --k-fn:#2B5CA8; --k-trait:#6B4CA8; --k-doc:#A03D6B;
  --mono: ui-monospace, "SF Mono", "JetBrains Mono", "Cascadia Code", Menlo, Consolas, monospace;
  --sans: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, Helvetica, Arial, sans-serif;
  --gut:28px; --max:1120px;
  color-scheme: light;
}}
*,*::before,*::after {{ box-sizing:border-box; margin:0; padding:0; }}
html {{ font-size:16px; scroll-behavior:smooth; }}
@media (prefers-reduced-motion: reduce) {{ html {{ scroll-behavior:auto; }} }}
body {{
  background:var(--paper); color:var(--ink); font-family:var(--sans);
  line-height:1.6; -webkit-font-smoothing:antialiased; text-rendering:optimizeLegibility;
}}
a {{ color:inherit; text-decoration:none; }}
:focus-visible {{ outline:2px solid var(--brand); outline-offset:3px; border-radius:2px; }}
code {{
  font-family:var(--mono); font-size:.86em; color:var(--ink);
  background:var(--surface-2); border:1px solid var(--rule-soft);
  border-radius:4px; padding:.08em .36em;
}}
.wrap {{ max-width:var(--max); margin:0 auto; padding:0 var(--gut); }}
.narrow {{ max-width:760px; }}

.nav {{
  position:sticky; top:0; z-index:50;
  background:color-mix(in srgb, var(--paper) 88%, transparent);
  backdrop-filter:blur(10px); border-bottom:1px solid var(--rule);
}}
.nav-in {{
  max-width:var(--max); margin:0 auto; padding:0 var(--gut); height:60px;
  display:flex; align-items:center; justify-content:space-between; gap:20px;
}}
.brandmark {{ display:flex; align-items:center; gap:9px; font-weight:700; letter-spacing:-.02em; }}
.brandmark svg {{ display:block; color:var(--brand); }}
.nav-links {{ display:flex; align-items:center; gap:26px; list-style:none; flex-wrap:wrap; }}
.nav-links a {{ font-size:.875rem; color:var(--ink-2); }}
.nav-links a:hover {{ color:var(--ink); }}
.nav-links a[aria-current="page"] {{ color:var(--ink); font-weight:600; }}
.nav-cta {{
  font-family:var(--mono); font-size:.76rem; font-weight:600;
  border:1px solid var(--rule); border-radius:6px; padding:6px 12px; color:var(--ink);
}}
.nav-cta:hover {{ border-color:var(--brand); color:var(--brand); }}
@media (max-width:760px) {{ .nav-hide {{ display:none; }} }}

.masthead {{ padding:76px 0 34px; }}
.eyebrow {{
  font-family:var(--mono); font-size:.69rem; text-transform:uppercase;
  letter-spacing:.145em; font-weight:600; color:var(--ink-3);
  display:block; margin-bottom:14px;
}}
.masthead h1 {{
  font-size:clamp(2.1rem,4.6vw,3rem); line-height:1.05;
  letter-spacing:-.033em; font-weight:800;
}}
.masthead p {{ color:var(--ink-2); margin-top:16px; max-width:62ch; }}
.masthead a {{ color:var(--brand); text-decoration:underline; text-underline-offset:2px; }}

.index {{
  display:flex; flex-wrap:wrap; gap:7px;
  padding:26px 0 30px; border-bottom:1px solid var(--rule);
}}
.index a {{
  font-family:var(--mono); font-size:.75rem; font-weight:600; color:var(--ink-2);
  border:1px solid var(--rule); border-radius:999px; padding:4px 12px; background:var(--surface);
}}
.index a:hover {{ color:var(--brand); border-color:var(--brand); }}

.release {{ padding:42px 0; border-bottom:1px solid var(--rule); }}
.release:last-of-type {{ border-bottom:0; }}
.release-head {{ display:flex; align-items:baseline; gap:14px; flex-wrap:wrap; margin-bottom:4px; }}
.release-head h2 {{
  font-size:1.55rem; font-weight:780; letter-spacing:-.022em; scroll-margin-top:80px;
}}
.release-date {{ font-family:var(--mono); font-size:.78rem; color:var(--ink-3); }}
.badge {{
  font-family:var(--mono); font-size:.65rem; font-weight:700; text-transform:uppercase;
  letter-spacing:.1em; color:var(--brand); background:var(--brand-soft);
  border:1px solid color-mix(in srgb, var(--brand) 28%, transparent);
  border-radius:999px; padding:2px 9px; line-height:1.5;
}}
.release h3 {{
  font-family:var(--mono); font-size:.71rem; text-transform:uppercase;
  letter-spacing:.12em; font-weight:700; color:var(--ink-3); margin:24px 0 10px;
}}
.release h3.tone-green {{ color:var(--k-type); }}
.release h3.tone-blue {{ color:var(--k-fn); }}
.release h3.tone-accent {{ color:var(--brand); }}
.release h3.tone-purple {{ color:var(--k-trait); }}
.release p {{ color:var(--ink-2); margin:12px 0; max-width:74ch; }}
.release ul {{ list-style:none; }}
.release li {{
  position:relative; padding-left:22px; margin:9px 0;
  color:var(--ink-2); max-width:74ch;
}}
.release li::before {{
  content:""; position:absolute; left:2px; top:.72em;
  width:9px; height:1px; background:var(--ink-3);
}}
.release strong {{ color:var(--ink); font-weight:650; }}
.tag-link {{ margin-top:22px; }}
.tag-link a {{
  font-family:var(--mono); font-size:.76rem; color:var(--ink-3);
  border-bottom:1px solid var(--rule); padding-bottom:2px;
}}
.tag-link a:hover {{ color:var(--brand); border-color:var(--brand); }}

.foot {{ border-top:1px solid var(--rule); padding:34px 0 44px; margin-top:30px; }}
.foot-in {{ display:flex; justify-content:space-between; gap:20px; flex-wrap:wrap; align-items:center; }}
.foot-id {{ display:flex; align-items:center; gap:9px; font-family:var(--mono); font-size:.78rem; color:var(--ink-3); }}
.foot-id svg {{ width:18px; height:18px; color:var(--brand); }}
.foot-links {{ display:flex; gap:20px; flex-wrap:wrap; }}
.foot-links a {{ font-size:.85rem; color:var(--ink-2); }}
.foot-links a:hover {{ color:var(--ink); }}
</style>
</head>
<body>

<nav class="nav">
  <div class="nav-in">
    <a class="brandmark" href="./" aria-label="GitCortex home">{logo} GitCortex</a>
    <ul class="nav-links">
      <li class="nav-hide"><a href="./#how">How it works</a></li>
      <li class="nav-hide"><a href="./#features">Features</a></li>
      <li class="nav-hide"><a href="./#install">Install</a></li>
      <li><a href="releases.html" aria-current="page">Releases</a></li>
      <li><a class="nav-cta" href="{repo}">GitHub</a></li>
    </ul>
  </div>
</nav>

<header class="masthead">
  <div class="wrap">
    <span class="eyebrow">Release log</span>
    <h1>Every version, and what changed in it.</h1>
    <p>
      Newest first. Generated from
      <a href="{repo}/blob/main/CHANGELOG.md">CHANGELOG.md</a> at build time, so this page
      and the changelog cannot drift apart. Binaries and checksums for each version are on
      the <a href="{repo}/releases">releases page</a>.
    </p>
    <nav class="index" aria-label="Jump to version">{jump}</nav>
  </div>
</header>

<main class="wrap">
{releases}
</main>

<footer class="foot">
  <div class="wrap foot-in">
    <div class="foot-id">{logo} GitCortex · MIT</div>
    <div class="foot-links">
      <a href="./">Home</a>
      <a href="{repo}">GitHub</a>
      <a href="{repo}/releases">Downloads</a>
      <a href="{repo}/blob/main/docs/REFERENCE.md">Reference</a>
      <a href="https://crates.io/crates/gitcortex">crates.io</a>
      <a href="https://pypi.org/project/gitcortex/">PyPI</a>
    </div>
  </div>
</footer>
</body>
</html>
"""


def render_page(releases: list[Release]) -> str:
    """Render the full releases page from parsed releases."""
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
        blocks.append(
            f'<article class="release">'
            f'<div class="release-head">'
            f'<h2 id="v{version}">{version}</h2>{date}{latest}'
            f"</div>"
            f"{render_body(release.body)}"
            f'<p class="tag-link"><a href="{REPO}/releases/tag/v{version}">'
            f"Downloads and checksums for v{version} &rarr;</a></p>"
            f"</article>"
        )
    return PAGE.format(logo=LOGO, repo=REPO, jump=jump, releases="\n".join(blocks))




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
