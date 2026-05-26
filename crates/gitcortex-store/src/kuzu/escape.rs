//! Cypher string escaping. Kept separate to make it easy to audit the one
//! place that constructs inline string literals in queries.

/// Escape a string for inline use in a Cypher query.
/// Replaces `\` → `\\` and `'` → `\'`.
pub(super) fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Escape a multi-line string (e.g. a function body) for inline Cypher use.
/// In addition to [`esc`], encodes `\n`, `\r`, `\t` so the resulting literal
/// fits on a single line and is unambiguously round-trippable.
pub(super) fn esc_multiline(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
