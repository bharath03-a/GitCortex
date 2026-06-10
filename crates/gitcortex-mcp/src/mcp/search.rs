//! Fuzzy search over the graph — multi-signal ranking with CamelCase/snake_case
//! tokenisation, token overlap scoring, and edit-distance typo tolerance.
//!
//! Ranking signals (higher score = better match):
//! - exact name match:                   +100
//! - prefix name match:                  +60
//! - all query tokens match name tokens: +50
//! - substring in name:                  +30
//! - partial token overlap:              +10..+25
//! - edit distance ≤1 (typo):            +20
//! - edit distance ≤2:                   +10
//! - substring in qualified_name only:   +10
//! - shorter names break ties
//! - kind boost: Function/Method/Struct/Trait > others

use std::collections::HashSet;

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

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 200;
const MIN_TOKEN_LEN: usize = 3;

/// Split a camelCase/snake_case/PascalCase identifier into lowercase tokens.
///
/// "AuthConfig"      → ["auth", "config"]
/// "validate_token"  → ["validate", "token"]
/// "parseJSONResponse" → ["parse", "j", "s", "o", "n", "response"]  (intentional — acronyms split per char)
/// "HTTPClient"      → ["h", "t", "t", "p", "client"]
fn tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' || ch == '.' || ch == ':' || ch == '/' {
            if !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current = String::new();
            }
        } else if ch.is_uppercase() {
            // Start new token on uppercase — but keep run of capitals together
            // as one token (e.g. "HTTP" stays "http" not split per char).
            let next_is_lower = chars.get(i + 1).map(|c| c.is_lowercase()).unwrap_or(false);
            let prev_is_upper = i > 0 && chars[i - 1].is_uppercase();
            if !current.is_empty() && (!prev_is_upper || next_is_lower) {
                tokens.push(current.to_ascii_lowercase());
                current = String::new();
            }
            current.push(ch.to_ascii_lowercase());
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }
    tokens
}

/// Levenshtein edit distance between two strings (capped early at `max`).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    // Quick bounds: length difference alone is a lower bound.
    if m.abs_diff(n) > 3 {
        return usize::MAX;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Score a node against a query. Returns `None` when the node is not a match.
fn score(n: &Node, q_lower: &str, q_tokens: &[String]) -> Option<i32> {
    let name_lower = n.name.to_ascii_lowercase();
    let qname_lower = n.qualified_name.to_ascii_lowercase();
    let name_tokens = tokenize(&n.name);

    let base = if name_lower == q_lower {
        // Exact name match — highest confidence.
        100
    } else if name_lower.starts_with(q_lower) {
        60
    } else if !q_tokens.is_empty() && q_tokens.iter().all(|t| name_tokens.contains(t)) {
        // All query tokens present in name tokens.
        // "auth config" fully matches "AuthConfig" or "auth_config".
        50
    } else if name_lower.contains(q_lower) {
        30
    } else {
        // Partial token overlap.
        let overlap = q_tokens
            .iter()
            .filter(|qt| qt.len() >= MIN_TOKEN_LEN && name_tokens.contains(*qt))
            .count();
        if overlap > 0 {
            10 + (overlap as i32 * 5).min(15)
        } else if qname_lower.contains(q_lower) {
            // Match only in qualified path (e.g. module prefix).
            10
        } else if q_lower.len() >= 4 && q_lower.len() <= 15 && name_lower.len() <= 25 {
            // Typo tolerance: edit distance on short-ish queries.
            let dist = edit_distance(q_lower, &name_lower);
            if dist <= 1 {
                20
            } else if dist <= 2 {
                10
            } else {
                return None;
            }
        } else {
            return None;
        }
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

/// Run a fuzzy search across all nodes on `branch`.
///
/// Candidate set is built by querying the store for the whole query string AND
/// for each individual token (for multi-word / camelCase queries). Candidates
/// are deduplicated, scored with the multi-signal scorer, sorted by score
/// descending, and truncated to `limit`.
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

    let q_lower = q.to_ascii_lowercase();
    let q_tokens = tokenize(q);
    let candidate_limit = (limit * 50).max(500);

    // Fetch candidates: whole query first, then per token.
    let mut seen: HashSet<String> = HashSet::new();
    let mut nodes: Vec<Node> = Vec::new();

    let push = |nodes: &mut Vec<Node>, seen: &mut HashSet<String>, batch: Vec<Node>| {
        for n in batch {
            let id = n.id.as_str();
            if seen.insert(id) {
                nodes.push(n);
            }
        }
    };

    push(
        &mut nodes,
        &mut seen,
        store.search_nodes(branch, q, candidate_limit)?,
    );

    // Per-token expansion: lets "validate token" find "validate_token" even
    // when the store's CONTAINS filter requires the full substring.
    for token in &q_tokens {
        if token.len() < MIN_TOKEN_LEN {
            continue;
        }
        // Skip if token equals the whole query (already fetched above).
        if token.as_str() == q_lower {
            continue;
        }
        push(
            &mut nodes,
            &mut seen,
            store.search_nodes(branch, token, candidate_limit)?,
        );
    }

    let mut hits: Vec<SearchHit> = nodes
        .into_iter()
        .filter_map(|n| score(&n, &q_lower, &q_tokens).map(|s| to_hit(n, s)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_camel_case() {
        assert_eq!(tokenize("AuthConfig"), vec!["auth", "config"]);
        assert_eq!(tokenize("validateToken"), vec!["validate", "token"]);
        assert_eq!(tokenize("HTTPClient"), vec!["http", "client"]);
    }

    #[test]
    fn tokenize_snake_case() {
        assert_eq!(tokenize("validate_token"), vec!["validate", "token"]);
        assert_eq!(tokenize("auth_middleware"), vec!["auth", "middleware"]);
    }

    #[test]
    fn tokenize_pascal_case() {
        assert_eq!(tokenize("KuzuGraphStore"), vec!["kuzu", "graph", "store"]);
    }

    #[test]
    fn edit_distance_exact() {
        assert_eq!(edit_distance("validate", "validate"), 0);
    }

    #[test]
    fn edit_distance_typo() {
        assert_eq!(edit_distance("vlidate", "validate"), 1);
        assert_eq!(edit_distance("authnticate", "authenticate"), 1);
    }

    #[test]
    fn edit_distance_length_short_circuit() {
        // length difference > 3 → MAX
        assert_eq!(edit_distance("a", "abcde"), usize::MAX);
    }
}
