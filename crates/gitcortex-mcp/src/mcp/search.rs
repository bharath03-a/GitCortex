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
//! - byte-exact (case-sensitive) name:   +8 on top of the base
//! - shorter names break ties
//! - kind boost: Function/Method/Struct/Trait > others
//! - kind penalty: File -25, Folder -45 — containers are paths, not definitions
//! - Markdown `Section` headings are excluded from code search entirely

use std::collections::HashSet;

use gitcortex_core::{error::Result, graph::Node, schema::NodeKind, store::GraphStore};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
    pub score: i32,
}

/// Hits grouped by file — lets a model see "parse is in parser.rs (8 symbols)"
/// at a glance without iterating the full flat hit list.
#[derive(Debug, Clone, Serialize)]
pub struct FileGroup {
    pub file: String,
    pub symbol_count: usize,
    /// Up to 3 highest-scoring symbol names in this file.
    pub top_symbols: Vec<String>,
}

/// Group search hits by file. Groups are sorted by `symbol_count` descending
/// (most hits first), ties broken alphabetically by file path.
///
/// Input must already be sorted by score descending so `top_symbols` picks the
/// highest-scoring symbols per file without extra sorting.
pub fn group_by_file(hits: &[SearchHit]) -> Vec<FileGroup> {
    // Preserve insertion order (first-seen = highest-scored) per file.
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, (usize, Vec<String>)> =
        std::collections::HashMap::new();

    for h in hits {
        let e = map.entry(h.file.clone()).or_insert_with(|| {
            order.push(h.file.clone());
            (0, Vec::new())
        });
        e.0 += 1;
        if e.1.len() < 3 {
            e.1.push(h.name.clone());
        }
    }

    let mut groups: Vec<FileGroup> = order
        .into_iter()
        .map(|file| {
            let (count, top) = map.remove(&file).unwrap();
            FileGroup {
                file,
                symbol_count: count,
                top_symbols: top,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.symbol_count
            .cmp(&a.symbol_count)
            .then_with(|| a.file.cmp(&b.file))
    });
    groups
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
pub(crate) fn tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' || ch == '.' || ch == ':' || ch == '/' || ch == ' ' {
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
/// Bonus for a byte-exact name match, on top of the case-insensitive base.
///
/// Without it, `Searcher` and `searcher` tie on base score and the winner is
/// decided by kind boost alone — which ranked `HiArgs::searcher` (Method, +5)
/// above the `Searcher` struct (+4). Must exceed the largest kind-boost gap.
const EXACT_CASE_BONUS: i32 = 8;

/// Penalty for symbols defined in test files.
///
/// Production evidence comes first: an exactly-named test helper must not
/// outrank a real definition. Sized to drop the exact-match band (100) below
/// the prefix-match band (60), so tests stay findable but never lead.
const TEST_FILE_PENALTY: i32 = -45;

fn score(n: &Node, q_lower: &str, q_tokens: &[String], query: &str) -> Option<i32> {
    // Markdown headings are prose, not definitions. A README section named
    // after a symbol is never the answer to a code search, and lookup_symbol
    // and get_subgraph already exclude them.
    if n.kind == NodeKind::Section {
        return None;
    }
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

    let exact_case = if n.name == query { EXACT_CASE_BONUS } else { 0 };
    let test_penalty = if super::helpers::is_test_file(&n.file) {
        TEST_FILE_PENALTY
    } else {
        0
    };
    Some(base + kind_boost(&n.kind) + exact_case + test_penalty)
}

fn kind_boost(k: &NodeKind) -> i32 {
    match k {
        NodeKind::Function | NodeKind::Method => 5,
        NodeKind::Struct | NodeKind::Trait | NodeKind::Interface => 4,
        NodeKind::Enum | NodeKind::TypeAlias => 3,
        NodeKind::Constant | NodeKind::Macro | NodeKind::Annotation => 2,
        // Containers are paths, not definitions. A folder or file whose name
        // happens to match should stay findable but must never outrank a real
        // definition — ripgrep's `crates/searcher` folder was landing above
        // `SearcherBuilder`. The penalties clear the strongest weaker-match
        // band (prefix, 60) from the exact-match band (100).
        NodeKind::File => -25,
        NodeKind::Folder => -45,
        _ => 0,
    }
}

fn to_hit(n: Node, score: i32) -> SearchHit {
    SearchHit {
        id: n.id.as_str().to_owned(),
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

    // Typo-fallback: CONTAINS can't find misspelled queries ("Greetter" won't
    // match "Greeter"). When no candidates found and query is short enough for
    // edit-distance to be meaningful, scan all nodes so the scorer can apply
    // typo tolerance.
    if nodes.is_empty() && q_lower.len() >= 4 && q_lower.len() <= 20 {
        push(&mut nodes, &mut seen, store.list_all_nodes(branch)?);
    }

    let mut hits: Vec<SearchHit> = nodes
        .into_iter()
        .filter_map(|n| score(&n, &q_lower, &q_tokens, query).map(|s| to_hit(n, s)))
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
    use gitcortex_core::graph::{NodeId, NodeMetadata, Span};
    use std::path::PathBuf;

    fn node_of(kind: NodeKind, name: &str) -> Node {
        Node {
            id: NodeId::new(),
            kind,
            name: name.to_owned(),
            qualified_name: name.to_owned(),
            file: PathBuf::from("src/lib.rs"),
            span: Span {
                start_line: 1,
                end_line: 2,
            },
            metadata: NodeMetadata::default(),
        }
    }

    /// Score a node against a query the way `search` does.
    fn score_of(kind: NodeKind, name: &str, query: &str) -> Option<i32> {
        let q_lower = query.to_ascii_lowercase();
        let q_tokens = tokenize(query);
        score(&node_of(kind, name), &q_lower, &q_tokens, query)
    }

    /// Score a node that lives at an explicit path.
    fn score_at(kind: NodeKind, name: &str, file: &str, query: &str) -> Option<i32> {
        let q_lower = query.to_ascii_lowercase();
        let q_tokens = tokenize(query);
        let mut n = node_of(kind, name);
        n.file = PathBuf::from(file);
        score(&n, &q_lower, &q_tokens, query)
    }

    // ── ranking defects found by the relevance gate ──────────────────────────

    #[test]
    fn case_exact_definition_outranks_case_insensitive_method() {
        // ripgrep: `HiArgs::searcher` (Method) scored 105 and buried the
        // `Searcher` struct at 104 purely on the Method kind boost.
        let struct_hit = score_of(NodeKind::Struct, "Searcher", "Searcher").unwrap();
        let method_hit = score_of(NodeKind::Method, "searcher", "Searcher").unwrap();
        assert!(
            struct_hit > method_hit,
            "case-exact struct {struct_hit} must outrank case-insensitive method {method_hit}"
        );
    }

    #[test]
    fn case_insensitive_match_still_scores() {
        assert!(score_of(NodeKind::Method, "searcher", "Searcher").is_some());
    }

    #[test]
    fn folder_ranks_below_a_weaker_code_match() {
        // `crates/searcher` (Folder, exact name match) outranked
        // `SearcherBuilder` (Struct, prefix match) in the measured baseline.
        let folder = score_of(NodeKind::Folder, "searcher", "Searcher").unwrap();
        let prefix_struct = score_of(NodeKind::Struct, "SearcherBuilder", "Searcher").unwrap();
        assert!(
            folder < prefix_struct,
            "folder {folder} must rank below prefix-matched struct {prefix_struct}"
        );
    }

    #[test]
    fn file_ranks_below_a_definition_of_the_same_name() {
        let file = score_of(NodeKind::File, "Searcher", "Searcher").unwrap();
        let definition = score_of(NodeKind::Struct, "Searcher", "Searcher").unwrap();
        assert!(
            file < definition,
            "file {file} must rank below struct {definition}"
        );
    }

    #[test]
    fn file_is_still_findable_by_its_own_name() {
        // Demotion must not remove files from search entirely.
        assert!(score_of(NodeKind::File, "sessions.py", "sessions.py").is_some());
    }

    #[test]
    fn test_symbols_rank_below_production_symbols() {
        // gson put two src/test/ classes in the top five, and requests put a
        // test method at rank 5. The plan requires production evidence first.
        let test_exact = score_at(
            NodeKind::Method,
            "jsonReader",
            "src/test/java/NumberLimitsTest.java",
            "JsonReader",
        )
        .unwrap();
        let prod_prefix = score_at(
            NodeKind::Struct,
            "JsonReaderInternal",
            "src/main/java/JsonReaderInternal.java",
            "JsonReader",
        )
        .unwrap();
        assert!(
            test_exact < prod_prefix,
            "exact-match test symbol {test_exact} must rank below production prefix match {prod_prefix}"
        );
    }

    #[test]
    fn test_support_modules_rank_below_production() {
        // ripgrep ships `crates/searcher/src/testutil.rs`; its `SearcherTester`
        // tied `SearcherBuilder` and won on the shorter-name tie-break.
        let helper = score_at(
            NodeKind::Struct,
            "SearcherTester",
            "crates/searcher/src/testutil.rs",
            "Searcher",
        )
        .unwrap();
        let production = score_at(
            NodeKind::Struct,
            "SearcherBuilder",
            "crates/searcher/src/searcher/mod.rs",
            "Searcher",
        )
        .unwrap();
        assert!(
            helper < production,
            "test-support struct {helper} must rank below production struct {production}"
        );
    }

    #[test]
    fn test_symbols_are_still_findable() {
        // Demoted, never dropped: searching for a test by name must still work.
        assert!(score_at(
            NodeKind::Struct,
            "JsonReaderTest",
            "src/test/java/JsonReaderTest.java",
            "JsonReaderTest"
        )
        .is_some());
    }

    #[test]
    fn markdown_sections_are_excluded_from_code_search() {
        // A README heading named after a symbol is prose, not a definition.
        // lookup_symbol and get_subgraph already filter Section; search did not.
        assert_eq!(score_of(NodeKind::Section, "Searcher", "Searcher"), None);
    }

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

    // ── group_by_file ─────────────────────────────────────────────────────────

    fn hit(name: &str, file: &str, score: i32) -> SearchHit {
        SearchHit {
            id: String::new(),
            name: name.to_owned(),
            qualified_name: name.to_owned(),
            kind: "Function".to_owned(),
            file: file.to_owned(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn group_by_file_empty_returns_empty() {
        assert!(group_by_file(&[]).is_empty());
    }

    #[test]
    fn group_by_file_single_file() {
        let hits = vec![hit("parse_args", "src/parser.rs", 100)];
        let groups = group_by_file(&hits);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].file, "src/parser.rs");
        assert_eq!(groups[0].symbol_count, 1);
        assert_eq!(groups[0].top_symbols, vec!["parse_args"]);
    }

    #[test]
    fn group_by_file_sorted_by_count_desc() {
        let hits = vec![
            hit("a", "src/big.rs", 80),
            hit("b", "src/big.rs", 70),
            hit("c", "src/big.rs", 60),
            hit("x", "src/small.rs", 50),
        ];
        let groups = group_by_file(&hits);
        assert_eq!(groups[0].file, "src/big.rs");
        assert_eq!(groups[0].symbol_count, 3);
        assert_eq!(groups[1].file, "src/small.rs");
        assert_eq!(groups[1].symbol_count, 1);
    }

    #[test]
    fn group_by_file_top_symbols_capped_at_three() {
        let hits: Vec<SearchHit> = (0..8)
            .map(|i| hit(&format!("fn{i}"), "src/big.rs", 100 - i))
            .collect();
        let groups = group_by_file(&hits);
        assert_eq!(groups[0].symbol_count, 8);
        assert_eq!(groups[0].top_symbols.len(), 3);
        // First three are highest-scored (fn0, fn1, fn2).
        assert_eq!(groups[0].top_symbols[0], "fn0");
    }

    #[test]
    fn group_by_file_alphabetical_tie_break() {
        let hits = vec![hit("a", "src/z.rs", 50), hit("b", "src/a.rs", 50)];
        let groups = group_by_file(&hits);
        // Both have count=1; alphabetical tie-break: a.rs before z.rs.
        assert_eq!(groups[0].file, "src/a.rs");
    }
}
