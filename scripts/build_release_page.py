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
    'width="26" height="26" aria-hidden="true">'
    '<line x1="18" y1="18" x2="46" y2="32" stroke="#cc785c" stroke-width="3" stroke-linecap="round"/>'
    '<line x1="18" y1="18" x2="32" y2="50" stroke="#cc785c" stroke-width="3" stroke-linecap="round"/>'
    '<line x1="46" y1="32" x2="32" y2="50" stroke="#cc785c" stroke-width="2.5" stroke-linecap="round" opacity="0.7"/>'
    '<circle cx="18" cy="18" r="7" fill="#cc785c"/>'
    '<circle cx="46" cy="32" r="5.5" fill="#e0a07f"/>'
    '<circle cx="32" cy="50" r="5.5" fill="#e0a07f"/>'
    "</svg>"
)

PAGE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="GitCortex release log — every version, what changed, and when it shipped.">
<title>GitCortex — Release Log</title>
<meta property="og:type" content="website">
<meta property="og:url" content="https://bharath03-a.github.io/GitCortex/releases.html">
<meta property="og:title" content="GitCortex — Release Log">
<meta property="og:description" content="Every GitCortex version, what changed, and when it shipped.">
<link rel="icon" type="image/svg+xml" href="favicon.svg">
<link rel="apple-touch-icon" href="favicon.svg">
<link rel="canonical" href="https://bharath03-a.github.io/GitCortex/releases.html">
<style>
:root {{
  --bg:#06060a; --surface:#0d0d14; --surface-2:#13131e; --border:#1e1e2e;
  --accent:#cc785c; --accent-2:#e0a07f; --blue:#7c9ef5; --green:#5bbd8f;
  --purple:#a78bfa; --text:#e2e2f0; --text-dim:#9090aa; --text-muted:#5a5a72;
  --radius:12px; --radius-sm:8px;
}}
*,*::before,*::after {{ box-sizing:border-box; margin:0; padding:0; }}
html {{ scroll-behavior:smooth; font-size:16px; }}
body {{
  background:var(--bg); color:var(--text);
  font-family:system-ui,-apple-system,"Segoe UI",Helvetica,Arial,sans-serif;
  line-height:1.65; -webkit-font-smoothing:antialiased;
}}
a {{ color:var(--accent-2); text-decoration:none; }}
a:hover {{ text-decoration:underline; }}
code {{
  font-family:"JetBrains Mono","Fira Code",ui-monospace,monospace;
  font-size:.875em; background:var(--surface-2); border:1px solid var(--border);
  border-radius:4px; padding:.1em .38em; color:var(--accent-2);
}}
.container {{ max-width:860px; margin:0 auto; padding:0 24px; }}
nav {{
  position:sticky; top:0; z-index:100; background:rgba(6,6,10,.85);
  backdrop-filter:blur(12px); border-bottom:1px solid var(--border);
}}
.nav-inner {{
  max-width:1100px; margin:0 auto; padding:0 24px; height:60px;
  display:flex; align-items:center; justify-content:space-between; gap:16px;
}}
.nav-logo {{ display:flex; align-items:center; gap:10px; font-weight:700; color:var(--text); }}
.nav-logo:hover {{ text-decoration:none; }}
.nav-links {{ display:flex; gap:22px; list-style:none; align-items:center; flex-wrap:wrap; }}
.nav-links a {{ color:var(--text-dim); font-size:.925rem; }}
.nav-links a:hover {{ color:var(--text); text-decoration:none; }}
header.page {{ padding:64px 0 28px; }}
header.page h1 {{ font-size:clamp(2rem,5vw,2.75rem); line-height:1.15; letter-spacing:-.02em; }}
header.page p {{ color:var(--text-dim); margin-top:12px; max-width:60ch; }}
.jump {{
  display:flex; flex-wrap:wrap; gap:8px; margin:28px 0 8px;
  padding-bottom:28px; border-bottom:1px solid var(--border);
}}
.jump a {{
  font-size:.8rem; font-family:ui-monospace,monospace; color:var(--text-dim);
  border:1px solid var(--border); border-radius:999px; padding:4px 11px;
  background:var(--surface);
}}
.jump a:hover {{ color:var(--text); border-color:var(--accent); text-decoration:none; }}
.release {{ padding:40px 0; border-bottom:1px solid var(--border); }}
.release:last-child {{ border-bottom:0; }}
.release-head {{ display:flex; align-items:baseline; gap:14px; flex-wrap:wrap; margin-bottom:6px; }}
.release-head h2 {{ font-size:1.6rem; letter-spacing:-.01em; scroll-margin-top:80px; }}
.release-date {{ color:var(--text-muted); font-size:.85rem; font-family:ui-monospace,monospace; }}
.badge-latest {{
  font-size:.7rem; text-transform:uppercase; letter-spacing:.06em; font-weight:700;
  color:var(--bg); background:var(--accent); border-radius:999px; padding:2px 9px;
}}
.release h3 {{
  font-size:.78rem; text-transform:uppercase; letter-spacing:.08em;
  margin:22px 0 8px; color:var(--text-dim);
}}
.release h3.tone-green {{ color:var(--green); }}
.release h3.tone-blue {{ color:var(--blue); }}
.release h3.tone-accent {{ color:var(--accent-2); }}
.release h3.tone-purple {{ color:var(--purple); }}
.release p {{ color:var(--text-dim); margin:10px 0; }}
.release ul {{ list-style:none; }}
.release li {{ position:relative; padding-left:20px; margin:7px 0; color:var(--text-dim); }}
.release li::before {{ content:"—"; position:absolute; left:0; color:var(--text-muted); }}
.release strong {{ color:var(--text); font-weight:650; }}
.tag-link {{ font-size:.82rem; color:var(--text-muted); }}
footer {{ border-top:1px solid var(--border); padding:36px 0; margin-top:20px; }}
.footer-inner {{
  max-width:1100px; margin:0 auto; padding:0 24px; display:flex;
  justify-content:space-between; gap:18px; flex-wrap:wrap; align-items:center;
}}
.footer-logo {{ display:flex; align-items:center; gap:10px; color:var(--text-dim); font-size:.9rem; }}
.footer-links {{ display:flex; gap:20px; flex-wrap:wrap; }}
.footer-links a {{ color:var(--text-dim); font-size:.9rem; }}
@media (max-width:640px) {{
  .nav-links {{ gap:14px; }}
  .release {{ padding:30px 0; }}
}}
</style>
</head>
<body>
<nav>
  <div class="nav-inner">
    <a class="nav-logo" href="./">{logo} GitCortex</a>
    <ul class="nav-links">
      <li><a href="./#features">Features</a></li>
      <li><a href="./#install">Install</a></li>
      <li><a href="./#mcp">MCP</a></li>
      <li><a href="releases.html">Releases</a></li>
      <li><a href="{repo}">GitHub</a></li>
    </ul>
  </div>
