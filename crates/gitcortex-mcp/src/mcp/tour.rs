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
    schema::{EdgeConfidence, EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use serde::Serialize;

use super::{centrality::in_degree_by_calls, helpers::is_test_file};

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
    /// Community group label (no-seed tours only). `None` for seeded tours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
}

/// A component (directory/module) in the architecture summary.
#[derive(Debug, Clone, Serialize)]
pub struct Component {
    /// Directory path that groups the component's files, e.g. `src/parser`.
    pub path: String,
    /// Number of distinct source files in the component.
    pub files: u32,
    /// Highest-ranked public production symbols in the component (up to 2).
    pub key_symbols: Vec<String>,
    /// Other components this one calls into / uses / imports from.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tour {
    pub seed: Option<String>,
    pub branch: String,
    pub steps: Vec<TourStep>,
    /// Component-level architecture summary (no-seed tours only). Answers
    /// "what are the main components and how do they fit together" in one call.
    pub components: Vec<Component>,
}

/// Default tour length when caller doesn't specify.
const DEFAULT_TOUR_LEN: usize = 6;
/// Hard cap to keep tour outputs bounded.
const MAX_TOUR_LEN: usize = 20;
/// No-seed tours retain a small internal entry-point head alongside components.
const NO_SEED_STEP_CAP: usize = 8;

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

    let in_degree = in_degree_by_calls(&edges);
    let mut callees_of: HashMap<String, Vec<String>> = HashMap::new();
    for e in &edges {
        if matches!(e.kind, EdgeKind::Calls) {
            callees_of
                .entry(e.src.as_str())
                .or_default()
                .push(e.dst.as_str());
        }
    }

    // Cross-component dependency edges (Calls/Uses/Imports), kept for the
    // architecture summary so we can show how components fit together.
    let mut dep_edges: Vec<(String, String)> = Vec::new();
    for e in &edges {
        if matches!(e.kind, EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Imports)
            && !matches!(e.confidence, EdgeConfidence::Inferred)
        {
            dep_edges.push((e.src.as_str(), e.dst.as_str()));
        }
    }

    let by_id: HashMap<String, Node> = nodes.into_iter().map(|n| (n.id.as_str(), n)).collect();

    let (steps, components) = match seed {
        Some(name) => (
            seeded_tour(&by_id, &callees_of, &in_degree, name, limit),
            Vec::new(),
        ),
        None => (
            global_tour(&by_id, &in_degree, limit.min(NO_SEED_STEP_CAP)),
            architecture_summary(&by_id, &in_degree, &dep_edges, limit),
        ),
    };

    Ok(Tour {
        seed: seed.map(str::to_owned),
        branch: branch.to_owned(),
        steps,
        components,
    })
}

/// Derive a component label from a file path: the parent directory, or the
/// stem when the file is at the repo root.
fn component_of(file: &str) -> String {
    match file.rfind('/') {
        Some(i) => file[..i].to_owned(),
        None => "<root>".to_owned(),
    }
}

fn is_agent_relevant(node: &Node) -> bool {
    let path = node.file.to_string_lossy();
    let lower = path.to_ascii_lowercase().replace('\\', "/");
    let generated_or_docs = lower.starts_with("docs/")
        || lower.starts_with("site/")
        || lower.starts_with("examples/")
        || lower.contains("/generated/")
        || lower.contains("/vendor/")
        || lower.contains("/node_modules/")
        || lower.contains("/target/");
    !generated_or_docs
        && !is_test_file(&node.file)
        && !matches!(node.metadata.visibility, Visibility::Private)
        && matches!(
            node.kind,
            NodeKind::Function
                | NodeKind::Method
                | NodeKind::Struct
                | NodeKind::Trait
                | NodeKind::Interface
                | NodeKind::Enum
        )
}

fn tour_score(node: &Node, in_degree: u32) -> u32 {
    let kind_weight = match node.kind {
        NodeKind::Struct | NodeKind::Trait | NodeKind::Interface | NodeKind::Enum => 100,
        NodeKind::Function => 70,
        NodeKind::Method => 10,
        _ => 0,
    };
    // Very common helper names can accumulate noisy inferred edges. Cap the
    // centrality contribution so architecture-bearing types and entry
    // functions remain ahead of generic methods such as `get` or `as_str`.
    kind_weight + in_degree.min(30)
}

