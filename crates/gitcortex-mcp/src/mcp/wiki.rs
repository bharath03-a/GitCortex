//! Wiki rendering — markdown summary for a single symbol assembled from
//! the graph store. Pure formatter: no I/O beyond the store reads.
//!
//! Output shape (markdown):
//!
//! ```text
//! # <name> (<kind>)
//!
//! **Defined in** `<file>:<start>-<end>` · visibility=<vis> · async=<bool> ...
//!
//! ## Signature
//! ```<lang>
//! <signature>
//! ```
//!
//! ## Doc
//! <doc_comment>
//!
//! ## Callers (N)
//! - <name> (<kind>) — <file>:<line>
//!
//! ## Calls (N)
//! - …
//!
//! ## Used by (N)
//! - …
//! ```

use std::fmt::Write;

use gitcortex_core::{
    error::Result,
    graph::Node,
    store::{GraphStore, SymbolContext},
};

/// Markdown wiki rendering for `name` on `branch`.
/// Returns an `Err` only when the store itself fails; "symbol not found" is
/// surfaced by the upstream `symbol_context` error.
pub fn render_symbol<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    name: &str,
) -> Result<String> {
    let ctx = store.symbol_context(branch, name)?;
    Ok(format(ctx))
}

fn format(ctx: SymbolContext) -> String {
    let def = &ctx.definition;
    let lang = file_lang(&def.file.to_string_lossy());
    let mut out = String::with_capacity(1024);

    let _ = writeln!(out, "# {} ({})", def.name, def.kind);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "**Defined in** `{}:{}-{}`  ·  visibility={}  ·  async={}  ·  loc={}",
        def.file.display(),
        def.span.start_line,
        def.span.end_line,
        def.metadata.visibility,
        def.metadata.is_async,
        def.metadata.loc,
    );
    if def.qualified_name != def.name {
        let _ = writeln!(out, "**Qualified** `{}`", def.qualified_name);
    }
    let _ = writeln!(out);

    let sig = def.metadata.definition.signature.trim();
    if !sig.is_empty() {
        let _ = writeln!(out, "## Signature");
        let _ = writeln!(out, "```{lang}");
        let _ = writeln!(out, "{sig}");
        let _ = writeln!(out, "```");
        let _ = writeln!(out);
    }

    if let Some(doc) = def.metadata.definition.doc_comment.as_deref() {
        let stripped = strip_doc_markers(doc);
        if !stripped.trim().is_empty() {
            let _ = writeln!(out, "## Doc");
            let _ = writeln!(out, "{}", stripped.trim());
            let _ = writeln!(out);
        }
    }

    write_neighbor_list(&mut out, "Callers", &ctx.callers);
    write_neighbor_list(&mut out, "Calls", &ctx.callees);
    write_neighbor_list(&mut out, "Used by", &ctx.used_by);

    out
}

const WIKI_NEIGHBOR_LIMIT: usize = 5;

fn write_neighbor_list(out: &mut String, label: &str, nodes: &[Node]) {
    if nodes.is_empty() {
        return;
    }
    let shown = nodes.len().min(WIKI_NEIGHBOR_LIMIT);
    let _ = writeln!(out, "## {label} ({})", nodes.len());
    for n in &nodes[..shown] {
        let _ = writeln!(
            out,
            "- `{}` ({})  — `{}:{}`",
            n.name,
            n.kind,
            n.file.display(),
            n.span.start_line
        );
    }
    if nodes.len() > shown {
        let _ = writeln!(out, "- _+{} more — use `find_callers` for the full list_", nodes.len() - shown);
    }
    let _ = writeln!(out);
}

/// Strip per-line `///`, `//!`, `// `, `# `, `*` doc-comment leaders so the
/// rendered markdown reads as prose, not as code-fence content.
fn strip_doc_markers(doc: &str) -> String {
    let mut out = String::with_capacity(doc.len());
    for line in doc.lines() {
        let trimmed = line.trim_start();
        let cleaned = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"))
            .or_else(|| trimmed.strip_prefix("/**"))
            .or_else(|| trimmed.strip_prefix("*/"))
            .or_else(|| trimmed.strip_prefix("//"))
            .or_else(|| trimmed.strip_prefix("# "))
            .or_else(|| trimmed.strip_prefix("#"))
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed);
        // Also strip a trailing `*/` (single-line javadoc /** … */).
        let cleaned = cleaned
            .trim_end()
            .strip_suffix("*/")
            .unwrap_or(cleaned)
            .trim_end();
        out.push_str(cleaned.trim_start());
        out.push('\n');
    }
    out
}

/// Best-effort language hint from a file path, for fenced code-block tagging.
fn file_lang(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "go" => "go",
        "java" => "java",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_from_path() {
        assert_eq!(file_lang("src/main.rs"), "rust");
        assert_eq!(file_lang("app/foo.tsx"), "typescript");
        assert_eq!(file_lang("Makefile"), "");
    }

    #[test]
    fn strip_rust_doc_markers() {
        let input = "/// First line\n/// Second line\n";
        let out = strip_doc_markers(input);
        assert!(out.contains("First line"));
        assert!(!out.contains("///"));
    }
}
