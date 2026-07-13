//! Prose-summary generation for `get_subgraph` responses.
//!
//! The raw subgraph (nodes + edges as JSON arrays) forces a model to iterate
//! many times to extract relationship facts. A single prose `summary` string
//! delivers the same information in one read, cutting turns from 20+ to 1.
//!
//! The summary is intentionally compact and model-readable:
//!
//! ```text
//! `main` (Function) — src/main.rs:1
//!   • called by: nothing
//!   • calls: run, setup_tracing, parse_args, handle_signals
//!   • uses types: Config, Args
//!   • subgraph: 5 nodes, 4 edges (depth 1)
//! ```

use gitcortex_core::{
    graph::{Edge, Node},
    schema::EdgeKind,
};

const MAX_NAMES_IN_LIST: usize = 5;

/// Build a prose summary of the subgraph centred on `seed_name`.
///
/// Returns an empty-result hint when the subgraph has no nodes so the model
/// can suggest a follow-up action rather than silently stopping.
pub fn build_prose_summary(seed_name: &str, nodes: &[Node], edges: &[Edge], depth: u8) -> String {
    if nodes.is_empty() {
        return format!(
            "No symbol matching '{seed_name}' found in this branch's graph. \
             Try `search_code` to locate the nearest match by name."
        );
    }

    // Find the seed node (case-insensitive so "Main" matches "main").
    // nodes.is_empty() is guarded above, so nodes[0] is safe.
    let seed = nodes
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case(seed_name))
        .unwrap_or(&nodes[0]);

    let seed_id = seed.id.as_str();

    // Build id→name map for edge label resolution.
    let id_to_name: std::collections::HashMap<String, &str> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.name.as_str()))
        .collect();

    // Classify edges by kind relative to the seed.
    let mut callers: Vec<&str> = Vec::new(); // edges that point INTO seed (Calls)
    let mut callees: Vec<&str> = Vec::new(); // edges that seed points OUT (Calls)
    let mut used_types: Vec<&str> = Vec::new(); // Uses edges from seed
    let mut implements: Vec<&str> = Vec::new(); // Implements edges from seed

    for e in edges {
        let src = e.src.as_str();
        let dst = e.dst.as_str();
        match e.kind {
            EdgeKind::Calls => {
                if src == seed_id {
                    if let Some(name) = id_to_name.get(&dst) {
                        callees.push(name);
                    }
                } else if dst == seed_id {
                    if let Some(name) = id_to_name.get(&src) {
                        callers.push(name);
                    }
                }
            }
            EdgeKind::Uses if src == seed_id => {
                if let Some(name) = id_to_name.get(&dst) {
                    used_types.push(name);
                }
            }
            EdgeKind::Implements if src == seed_id => {
                if let Some(name) = id_to_name.get(&dst) {
                    implements.push(name);
                }
            }
            _ => {}
        }
    }

    callers.sort();
    callers.dedup();
    callees.sort();
    callees.dedup();
    used_types.sort();
    used_types.dedup();
    implements.sort();
    implements.dedup();

    let calls_edges: Vec<&Edge> = edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Calls))
        .collect();
    let direct = calls_edges
        .iter()
        .filter(|e| {
            matches!(
                e.confidence,
                gitcortex_core::schema::EdgeConfidence::Extracted
            )
        })
        .count();
    let inferred = calls_edges.len() - direct;
    let conf_note = if calls_edges.is_empty() {
        String::new()
    } else {
        format!(", {direct} direct + {inferred} inferred calls")
    };

    let file_line = format!("{}:{}", seed.file.display(), seed.span.start_line);

    let mut lines = vec![format!("`{}` ({}) — {}", seed.name, seed.kind, file_line)];

    lines.push(format!("  • called by: {}", format_list(&callers)));
    lines.push(format!("  • calls: {}", format_list(&callees)));

    if !used_types.is_empty() {
        lines.push(format!("  • uses types: {}", format_list(&used_types)));
    }
    if !implements.is_empty() {
        lines.push(format!("  • implements: {}", format_list(&implements)));
    }

    lines.push(format!(
        "  • subgraph: {} nodes, {} edges (depth {depth}{})",
        nodes.len(),
        edges.len(),
        conf_note
    ));

    lines.join("\n")
}

