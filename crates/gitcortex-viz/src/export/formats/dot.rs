use super::cypher::kind_dot_color;
use crate::export::ScopedGraph;
pub fn to_dot(g: &ScopedGraph) -> String {
    let nodes = &g.nodes;
    let edges = &g.edges;

    let mut out = String::from(
        "digraph gitcortex {\n  rankdir=LR;\n  node [shape=box style=filled fontcolor=white fontname=monospace];\n  edge [fontname=monospace fontsize=9];\n"
    );

    for n in nodes {
        let id = n.id.as_str();
        let label = dot_escape(&format!("{}\\n{}", n.name, n.kind));
        let color = kind_dot_color(&n.kind);
        out.push_str(&format!(
            "  \"{id}\" [label=\"{label}\" fillcolor=\"{color}\"];\n"
        ));
    }

    for e in edges {
        let src = e.src.as_str();
        let dst = e.dst.as_str();
        let lbl = e.kind.to_string();
        out.push_str(&format!("  \"{src}\" -> \"{dst}\" [label=\"{lbl}\"];\n"));
    }

    out.push('}');
    out
}

pub(crate) fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
