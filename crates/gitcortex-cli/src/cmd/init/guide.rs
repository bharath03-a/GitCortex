//! One source for the agent guidance every editor integration writes. It is
//! loaded into each agent session, so it stays short.

use std::{fs, path::Path};

use anyhow::{Context, Result};

use super::helpers::write_atomic;

pub const CORE_RULES: &str = "\
This repo has a GitCortex code graph (definitions, calls, imports, types), kept in sync by git hooks.

- Before editing, renaming, or deleting a function or type, run `pre_edit_impact` (MCP `gcx` tool) or `gcx blast-radius --base main --head HEAD`.
- For \"who calls X\", \"what does X call\", \"where is type T used\", or \"how does A reach B\", ask the graph (`find_callers`, `find_callees`, `find_type_usages`, `trace_path`) before grepping.
- Use grep for exact strings, messages, config, and comments.
- Read `index.state`: only `ready` makes an empty result a real absence. If `stale` or `missing`, run `gcx hook` or `gcx init`.
- Graph output is static evidence; confirm in source and tests before calling a change safe.
- Without MCP: `gcx query find-callers <name>`, `gcx query symbol-context <name>`, `gcx query search <text>`.
";

/// Marker-delimited block so re-running `gcx init` replaces it in place and
/// `gcx deinit` can strip it. `marker` keeps each editor's historic marker.
pub fn managed_block(marker: &str, extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("{extra}\n")
    };
    format!(
        "{}\n## GitCortex knowledge graph\n\n{CORE_RULES}{extra}{}\n",
        start_marker(marker),
        end_marker(marker)
    )
}

pub fn start_marker(marker: &str) -> String {
    format!("<!-- >>> gitcortex {marker} >>> -->")
}

pub fn end_marker(marker: &str) -> String {
    format!("<!-- <<< gitcortex {marker} <<< -->")
}

/// Write `block` into `path`: replace an existing managed block, otherwise
/// append. Returns whether the file changed.
pub fn upsert_block(path: &Path, marker: &str, block: &str) -> Result<bool> {
    let existing = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", path.display()));
        }
    };
    let updated = match existing.as_deref() {
        None => block.to_owned(),
        Some(text) => replace_block(text, marker, block).unwrap_or_else(|| {
            let prefix = text.trim_end();
            if prefix.is_empty() {
                block.to_owned()
            } else {
                format!("{prefix}\n\n{block}")
            }
        }),
    };
    if existing.as_deref() == Some(updated.as_str()) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomic(path, &updated).with_context(|| format!("write {}", path.display()))?;
    Ok(true)
}

fn replace_block(text: &str, marker: &str, block: &str) -> Option<String> {
    let start = text.find(&start_marker(marker))?;
    let end_tag = end_marker(marker);
    let end = start + text[start..].find(&end_tag)? + end_tag.len();
    let rest = text[end..].strip_prefix('\n').unwrap_or(&text[end..]);
    Some(format!("{}{block}{rest}", &text[..start]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_a_stale_block_and_keeps_neighbors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("AGENTS.md");
        let old = format!(
            "# Mine\n\n{}\nold text\n{}\nafter\n",
            start_marker("codex integration"),
            end_marker("codex integration")
        );
        fs::write(&path, old).expect("seed");
        let block = managed_block("codex integration", "");
        assert!(upsert_block(&path, "codex integration", &block).expect("upsert"));
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.starts_with("# Mine\n\n"));
        assert!(text.ends_with("after\n"));
        assert!(text.contains(CORE_RULES));
        assert!(!text.contains("old text"));
        assert!(!upsert_block(&path, "codex integration", &block).expect("again"));
    }

    #[test]
    fn core_rules_stay_short() {
        assert!(CORE_RULES.len() <= 1_000, "{} bytes", CORE_RULES.len());
    }
}
