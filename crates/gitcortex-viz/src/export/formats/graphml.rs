use super::svg::svg_escape;
use crate::export::ScopedGraph;
pub fn to_graphml(g: &ScopedGraph) -> String {
    let branch = g.branch.as_str();
    let nodes = &g.nodes;
    let edges = &g.edges;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    out.push_str("  <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"file\" for=\"node\" attr.name=\"file\" attr.type=\"string\"/>\n");
    out.push_str(
        "  <key id=\"qname\" for=\"node\" attr.name=\"qualified_name\" attr.type=\"string\"/>\n",
    );
    out.push_str("  <key id=\"line\" for=\"node\" attr.name=\"start_line\" attr.type=\"int\"/>\n");
    out.push_str("  <key id=\"ekind\" for=\"edge\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str(&format!(
        "  <graph id=\"gitcortex-{}\" edgedefault=\"directed\">\n",
        svg_escape(branch)
    ));
    for n in nodes {
        out.push_str(&format!(
            "    <node id=\"{id}\">\n      <data key=\"name\">{name}</data>\n      <data key=\"kind\">{kind}</data>\n      <data key=\"file\">{file}</data>\n      <data key=\"qname\">{qname}</data>\n      <data key=\"line\">{line}</data>\n    </node>\n",
            id = svg_escape(&n.id.as_str()),
            name = svg_escape(&n.name),
            kind = n.kind,
            file = svg_escape(&n.file.display().to_string()),
            qname = svg_escape(&n.qualified_name),
            line = n.span.start_line
        ));
    }
    for (i, e) in edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{i}\" source=\"{src}\" target=\"{dst}\">\n      <data key=\"ekind\">{kind}</data>\n    </edge>\n",
            src = svg_escape(&e.src.as_str()),
            dst = svg_escape(&e.dst.as_str()),
            kind = e.kind
        ));
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}
