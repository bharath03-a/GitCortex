//! Cypher string escaping. Kept separate to make it easy to audit the one
//! place that constructs inline string literals in queries.

/// Escape a string for inline use in a Cypher query.
/// Replaces `\` → `\\` and `'` → `\'`.
pub(super) fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Escape a multi-line string (e.g. a function body or docstring) for inline
/// Cypher use.
///
/// Kuzu's single-quoted string literals accept literal newlines, tabs, and
/// carriage returns verbatim — and it does NOT honour C-style `\n`/`\t`
/// escapes (an unknown escape silently drops the backslash, turning `\n`
/// into a bare `n`). So the only characters we must escape are the ones that
/// would terminate or corrupt the literal: backslash and single-quote.
/// Identical to [`esc`]; kept as a named alias to document intent at the
/// multi-line call sites.
pub(super) fn esc_multiline(s: &str) -> String {
    esc(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_escapes_backslash_and_quote() {
        assert_eq!(esc(r"a\b'c"), r"a\\b\'c");
    }

    #[test]
    fn esc_multiline_preserves_newlines_tabs() {
        // Regression: a prior version encoded `\n` as the two chars `\` + `n`,
        // which Kuzu then stored as a bare `n` (it drops unknown escapes),
        // collapsing multi-line docstrings. Literal newlines must survive.
        let input = "line1\nline2\twith tab\r\nend";
        let out = esc_multiline(input);
        assert!(out.contains('\n'), "newline must remain literal");
        assert!(out.contains('\t'), "tab must remain literal");
        assert!(
            !out.contains("\\n"),
            "must not introduce a backslash-n escape"
        );
    }

    #[test]
    fn esc_multiline_still_escapes_quotes() {
        assert_eq!(esc_multiline("it's\n\"ok\""), "it\\'s\n\"ok\"");
    }
}
