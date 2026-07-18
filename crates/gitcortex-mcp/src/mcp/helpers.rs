use std::path::Path;

use gitcortex_core::schema::{EdgeConfidence, NodeKind, Visibility};

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

/// Sort key for edge confidence: lower = higher quality caller.
pub(crate) fn confidence_rank(c: &EdgeConfidence) -> u8 {
    match c {
        EdgeConfidence::Extracted => 0,
        EdgeConfidence::Resolved => 1,
        EdgeConfidence::Inferred => 2,
    }
}

/// True when a file path looks like a test file (demote in ranked heads).
pub(crate) fn is_test_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    // directory components: /tests/, /test/, /spec/, /__tests__/
    if s.contains("/tests/")
        || s.contains("/test/")
        || s.contains("/spec/")
        || s.contains("/__tests__/")
    {
        return true;
    }
    // filename patterns: test_*.rs, *_test.rs, *_spec.rs, *.test.ts, *.spec.ts, etc.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("test_")
            || name.ends_with("_test.rs")
            || name.ends_with("_spec.rs")
            || name.ends_with(".test.ts")
            || name.ends_with(".spec.ts")
            || name.ends_with(".test.js")
            || name.ends_with(".spec.js")
            || name.ends_with("_test.go")
            || name.ends_with("Test.java")
            || name.ends_with("Spec.java")
        {
            return true;
        }
    }
    false
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