</nav>

<header class="page">
  <div class="container">
    <h1>Release log</h1>
    <p>
      Every released version of GitCortex, newest first. Generated from
      <a href="{repo}/blob/main/CHANGELOG.md">CHANGELOG.md</a> at build time.
    </p>
    <div class="jump">{jump}</div>
  </div>
</header>

<main class="container">
{releases}
</main>

<footer>
  <div class="footer-inner">
    <div class="footer-logo">{logo} GitCortex · MIT License</div>
    <div class="footer-links">
      <a href="./">Home</a>
      <a href="{repo}">GitHub</a>
      <a href="{repo}/releases">Downloads</a>
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
        date = f'<span class="release-date">{html.escape(release.date)}</span>' if release.date else ""
        latest = '<span class="badge-latest">Latest</span>' if index == 0 else ""
        blocks.append(
            f'<article class="release">'
            f'<div class="release-head">'
            f'<h2 id="v{version}">{version}</h2>{date}{latest}'
            f"</div>"
            f"{render_body(release.body)}"
            f'<p class="tag-link"><a href="{REPO}/releases/tag/v{version}">'
            f"Downloads and checksums for v{version} →</a></p>"
            f"</article>"
        )
    return PAGE.format(
        logo=LOGO, repo=REPO, jump=jump, releases="\n".join(blocks)
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
