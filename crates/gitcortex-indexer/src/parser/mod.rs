use std::path::Path;

use gitcortex_core::{
    error::Result,
    graph::{Edge, Node},
};

pub mod rust;

/// Contract every language parser must satisfy.
///
/// Each implementation is stateless — a new parser value can be created cheaply
/// and reused across files. Parsing is purely functional: source text in,
/// graph nodes + edges out.
pub trait LanguageParser: Send + Sync {
    /// File extensions this parser handles (lower-case, without the dot).
    fn extensions(&self) -> &[&str];

    /// Parse `source` (content of `path`) and return all nodes and edges
    /// found in that file. Returned edges are intra-file only; cross-file
    /// edges are resolved by the store at query time.
    fn parse(&self, path: &Path, source: &str) -> Result<(Vec<Node>, Vec<Edge>)>;
}

/// Return the appropriate parser for `path`, keyed on file extension.
/// Returns `None` when the extension is unsupported.
pub fn parser_for_path(path: &Path) -> Option<Box<dyn LanguageParser>> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(Box::new(rust::RustParser::new())),
        _ => None,
    }
}
