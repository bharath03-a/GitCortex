use super::cypher::kind_dot_color;
use crate::export::ScopedGraph;
use gitcortex_core::graph::Node;
pub fn to_svg(g: &ScopedGraph) -> String {
    let branch = g.branch.as_str();
    use std::collections::HashMap;

    let nodes = &g.nodes;
    let edges = &g.edges;

    let mut by_kind: HashMap<String, Vec<&Node>> = HashMap::new();
    for n in nodes {
        by_kind.entry(n.kind.to_string()).or_default().push(n);
    }
    let mut kinds: Vec<String> = by_kind.keys().cloned().collect();
    kinds.sort();

    let cx = 600.0_f64;
    let cy = 600.0_f64;
    let ring_base = 80.0_f64;
    let ring_step = 95.0_f64;

    let mut pos: HashMap<String, (f64, f64)> = HashMap::new();
    let mut svg = String::new();
    let branch_esc = svg_escape(branch);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1200 1200\" font-family=\"monospace\" font-size=\"9\">\n  <rect width=\"1200\" height=\"1200\" fill=\"#1e1e2e\"/>\n  <text x=\"20\" y=\"24\" fill=\"#cdd6f4\">GitCortex · branch {branch_esc} · {n} nodes · {e} edges</text>\n",
        n = nodes.len(),
        e = edges.len()
    ));

    for (i, kind) in kinds.iter().enumerate() {
        let radius = ring_base + ring_step * (i as f64);
        let group = &by_kind[kind];
        let n = group.len() as f64;
        for (j, node) in group.iter().enumerate() {
            let theta = (j as f64) * std::f64::consts::TAU / n.max(1.0);
            let x = cx + radius * theta.cos();
            let y = cy + radius * theta.sin();
            pos.insert(node.id.as_str(), (x, y));
        }
    }

    // Draw edges first (behind nodes).
    for e in edges {
        let src = e.src.as_str();
        let dst = e.dst.as_str();
        let (Some(&(x1, y1)), Some(&(x2, y2))) = (pos.get(&src), pos.get(&dst)) else {
            continue;
        };
        svg.push_str(&format!(
            "  <line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"#45475a\" stroke-width=\"0.5\" opacity=\"0.5\"/>\n"
        ));
    }

    // Draw nodes + labels.
    for n in nodes {
        let nid = n.id.as_str();
        let Some(&(x, y)) = pos.get(&nid) else {
            continue;
        };
        let color = kind_dot_color(&n.kind);
        let name = svg_escape(&n.name);
        svg.push_str(&format!(
            "  <circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"3.5\" fill=\"{color}\" stroke=\"#11111b\" stroke-width=\"0.5\"/>\n  <text x=\"{tx:.1}\" y=\"{ty:.1}\" fill=\"#cdd6f4\">{name}</text>\n",
            tx = x + 5.0,
            ty = y + 3.0
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

pub(crate) fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape `</` inside a JSON string about to be embedded in a `<script>`
/// block. `serde_json::to_string` does not escape this sequence, so a node
/// name/file path containing the literal substring `</script>` (sourced
/// from a cloned, potentially untrusted repo) would close the tag early and
/// inject arbitrary HTML/JS into the exported file.
pub(crate) fn escape_script_payload(s: &str) -> String {
    s.replace("</", "<\\/")
}
