use std::collections::{BTreeMap, BTreeSet, HashMap};

use gitcortex_core::schema::NodeKind;

use super::cypher::kind_dot_color;
use crate::export::ScopedGraph;

/// Render a [`ScopedGraph`] as a GitHub-compatible Mermaid `flowchart LR`.
///
/// Single-seed export renderer: emits deterministic node ids (`n0..nK`),
/// escapes hostile characters in symbol labels, adds `classDef` styling per
/// emitted [`NodeKind`] plus a `seed` class when `g.seed` is present, and
/// inserts a `%% truncated: showing X of Y nodes` comment immediately after the
/// header when the graph was capped.
pub fn to_mermaid(g: &ScopedGraph) -> String {
    let mut out = String::from("flowchart LR\n");
    if g.truncated {
        out.push_str(&format!(
            "%% truncated: showing {} of {} nodes\n",
            g.nodes.len(),
            g.total_nodes
        ));
    }

    let mut id_map: HashMap<String, String> = HashMap::with_capacity(g.nodes.len());
    let mut kinds_used: BTreeMap<String, &NodeKind> = BTreeMap::new();
    let mut by_class: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let seed_id = g.seed.as_ref().map(|s| s.id.as_str());
    let mut seed_mermaid_id: Option<String> = None;

    for (idx, n) in g.nodes.iter().enumerate() {
        let mid = format!("n{idx}");
        let raw_id = n.id.as_str();
        if seed_id.as_deref() == Some(raw_id.as_str()) {
            seed_mermaid_id = Some(mid.clone());
        }
        id_map.insert(raw_id, mid.clone());

        let label = format!("{}<br/>{}", mermaid_escape(&n.name), n.kind);
        out.push_str(&format!("  {mid}[\"{label}\"]\n"));

        let kind_key = n.kind.to_string();
        kinds_used.entry(kind_key.clone()).or_insert(&n.kind);
        by_class.entry(kind_key).or_default().push(mid);
    }

    let mut emitted_edges: BTreeSet<(String, String, String)> = BTreeSet::new();
    for e in &g.edges {
        let (Some(src), Some(dst)) = (id_map.get(&e.src.as_str()), id_map.get(&e.dst.as_str()))
        else {
            continue;
        };
        let kind = e.kind.to_string();
        if emitted_edges.insert((src.clone(), kind.clone(), dst.clone())) {
            out.push_str(&format!("  {src} -->|{kind}| {dst}\n"));
        }
    }

    for (kind_key, kind) in &kinds_used {
        let fill = kind_dot_color(kind);
        out.push_str(&format!(
            "  classDef {kind_key} fill:{fill},stroke:#1e1e2e,color:#1e1e2e\n"
        ));
    }
    if seed_mermaid_id.is_some() {
        out.push_str("  classDef seed stroke:#f38ba8,stroke-width:3px\n");
    }

    for (kind_key, members) in &by_class {
        out.push_str(&format!("  class {} {kind_key}\n", members.join(",")));
    }
    if let Some(seed_mid) = seed_mermaid_id {
        out.push_str(&format!("  class {seed_mid} seed\n"));
    }

    out
}

pub(crate) fn mermaid_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("#quot;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '|' => out.push_str("#124;"),
            '`' => out.push_str("#96;"),
            '\r' | '\n' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use gitcortex_core::{
        graph::{Edge, Node, NodeId, Span},
        schema::{EdgeKind, NodeKind},
    };

    use super::*;

    fn node(name: &str, kind: NodeKind, line: u32) -> Node {
        Node {
            id: NodeId::stable(name),
            name: name.into(),
            qualified_name: format!("test::{name}"),
            kind,
            file: "src/lib.rs".into(),
            span: Span {
                start_line: line,
                end_line: line + 1,
            },
            metadata: Default::default(),
        }
    }

    #[test]
    fn mermaid_header_and_edges() {
        let a = node("alpha", NodeKind::Function, 1);
        let b = node("Beta", NodeKind::Struct, 10);
        let edge = Edge::new(a.id.clone(), b.id.clone(), EdgeKind::Calls);
        let g = ScopedGraph::from_explicit("main".into(), vec![a, b], vec![edge]).unwrap();

        let rendered = to_mermaid(&g);
        assert!(rendered.starts_with("flowchart LR\n"), "{rendered}");
        assert!(
            rendered.contains("  n0[\"alpha<br/>function\"]\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  n1[\"Beta<br/>struct\"]\n"),
            "{rendered}"
        );
        assert!(rendered.contains("  n0 -->|calls| n1\n"), "{rendered}");
        assert!(rendered.contains("  classDef function "), "{rendered}");
        assert!(rendered.contains("  classDef struct "), "{rendered}");
        assert!(rendered.contains("  class n0 function\n"), "{rendered}");
        assert!(rendered.contains("  class n1 struct\n"), "{rendered}");
    }

    #[test]
    fn mermaid_escapes_hostile_names() {
        let hostile = node("\"x<script>`|\"</script>λ\n[b]", NodeKind::Function, 1);
        let g = ScopedGraph::from_explicit("main".into(), vec![hostile], vec![]).unwrap();

        let rendered = to_mermaid(&g);
        assert!(!rendered.contains("<script>"), "{rendered}");
        assert!(!rendered.contains("</script>"), "{rendered}");
        assert!(!rendered.contains('`'), "{rendered}");
        assert!(rendered.contains("#quot;"), "{rendered}");
        assert!(rendered.contains("#lt;script#gt;"), "{rendered}");
        assert!(rendered.contains("#96;"), "{rendered}");
        assert!(rendered.contains("#124;"), "{rendered}");
        assert!(rendered.contains('λ'), "{rendered}");
        assert!(rendered.contains("<br/>function"), "{rendered}");
    }

    #[test]
    fn mermaid_stable_output() {
        let a = node("alpha", NodeKind::Function, 1);
        let b = node("beta", NodeKind::Function, 5);
        let edge = Edge::new(a.id.clone(), b.id.clone(), EdgeKind::Calls);
        let g = ScopedGraph::from_explicit("main".into(), vec![a, b], vec![edge]).unwrap();

        assert_eq!(to_mermaid(&g), to_mermaid(&g));
    }

    #[test]
    fn mermaid_truncation_comment() {
        let a = node("alpha", NodeKind::Function, 1);
        let b = node("beta", NodeKind::Function, 5);
        let g = ScopedGraph {
            branch: "main".into(),
            seed: Some(a.clone()),
            nodes: vec![a, b],
            edges: vec![],
            truncated: true,
            total_nodes: 250,
            total_edges: 400,
        };

        let rendered = to_mermaid(&g);
        let mut lines = rendered.lines();
        assert_eq!(lines.next(), Some("flowchart LR"));
        assert_eq!(lines.next(), Some("%% truncated: showing 2 of 250 nodes"));
        assert!(rendered.contains("  classDef seed "), "{rendered}");
        assert!(rendered.contains("  class n0 seed\n"), "{rendered}");
    }
}
