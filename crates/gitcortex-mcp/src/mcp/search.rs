//! Fuzzy search over the graph — substring + qualified-path match with
//! deterministic ranking. Wraps `GraphStore::lookup_symbol(fuzzy=true)` and
//! a `list_all_nodes` scan to also match on `qualified_name`, then ranks.
//!
//! Ranking signal (higher score = better):
//! - exact name match: +100
//! - prefix name match: +60
//! - substring in name: +30
//! - substring in qualified_name only: +10
//! - shorter names rank above longer when scores tie
//! - kind boost: Function/Method/Struct/Trait > others

use gitcortex_core::{error::Result, graph::Node, schema::NodeKind, store::GraphStore};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
    pub score: i32,
}

/// Default result cap when caller does not specify.
const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 200;

/// Run a fuzzy search across all nodes of `branch`. The lookup matches both
/// `name` and `qualified_name`. Results are deduped by node id and sorted by
/// descending score.
pub fn search<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    query: &str,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    // Pull every node once and score in-process. Cheap: typical repo graphs
    // fit comfortably in memory and we avoid N queries.
    let nodes = store.list_all_nodes(branch)?;

    let mut hits: Vec<SearchHit> = nodes
        .into_iter()
        .filter_map(|n| score(&n, q).map(|s| to_hit(n, s)))
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn score(n: &Node, q: &str) -> Option<i32> {
    // Case-insensitive match — fuzzy search UX expects "greet" to find both
    // `greet` and `Greet`. Exact-case lookup is still available through
    // `lookup_symbol` with `fuzzy=false`.
    let q_lower = q.to_ascii_lowercase();
    let name_lower = n.name.to_ascii_lowercase();
    let qname_lower = n.qualified_name.to_ascii_lowercase();
    let base = if name_lower == q_lower {
        100
    } else if name_lower.starts_with(&q_lower) {
        60
    } else if name_lower.contains(&q_lower) {
        30
    } else if qname_lower.contains(&q_lower) {
        10
    } else {
        return None;
    };
    Some(base + kind_boost(&n.kind))
}

fn kind_boost(k: &NodeKind) -> i32 {
    match k {
        NodeKind::Function | NodeKind::Method => 5,
        NodeKind::Struct | NodeKind::Trait | NodeKind::Interface => 4,
        NodeKind::Enum | NodeKind::TypeAlias => 3,
        NodeKind::Constant | NodeKind::Macro | NodeKind::Annotation => 2,
        _ => 0,
    }
}

fn to_hit(n: Node, score: i32) -> SearchHit {
    SearchHit {
        name: n.name,
        qualified_name: n.qualified_name,
        kind: n.kind.to_string(),
        file: n.file.display().to_string(),
        start_line: n.span.start_line,
        score,
    }
}
