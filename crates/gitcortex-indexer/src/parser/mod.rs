use std::path::Path;

use gitcortex_core::{
    error::Result,
    graph::{Edge, Node, NodeId},
};

pub mod go;
pub mod python;
pub mod rust;
pub mod typescript;

/// Result of parsing a single source file.
pub struct ParseResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Unresolved call sites — resolved cross-file by the indexer.
    pub deferred_calls: Vec<(NodeId, String)>,
    /// Unresolved parameter/return-type references: (fn_id, type_name).
    pub deferred_uses: Vec<(NodeId, String)>,
    /// Unresolved trait implementations: (struct_id, trait_name).
    pub deferred_implements: Vec<(NodeId, String)>,
    /// Unresolved use-declaration imports: (src_node_id, imported_leaf_name).
    pub deferred_imports: Vec<(NodeId, String)>,
}

/// Contract every language parser must satisfy.
///
/// Implementations are stateless — a parser value is cheap to create and safe
/// to reuse across files. Parsing is purely functional: source text in, graph
/// nodes + edges out. Cross-file edges are resolved by the indexer after all
/// files in the diff have been parsed.
pub trait LanguageParser: Send + Sync {
    /// File extensions this parser handles (lower-case, without the dot).
    fn extensions(&self) -> &[&str];

    /// Parse `source` (content of `path`) and return all nodes, edges, and
    /// unresolved call references found in that file.
    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult>;
}

/// Return the appropriate parser for `path`, keyed on file extension.
/// Returns `None` when the extension is unsupported.
pub fn parser_for_path(path: &Path) -> Option<Box<dyn LanguageParser>> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs"  => Some(Box::new(rust::RustParser::new())),
        "py"  => Some(Box::new(python::PythonParser::new())),
        "ts"  => Some(Box::new(typescript::TypeScriptParser::new_ts())),
        "tsx" => Some(Box::new(typescript::TypeScriptParser::new_tsx())),
        "js" | "mjs" | "cjs" => Some(Box::new(typescript::JavaScriptParser::new())),
        "jsx" => Some(Box::new(typescript::JavaScriptParser::new())),
        "go"  => Some(Box::new(go::GoParser::new())),
        _ => None,
    }
}
