use super::cypher::kind_dot_color;
use super::svg::{escape_script_payload, svg_escape};
use crate::export::ScopedGraph;
use serde_json::{json, Value};
pub fn to_html(g: &ScopedGraph) -> String {
    let branch = g.branch.as_str();
    let nodes = &g.nodes;
    let edges = &g.edges;

    let nodes_json: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id":    n.id.as_str(),
                "label": format!("{}\n{}", n.name, n.kind),
                "title": format!(
                    "{} ({})\\n{}:{}\\nkind: {}",
                    n.name, n.qualified_name, n.file.display(), n.span.start_line, n.kind
                ),
                "color": kind_dot_color(&n.kind),
                "group": n.kind.to_string(),
                "shape": "box",
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges
        .iter()
        .map(|e| {
            json!({
                "from":  e.src.as_str(),
                "to":    e.dst.as_str(),
                "label": e.kind.to_string(),
                "arrows": "to",
            })
        })
        .collect();

    let payload = json!({ "nodes": nodes_json, "edges": edges_json });
    let payload_str = escape_script_payload(&payload.to_string());
    let branch_esc = svg_escape(branch);
    let total_nodes = nodes.len();
    let total_edges = edges.len();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>GitCortex graph — {branch_esc}</title>
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<style>
  html, body {{ margin: 0; padding: 0; height: 100%; background: #1e1e2e; color: #cdd6f4; font-family: -apple-system, sans-serif; }}
  #header {{ padding: 10px 16px; background: #181825; border-bottom: 1px solid #313244; font-size: 13px; }}
  #header strong {{ color: #89b4fa; }}
  #header input {{ background: #313244; color: #cdd6f4; border: 1px solid #45475a; padding: 4px 8px; border-radius: 4px; margin-left: 12px; }}
  #net {{ width: 100vw; height: calc(100vh - 48px); }}
</style>
</head>
<body>
<div id="header">
  <strong>GitCortex</strong> · branch <code>{branch_esc}</code> · {total_nodes} nodes, {total_edges} edges
  <input id="q" type="search" placeholder="search by name…">
</div>
<div id="net"></div>
<script>
  const DATA = {payload_str};
  const nodes = new vis.DataSet(DATA.nodes);
  const edges = new vis.DataSet(DATA.edges);
  const net = new vis.Network(document.getElementById('net'), {{ nodes, edges }}, {{
    nodes: {{ font: {{ color: '#1e1e2e', face: 'monospace', size: 12 }} }},
    edges: {{ color: {{ color: '#6c7086', highlight: '#f5c2e7' }}, font: {{ color: '#bac2de', size: 10, strokeWidth: 0, background: 'rgba(30,30,46,0.7)' }}, smooth: false }},
    physics: {{ stabilization: {{ iterations: 200 }}, barnesHut: {{ gravitationalConstant: -8000, springLength: 120 }} }},
    interaction: {{ hover: true, tooltipDelay: 200 }}
  }});
  document.getElementById('q').addEventListener('input', (e) => {{
    const term = e.target.value.toLowerCase();
    if (!term) {{ net.unselectAll(); return; }}
    const hits = DATA.nodes.filter(n => (n.label || '').toLowerCase().includes(term)).map(n => n.id);
    net.selectNodes(hits);
    if (hits.length) net.focus(hits[0], {{ animation: true, scale: 0.8 }});
  }});
</script>
</body>
</html>"#
    )
}
