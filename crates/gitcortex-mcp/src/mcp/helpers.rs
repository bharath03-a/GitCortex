use std::path::Path;

use gitcortex_core::schema::{NodeKind, Visibility};

/// First line of a node's captured signature, trimmed and length-capped, for
/// embedding in tool results so the model can judge a symbol without opening
/// its file. Empty string when no signature was captured.
pub(crate) fn sig_line(n: &gitcortex_core::graph::Node) -> String {
    const MAX: usize = 120;
    let first = n
        .metadata
        .definition
        .signature
        .lines()
        .next()
        .unwrap_or("")
        .trim();
    if first.chars().count() > MAX {
        let truncated: String = first.chars().take(MAX).collect();
        format!("{truncated}…")
    } else {
        first.to_owned()
    }
}

/// Parse a NodeKind from its snake_case string form (matches `NodeKind::Display`).
pub(crate) fn parse_node_kind(s: &str) -> Option<NodeKind> {
    s.parse().ok()
}

/// Parse a Visibility from its snake_case string form.
pub(crate) fn parse_visibility(s: &str) -> Option<Visibility> {
    s.parse().ok()
}

pub(crate) fn detect_current_branch(repo_root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let b = s.trim().to_owned();
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    } else {
        None
    }
}
