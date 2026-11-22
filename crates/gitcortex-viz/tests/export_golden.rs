use anyhow::Result;
use gitcortex_core::{
    graph::{Edge, Node, NodeId, Span},
    schema::{EdgeKind, NodeKind},
};
use gitcortex_viz::export::*;
fn fixture(count: usize) -> (Vec<Node>, Vec<Edge>) {
    let nodes: Vec<Node> = (0..count)
        .map(|i| Node {
            id: NodeId::stable(&format!("golden-{i}")),
            name: if i == 0 {
                "\"x<script>`|\"</script>λ".into()
            } else {
                format!("function_{i}")
            },
            qualified_name: format!("sample::function_{i}"),
            kind: NodeKind::Function,
            file: "tests/integration/fixtures/sample.rs".into(),
            span: Span {
                start_line: i as u32 + 1,
                end_line: i as u32 + 2,
            },
            metadata: Default::default(),
        })
        .collect();
    let edges = nodes
        .windows(2)
        .map(|n| Edge::new(n[0].id.clone(), n[1].id.clone(), EdgeKind::Calls))
        .collect();
    (nodes, edges)
}
#[test]
fn legacy_renderer_goldens() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    std::fs::create_dir_all(&root)?;
    for count in [4, 5002] {
        let (mut nodes, mut edges) = fixture(count);
        if count == 4 {
            use gitcortex_indexer::parser::{rust::RustParser, LanguageParser};
            let parsed = RustParser::new().parse(
                std::path::Path::new("sample.rs"),
                include_str!("../../../tests/integration/fixtures/sample.rs"),
            )?;
            // Canonicalize parser-assigned IDs before capturing reproducible bytes.
            let ids: std::collections::HashMap<_, _> = parsed
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.clone(), NodeId::stable(&format!("parsed-{i}"))))
                .collect();
            for mut n in parsed.nodes {
                n.id = ids[&n.id].clone();
                nodes.push(n);
            }
            for mut e in parsed.edges {
                e.src = ids[&e.src].clone();
                e.dst = ids[&e.dst].clone();
                edges.push(e);
            }
        }

        for (fmt, render) in [
            ("dot", to_dot as fn(&ScopedGraph) -> String),
            ("html", to_html),
            ("svg", to_svg),
            ("graphml", to_graphml),
            ("cypher", to_cypher),
        ] {
            let g = ScopedGraph::from_explicit("main".into(), nodes.clone(), edges.clone())?;
            let output = render(&g);
            let (name, actual) = if count > 5000 {
                (
                    format!("large.{fmt}.sha256"),
                    format!("{:x}", Sha256::digest(output.as_bytes())),
                )
            } else {
                (format!("sample.{fmt}"), output)
            };
            let path = root.join(name);
            if std::env::var_os("CAPTURE_GOLDENS").is_some() {
                std::fs::write(&path, &actual)?;
            }
            assert_eq!(std::fs::read_to_string(path)?, actual);
        }
    }
    Ok(())
}
