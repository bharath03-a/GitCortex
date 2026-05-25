//! Guided-tour generation — deterministic graph traversal that picks the
//! "important" symbols of a repo (or of a seeded subgraph) and orders them so
//! a reader can walk through the codebase top-down.
//!
//! Algorithm (pure graph, no LLM):
//! 1. Build adjacency from `Contains` + `Calls` edges.
//! 2. Score each node by in-degree across `Calls` (centrality).
//! 3. Pick top-K nodes globally, or BFS from a seed when one is provided.
//! 4. Return ordered tour steps with rationale per step.

use std::collections::{HashMap, HashSet, VecDeque};

use gitcortex_core::{
    error::Result,
    graph::Node,
    schema::{EdgeKind, NodeKind},
    store::GraphStore,
};
use serde::Serialize;

/// One step in a generated tour.
#[derive(Debug, Clone, Serialize)]
pub struct TourStep {
    pub order: u32,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
    /// Why this step appears here — e.g. "high in-degree (12 callers)" or
    /// "entry point: public function" or "called by previous step".
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tour {
    pub seed: Option<String>,
    pub branch: String,
    pub steps: Vec<TourStep>,
}

/// Default tour length when caller doesn't specify.
const DEFAULT_TOUR_LEN: usize = 12;
/// Hard cap to keep tour outputs bounded.
const MAX_TOUR_LEN: usize = 50;

/// Generate a tour for `branch`. If `seed` is `Some`, the tour is rooted at
/// that symbol and walks outward via `Calls` and `Contains` edges. If `None`,
/// the tour picks the highest-centrality public entry points across the repo.
pub fn generate<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    seed: Option<&str>,
    limit: Option<usize>,
) -> Result<Tour> {
    let limit = limit.unwrap_or(DEFAULT_TOUR_LEN).min(MAX_TOUR_LEN);
    let nodes = store.list_all_nodes(branch)?;
    let edges = store.list_all_edges(branch)?;

    let mut in_degree: HashMap<String, u32> = HashMap::new();
    let mut callees_of: HashMap<String, Vec<String>> = HashMap::new();
    for e in &edges {
        if matches!(e.kind, EdgeKind::Calls) {
            *in_degree.entry(e.dst.as_str()).or_insert(0) += 1;
            callees_of
                .entry(e.src.as_str())
                .or_default()
                .push(e.dst.as_str());
        }
    }

    let by_id: HashMap<String, Node> = nodes.into_iter().map(|n| (n.id.as_str(), n)).collect();

    let steps = match seed {
        Some(name) => seeded_tour(&by_id, &callees_of, &in_degree, name, limit),
        None => global_tour(&by_id, &in_degree, limit),
    };

    Ok(Tour {
        seed: seed.map(str::to_owned),
        branch: branch.to_owned(),
        steps,
    })
}

/// Pick highest-centrality public functions/methods across the repo.
fn global_tour(
    by_id: &HashMap<String, Node>,
    in_degree: &HashMap<String, u32>,
    limit: usize,
) -> Vec<TourStep> {
    let mut scored: Vec<(&Node, u32)> = by_id
        .values()
        .filter(|n| {
            matches!(
                n.kind,
                NodeKind::Function | NodeKind::Method | NodeKind::Struct | NodeKind::Trait
            )
        })
        .map(|n| {
            let deg = in_degree.get(&n.id.as_str()).copied().unwrap_or(0);
            (n, deg)
        })
        .collect();
    // Sort: higher degree first, then alphabetic by qualified_name for determinism.
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.qualified_name.cmp(&b.0.qualified_name))
    });

    scored
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, (n, deg))| TourStep {
            order: (i + 1) as u32,
            name: n.name.clone(),
            qualified_name: n.qualified_name.clone(),
            kind: n.kind.to_string(),
            file: n.file.display().to_string(),
            start_line: n.span.start_line,
            reason: if deg == 0 {
                "public surface (no inbound calls)".into()
            } else {
                format!("central — {deg} inbound calls")
            },
        })
        .collect()
}

/// BFS from `seed_name` along `Calls`, preserving discovery order.
fn seeded_tour(
    by_id: &HashMap<String, Node>,
    callees_of: &HashMap<String, Vec<String>>,
    in_degree: &HashMap<String, u32>,
    seed_name: &str,
    limit: usize,
) -> Vec<TourStep> {
    // Find a seed node by unqualified name; pick the highest-centrality one
    // when multiple match (matches user intent — "tour main" picks the
    // central main).
    let seed_node = by_id
        .values()
        .filter(|n| n.name == seed_name)
        .max_by_key(|n| in_degree.get(&n.id.as_str()).copied().unwrap_or(0));
    let Some(seed) = seed_node else {
        return Vec::new();
    };

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    queue.push_back((seed.id.as_str(), 0));
    visited.insert(seed.id.as_str());

    let mut steps: Vec<TourStep> = Vec::new();
    while let Some((id, hop)) = queue.pop_front() {
        if steps.len() >= limit {
            break;
        }
        let Some(n) = by_id.get(&id) else { continue };
        let reason = if hop == 0 {
            "seed".into()
        } else if hop == 1 {
            "directly called by seed".into()
        } else {
            format!("{hop} hops from seed")
        };
        steps.push(TourStep {
            order: (steps.len() + 1) as u32,
            name: n.name.clone(),
            qualified_name: n.qualified_name.clone(),
            kind: n.kind.to_string(),
            file: n.file.display().to_string(),
            start_line: n.span.start_line,
            reason,
        });
        if let Some(next) = callees_of.get(&id) {
            for callee_id in next {
                if visited.insert(callee_id.clone()) {
                    queue.push_back((callee_id.clone(), hop + 1));
                }
            }
        }
    }

    steps
}

/// Render a tour as a human-readable markdown plan.
pub fn render_markdown(tour: &Tour) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);
    let _ = writeln!(
        out,
        "# Tour ({} steps, branch={})",
        tour.steps.len(),
        tour.branch
    );
    if let Some(seed) = &tour.seed {
        let _ = writeln!(out, "Seed: `{seed}`");
    }
    let _ = writeln!(out);
    for s in &tour.steps {
        let _ = writeln!(
            out,
            "{}. `{}` ({})  — `{}:{}`  _{}_",
            s.order, s.name, s.kind, s.file, s.start_line, s.reason
        );
    }
    out
}
