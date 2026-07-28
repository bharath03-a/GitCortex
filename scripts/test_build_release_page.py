"""Tests for the CHANGELOG -> releases page renderer."""

from __future__ import annotations

import unittest

from build_release_page import parse_changelog, render_body, render_inline

CHANGELOG = """# Changelog

All notable changes to GitCortex are documented here.

## [0.6.3] - 2026-07-12

Pure refactor release.

### Changed
- **`tools.rs` split** into modules.
- Versions bumped across `Cargo.toml`.

## [0.1.0] - 2026-04-30

### Added
- First release.
"""


class ParseChangelogTest(unittest.TestCase):
    def test_extracts_releases_in_file_order(self) -> None:
        releases = parse_changelog(CHANGELOG)
        self.assertEqual([r.version for r in releases], ["0.6.3", "0.1.0"])
        self.assertEqual(releases[0].date, "2026-07-12")

    def test_ignores_preamble(self) -> None:
        releases = parse_changelog(CHANGELOG)
        self.assertNotIn("All notable changes", releases[0].body)

    def test_body_stops_at_next_release(self) -> None:
        releases = parse_changelog(CHANGELOG)
        self.assertIn("Versions bumped", releases[0].body)
        self.assertNotIn("First release", releases[0].body)


class RenderInlineTest(unittest.TestCase):
    def test_bold_and_code(self) -> None:
        self.assertEqual(
            render_inline("**bold** and `code`"),
            "<strong>bold</strong> and <code>code</code>",
        )

    def test_escapes_html(self) -> None:
        self.assertEqual(render_inline("a < b & c"), "a &lt; b &amp; c")

    def test_code_span_nested_in_bold(self) -> None:
        """The real changelog writes **`tools.rs` split** — bold wrapping code."""
        self.assertEqual(
            render_inline("**`tools.rs` split** into modules."),
            "<strong><code>tools.rs</code> split</strong> into modules.",
        )

    def test_escapes_inside_code(self) -> None:
        """Angle brackets are common in signatures and must not become tags."""
        self.assertEqual(
            render_inline("`Vec<String>`"), "<code>Vec&lt;String&gt;</code>"
        )


class RenderBodyTest(unittest.TestCase):
    def test_heading_and_list(self) -> None:
        html = render_body("### Changed\n- one\n- two\n")
        self.assertIn(">Changed</h3>", html)
        self.assertIn("<li>one</li>", html)
        self.assertIn("<li>two</li>", html)
        self.assertEqual(html.count("<ul>"), 1)

    def test_known_sections_carry_a_tone_class(self) -> None:
        self.assertIn('<h3 class="tone-green">Added</h3>', render_body("### Added\n"))
        self.assertIn("<h3>Notes</h3>", render_body("### Notes\n"))

    def test_wrapped_bullet_continuation_joins(self) -> None:
        html = render_body("- a bullet\n  continued here\n")
        self.assertIn("<li>a bullet continued here</li>", html)

    def test_paragraph_outside_list(self) -> None:
        html = render_body("Pure refactor release.\n")
        self.assertIn("<p>Pure refactor release.</p>", html)

    def test_list_closes_before_heading(self) -> None:
        html = render_body("- one\n\n### Fixed\n- two\n")
        self.assertLess(html.index("</ul>"), html.index(">Fixed</h3>"))


if __name__ == "__main__":
    unittest.main()
