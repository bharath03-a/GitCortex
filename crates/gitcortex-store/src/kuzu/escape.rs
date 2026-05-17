//! Cypher string escaping. Kept separate to make it easy to audit the one
//! place that constructs inline string literals in queries.

/// Escape a string for inline use in a Cypher query.
/// Replaces `\` → `\\` and `'` → `\'`.
pub(super) fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}
