use crate::export::ScopedGraph;
use gitcortex_core::schema::NodeKind;
pub fn to_cypher(g: &ScopedGraph) -> String {
    let branch = g.branch.as_str();
    let nodes = &g.nodes;
    let edges = &g.edges;
    let mut out = String::new();
    out.push_str(&format!(
        "// GitCortex Cypher export — branch {branch} — {} nodes, {} edges\n",
        nodes.len(),
        edges.len()
    ));
    for n in nodes {
        out.push_str(&format!(
            "CREATE (`{id}`:Symbol {{name: '{name}', kind: '{kind}', file: '{file}', qualified_name: '{qname}', start_line: {line}}});\n",
            id = cypher_id(&n.id.as_str()),
            name = cypher_str(&n.name),
            kind = n.kind,
            file = cypher_str(&n.file.display().to_string()),
            qname = cypher_str(&n.qualified_name),
            line = n.span.start_line
        ));
    }
    for e in edges {
        // Map to ALL_CAPS relationship types per Neo4j convention.
        let rel = e.kind.to_string().to_uppercase();
        out.push_str(&format!(
            "MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`{src}`) AND id(b) = id(`{dst}`) CREATE (a)-[:{rel}]->(b);\n",
            src = cypher_id(&e.src.as_str()),
            dst = cypher_id(&e.dst.as_str())
        ));
    }
    out
}

pub(crate) fn cypher_id(s: &str) -> String {
    // Backticked node-variable name: strip anything not [A-Za-z0-9_].
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn cypher_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

pub(crate) fn kind_dot_color(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::Folder => "#45475a",
        NodeKind::File => "#6c7086",
        NodeKind::Module => "#cba6f7",
        NodeKind::Struct => "#a6e3a1",
        NodeKind::Enum => "#94e2d5",
        NodeKind::Trait => "#fab387",
        NodeKind::TypeAlias => "#f38ba8",
        NodeKind::Function => "#89b4fa",
        NodeKind::Method => "#74c7ec",
        NodeKind::Constant => "#f9e2af",
        NodeKind::Macro => "#cdd6f4",
        NodeKind::Interface => "#89dceb",
        NodeKind::Property => "#cba6f7",
        NodeKind::Annotation => "#eba0ac",
        NodeKind::EnumMember => "#a6d189",
        NodeKind::Section => "#f5c2e7",
        NodeKind::Route => "#f2cdcd",
    }
}