fn format_list(names: &[&str]) -> String {
    if names.is_empty() {
        return "nothing".to_owned();
    }
    let shown: Vec<&str> = names.iter().copied().take(MAX_NAMES_IN_LIST).collect();
    let extra = names.len().saturating_sub(MAX_NAMES_IN_LIST);
    if extra == 0 {
        shown.join(", ")
    } else {
        format!("{} (+{} more)", shown.join(", "), extra)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gitcortex_core::{
        graph::{Edge, NodeId, NodeMetadata, Span},
        schema::{EdgeKind, NodeKind},
    };

    use super::*;
    use gitcortex_core::graph::Node;

    fn node(name: &str) -> Node {
        Node {
            id: NodeId::new(),
            kind: NodeKind::Function,
            name: name.to_owned(),
            qualified_name: name.to_owned(),
            file: PathBuf::from(format!("src/{name}.rs")),
            span: Span {
                start_line: 1,
                end_line: 10,
            },
            metadata: NodeMetadata::default(),
        }
    }

    // ── empty ────────────────────────────────────────────────────────────────

    #[test]
    fn empty_nodes_returns_not_found_hint() {
        let s = build_prose_summary("Main", &[], &[], 1);
        assert!(s.contains("No symbol matching 'Main'"), "got: {s}");
        assert!(s.contains("search_code"), "should suggest search_code: {s}");
    }

    // ── seed only ────────────────────────────────────────────────────────────

    #[test]
    fn seed_only_no_connections() {
        let n = node("main");
        let s = build_prose_summary("main", &[n], &[], 1);
        assert!(s.contains("`main`"), "got: {s}");
        assert!(s.contains("called by: nothing"), "got: {s}");
        assert!(s.contains("calls: nothing"), "got: {s}");
        assert!(s.contains("1 nodes, 0 edges"), "got: {s}");
    }

    // ── callers and callees ──────────────────────────────────────────────────

    #[test]
    fn callers_and_callees_appear_in_prose() {
        let seed = node("main");
        let caller = node("bootstrap");
        let callee1 = node("run");
        let callee2 = node("parse_args");

        let seed_id = seed.id.clone();
        let caller_id = caller.id.clone();
        let callee1_id = callee1.id.clone();
        let callee2_id = callee2.id.clone();

        let nodes = vec![seed, caller, callee1, callee2];
        let edges = vec![
            Edge::new(caller_id, seed_id.clone(), EdgeKind::Calls),
            Edge::new(seed_id.clone(), callee1_id, EdgeKind::Calls),
            Edge::new(seed_id, callee2_id, EdgeKind::Calls),
        ];

        let s = build_prose_summary("main", &nodes, &edges, 1);
        assert!(s.contains("bootstrap"), "caller missing: {s}");
        assert!(s.contains("run"), "callee1 missing: {s}");
        assert!(s.contains("parse_args"), "callee2 missing: {s}");
    }

    // ── uses / implements ────────────────────────────────────────────────────

    #[test]
    fn uses_and_implements_edges_appear() {
        let seed = node("Router");
        let iface = node("Handler");
        let typ = node("Request");

        let seed_id = seed.id.clone();
        let iface_id = iface.id.clone();
        let typ_id = typ.id.clone();

        let nodes = vec![seed, iface, typ];
        let edges = vec![
            Edge::new(seed_id.clone(), iface_id, EdgeKind::Implements),
            Edge::new(seed_id, typ_id, EdgeKind::Uses),
        ];

        let s = build_prose_summary("Router", &nodes, &edges, 1);
        assert!(s.contains("Handler"), "implements missing: {s}");
        assert!(s.contains("Request"), "uses missing: {s}");
        assert!(s.contains("uses types"), "uses types label missing: {s}");
        assert!(s.contains("implements"), "implements label missing: {s}");
    }

    // ── cap at MAX_NAMES_IN_LIST ─────────────────────────────────────────────

    #[test]
    fn long_callee_list_is_truncated_with_plus_more() {
        let seed = node("hub");
        let seed_id = seed.id.clone();
        let callees: Vec<Node> = (0..8).map(|i| node(&format!("callee{i}"))).collect();
        let mut nodes = vec![seed];
        let mut edges = Vec::new();
        for c in &callees {
            edges.push(Edge::new(seed_id.clone(), c.id.clone(), EdgeKind::Calls));
        }
        nodes.extend(callees);

        let s = build_prose_summary("hub", &nodes, &edges, 1);
        assert!(s.contains("+3 more"), "expected +3 more for 8 callees: {s}");
    }

    // ── case-insensitive seed matching ───────────────────────────────────────

    #[test]
    fn seed_found_case_insensitively() {
        let n = node("main"); // stored as lowercase
        let s = build_prose_summary("Main", &[n], &[], 1); // queried with capital M
        assert!(
            s.contains("`main`"),
            "should find seed case-insensitively: {s}"
        );
        assert!(!s.contains("No symbol"), "should not return not-found: {s}");
    }
}