/// Group symbols into components (directories) and summarise each: file count,
/// top central symbols, and the other components it depends on. Components are
/// ranked by aggregate centrality so the most important appear first.
fn architecture_summary(
    by_id: &HashMap<String, Node>,
    in_degree: &HashMap<String, u32>,
    dep_edges: &[(String, String)],
    limit: usize,
) -> Vec<Component> {
    // id → component, for edge resolution.
    let comp_of_id: HashMap<&str, String> = by_id
        .iter()
        .filter(|(_, node)| is_agent_relevant(node))
        .map(|(id, node)| (id.as_str(), component_of(&node.file.display().to_string())))
        .collect();

    // Per-component aggregates.
    let mut files: HashMap<String, HashSet<String>> = HashMap::new();
    let mut score: HashMap<String, u32> = HashMap::new();
    // (symbol_name "name — file:line", degree) for picking key symbols. The
    // location is embedded so a tour answer needs no follow-up lookups.
    let mut symbols: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    for n in by_id.values().filter(|node| is_agent_relevant(node)) {
        let file = n.file.display().to_string();
        let comp = component_of(&file);
        files.entry(comp.clone()).or_default().insert(file.clone());
        let deg = in_degree.get(&n.id.as_str()).copied().unwrap_or(0);
        *score.entry(comp.clone()).or_insert(0) += tour_score(n, deg);
        if matches!(
            n.kind,
            NodeKind::Function
                | NodeKind::Method
                | NodeKind::Struct
                | NodeKind::Trait
                | NodeKind::Interface
                | NodeKind::Enum
        ) {
            let label = format!("{} — {}:{}", n.name, file, n.span.start_line);
            symbols
                .entry(comp)
                .or_default()
                .push((label, tour_score(n, deg)));
        }
    }

    // Cross-component dependencies.
    let mut deps: HashMap<String, HashSet<String>> = HashMap::new();
    for (src, dst) in dep_edges {
        if let (Some(sc), Some(dc)) = (comp_of_id.get(src.as_str()), comp_of_id.get(dst.as_str())) {
            if sc != dc {
                deps.entry(sc.clone()).or_default().insert(dc.clone());
            }
        }
    }

    let mut ranked: Vec<(String, u32)> = score.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    ranked
        .into_iter()
        .take(limit)
        .map(|(comp, _)| {
            let mut key = symbols.remove(&comp).unwrap_or_default();
            key.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            key.dedup_by(|a, b| a.0 == b.0);
            let key_symbols: Vec<String> = key.into_iter().take(2).map(|(name, _)| name).collect();
            let mut depends_on: Vec<String> = deps
                .get(&comp)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            depends_on.sort();
            depends_on.truncate(5);
            Component {
                files: files.get(&comp).map(|f| f.len() as u32).unwrap_or(0),
                path: comp,
                key_symbols,
                depends_on,
            }
        })
        .collect()
}

/// Pick architecture-bearing public production symbols across the repo.
fn global_tour(
    by_id: &HashMap<String, Node>,
    in_degree: &HashMap<String, u32>,
    limit: usize,
) -> Vec<TourStep> {
    let mut scored: Vec<(&Node, u32, u32)> = by_id
        .values()
        .filter(|node| is_agent_relevant(node) && !matches!(node.kind, NodeKind::Method))
        .map(|node| {
            let degree = in_degree.get(&node.id.as_str()).copied().unwrap_or(0);
            (node, tour_score(node, degree), degree)
        })
        .collect();
    // Sort: architecture-weighted score first, then qualified name.
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.qualified_name.cmp(&b.0.qualified_name))
    });

    scored
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, (n, _, deg))| TourStep {
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
            community: None,
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
            community: None,
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

    // No-seed tours ARE the component-level architecture map — a compact,
    // self-contained answer to "what are the main components and how do they
    // fit together". A short "most central" list follows; we deliberately do
    // not dump a long step list, keeping the result token-cheap.
    if tour.seed.is_none() && !tour.components.is_empty() {
        let _ = writeln!(out, "# Architecture (branch={})", tour.branch);

        let _ = writeln!(
            out,
            "\n## Components ({} shown, ranked by centrality)\n",
            tour.components.len()
        );
        for c in &tour.components {
            let _ = writeln!(out, "### `{}` ({} files)", c.path, c.files);
            if !c.key_symbols.is_empty() {
                let keys = c
                    .key_symbols
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "- key: {keys}");
            }
            if !c.depends_on.is_empty() {
                let _ = writeln!(out, "- depends on: {}", c.depends_on.join(", "));
            }
        }
        return out;
    }

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
