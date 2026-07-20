//! Shared compact responses for agent-facing MCP and CLI queries.
//!
//! This module is the contract boundary between graph retrieval and agent
//! presentation. Both interfaces must call these functions so ranking,
//! ambiguity handling, and response budgets cannot drift.

use std::collections::{HashMap, HashSet};

use gitcortex_core::{
    error::Result,
    graph::Node,
    schema::{EdgeConfidence, EdgeKind, NodeKind, Visibility},
    store::GraphStore,
};
use serde::Serialize;

use super::helpers::{confidence_rank, is_test_file, sig_line};

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;
const DEFAULT_BUDGET_TOKENS: usize = 2_000;
const MIN_BUDGET_TOKENS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Ok,
    Ambiguous,
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolCandidate {
    pub id: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub start_line: u32,
    pub visibility: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConfidenceMix {
    pub extracted: usize,
    pub resolved: usize,
    pub inferred: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub confidence_mix: ConfidenceMix,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallerEvidence {
    pub hop: u8,
    pub symbol: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub signature: String,
    pub confidence: String,
    pub is_test: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCallersResponse {
    pub status: AgentStatus,
    pub answer: String,
    pub query: String,
    pub branch: String,
    pub depth: u8,
    pub risk_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolCandidate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<SymbolCandidate>,
    pub evidence: Vec<CallerEvidence>,
    pub coverage: Coverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationEvidence {
    pub relation: String,
    pub direction: String,
    pub symbol: String,
    pub qualified_name: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub confidence: String,
    pub is_test: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NeighborhoodCoverage {
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub direct_relations: usize,
    pub returned: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSubgraphResponse {
    pub status: AgentStatus,
    pub answer: String,
    pub query: String,
    pub branch: String,
    pub depth: u8,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolCandidate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<SymbolCandidate>,
    pub relation_counts: std::collections::BTreeMap<String, usize>,
    pub evidence: Vec<RelationEvidence>,
    pub coverage: NeighborhoodCoverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentQueryOptions {
    pub limit: usize,
    pub budget_tokens: usize,
}

impl Default for AgentQueryOptions {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            budget_tokens: DEFAULT_BUDGET_TOKENS,
        }
    }
}

enum Resolution {
    Exact(Box<Node>),
    Ambiguous(Vec<Node>),
    NotFound(Vec<Node>),
}

/// Find callers for exactly one symbol and return a globally-budgeted response.
/// Ambiguous short names return candidates without traversing the graph.
pub fn find_callers<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    query: &str,
    depth: u8,
    options: AgentQueryOptions,
) -> Result<AgentCallersResponse> {
    let depth = depth.clamp(1, 5);
    let options = AgentQueryOptions {
        limit: options.limit.clamp(1, MAX_LIMIT),
        budget_tokens: options.budget_tokens.max(MIN_BUDGET_TOKENS),
    };

    let target = match resolve_symbol(store, branch, query)? {
        Resolution::Exact(node) => *node,
        Resolution::Ambiguous(nodes) => {
            let total_candidates = nodes.len();
            let candidates = candidate_head(nodes, 5);
            return Ok(AgentCallersResponse {
                status: AgentStatus::Ambiguous,
                answer: format!(
                    "'{}' matches {total_candidates} code symbols; choose a qualified symbol before computing impact.",
                    query
                ),
                query: query.to_owned(),
                branch: branch.to_owned(),
                depth,
                risk_level: "UNKNOWN".to_owned(),
                symbol: None,
                candidates,
                evidence: Vec::new(),
                coverage: Coverage {
                    total: 0,
                    returned: 0,
                    truncated: false,
                    confidence_mix: ConfidenceMix::default(),
                },
                next_action: Some(
                    "Repeat find_callers with one candidate's qualified_name.".to_owned(),
                ),
            });
        }
        Resolution::NotFound(nodes) => {
            let candidates = candidate_head(nodes, 5);
            return Ok(AgentCallersResponse {
                status: AgentStatus::NotFound,
                answer: format!("No exact code symbol matching '{query}' was found."),
                query: query.to_owned(),
                branch: branch.to_owned(),
                depth,
                risk_level: "UNKNOWN".to_owned(),
                symbol: None,
                candidates,
                evidence: Vec::new(),
                coverage: Coverage {
                    total: 0,
                    returned: 0,
                    truncated: false,
                    confidence_mix: ConfidenceMix::default(),
                },
                next_action: Some("Use search_code to find the exact qualified symbol.".to_owned()),
            });
        }
    };

    let target_summary = to_candidate(&target);
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(target.id.as_str());
    let mut frontier = vec![target.id.as_str()];
    let mut evidence = Vec::new();
    let mut mix = ConfidenceMix::default();

    for hop in 1..=depth {
        let mut pairs = Vec::new();
        for target_id in &frontier {
            pairs.extend(store.find_callers_by_id_with_confidence(branch, target_id)?);
        }
        pairs.retain(|(node, _)| seen.insert(node.id.as_str()));
        pairs.sort_by(rank_callers);

        frontier = pairs.iter().map(|(node, _)| node.id.as_str()).collect();
        for (node, confidence) in pairs {
            match confidence {
                EdgeConfidence::Extracted => mix.extracted += 1,
                EdgeConfidence::Resolved => mix.resolved += 1,
                EdgeConfidence::Inferred => mix.inferred += 1,
            }
            evidence.push(to_evidence(node, confidence, hop));
        }
        if frontier.is_empty() {
            break;
        }
    }

    let total = evidence.len();
    let risk_level = match total {
        0..=2 => "LOW",
        3..=10 => "MEDIUM",
        11..=30 => "HIGH",
        _ => "CRITICAL",
    };
    evidence.truncate(options.limit);

    let answer = if total == 0 {
        format!(
            "No callers found for '{}' ({}).",
            target.name, target.qualified_name
        )
    } else {
        format!(
            "{total} caller(s) within {depth} hop(s) of '{}' — change risk {risk_level}.",
            target.qualified_name
        )
    };

    let mut response = AgentCallersResponse {
        status: AgentStatus::Ok,
        answer,
        query: query.to_owned(),
        branch: branch.to_owned(),
        depth,
        risk_level: risk_level.to_owned(),
        symbol: Some(target_summary),
        candidates: Vec::new(),
        evidence,
        coverage: Coverage {
            total,
            returned: 0,
            truncated: false,
            confidence_mix: mix,
        },
        next_action: None,
    };
    apply_budget(&mut response, options.budget_tokens);
    Ok(response)
}

/// Return a compact, exact-ID neighborhood digest. Only direct relationships
/// are serialized as evidence; deeper traversal contributes coverage counts.
pub fn get_subgraph<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    query: &str,
    depth: u8,
    direction: &str,
    options: AgentQueryOptions,
) -> Result<AgentSubgraphResponse> {
    let depth = depth.clamp(1, 5);
    let direction = match direction {
        "in" | "out" | "both" => direction,
        _ => "both",
    };
    let options = AgentQueryOptions {
        limit: options.limit.clamp(1, MAX_LIMIT),
        budget_tokens: options.budget_tokens.max(MIN_BUDGET_TOKENS),
    };

    let target = match resolve_symbol(store, branch, query)? {
        Resolution::Exact(node) => *node,
        Resolution::Ambiguous(nodes) => {
            let total = nodes.len();
            return Ok(AgentSubgraphResponse {
                status: AgentStatus::Ambiguous,
                answer: format!(
                    "'{query}' matches {total} code symbols; choose a qualified symbol before traversing its neighborhood."
                ),
                query: query.to_owned(),
                branch: branch.to_owned(),
                depth,
                direction: direction.to_owned(),
                symbol: None,
                candidates: candidate_head(nodes, 5),
                relation_counts: Default::default(),
                evidence: Vec::new(),
                coverage: NeighborhoodCoverage {
                    graph_nodes: 0,
                    graph_edges: 0,
                    direct_relations: 0,
                    returned: 0,
                    truncated: false,
                },
                next_action: Some(
                    "Repeat get_subgraph with one candidate's qualified_name.".to_owned(),
                ),
            });
        }
        Resolution::NotFound(nodes) => {
            return Ok(AgentSubgraphResponse {
                status: AgentStatus::NotFound,
                answer: format!("No exact code symbol matching '{query}' was found."),
                query: query.to_owned(),
                branch: branch.to_owned(),
                depth,
                direction: direction.to_owned(),
                symbol: None,
                candidates: candidate_head(nodes, 5),
                relation_counts: Default::default(),
                evidence: Vec::new(),
                coverage: NeighborhoodCoverage {
                    graph_nodes: 0,
                    graph_edges: 0,
                    direct_relations: 0,
                    returned: 0,
                    truncated: false,
                },
                next_action: Some("Use search_code to find the exact qualified symbol.".to_owned()),
            });
        }
    };

    let graph = store.get_subgraph_by_id(branch, &target.id.as_str(), depth, direction)?;
    let by_id: HashMap<String, &Node> = graph
        .nodes
        .iter()
        .filter(|node| is_code_node(node))
        .map(|node| (node.id.as_str(), node))
        .collect();
    let target_id = target.id.as_str();
    let mut evidence = Vec::new();
    let mut seen = HashSet::new();
    let mut counts = std::collections::BTreeMap::new();

    for edge in &graph.edges {
        let src = edge.src.as_str();
        let dst = edge.dst.as_str();
        let (other_id, edge_direction) = if src == target_id {
            (dst, "out")
        } else if dst == target_id {
            (src, "in")
        } else {
            continue;
        };
        if direction != "both" && direction != edge_direction {
            continue;
        }
        let Some(other) = by_id.get(&other_id) else {
            continue;
        };
        let relation = relation_label(&edge.kind, edge_direction);
        if !seen.insert((relation, other_id)) {
            continue;
        }
        *counts.entry(relation.to_owned()).or_insert(0) += 1;
        evidence.push(RelationEvidence {
            relation: relation.to_owned(),
            direction: edge_direction.to_owned(),
            symbol: other.name.clone(),
            qualified_name: other.qualified_name.clone(),
            kind: other.kind.to_string(),
            file: other.file.display().to_string(),
            line: edge.line.unwrap_or(other.span.start_line),
            confidence: edge.confidence.to_string(),
            is_test: is_test_file(&other.file),
        });
    }
    evidence.sort_by(|a, b| {
        relation_rank(&a.relation)
            .cmp(&relation_rank(&b.relation))
            .then_with(|| {
                confidence_label_rank(&a.confidence).cmp(&confidence_label_rank(&b.confidence))
            })
            .then_with(|| a.is_test.cmp(&b.is_test))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
    let direct_relations = evidence.len();
    evidence.truncate(options.limit);
    let count_summary = counts
        .iter()
        .map(|(relation, count)| format!("{relation}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let answer = if count_summary.is_empty() {
        format!(
            "'{}' has no direct relationships in the selected direction.",
            target.qualified_name
        )
    } else {
        format!(
            "Direct relationships for '{}': {count_summary}.",
            target.qualified_name
        )
    };
    let mut response = AgentSubgraphResponse {
        status: AgentStatus::Ok,
        answer,
        query: query.to_owned(),
        branch: branch.to_owned(),
        depth,
        direction: direction.to_owned(),
        symbol: Some(to_candidate(&target)),
        candidates: Vec::new(),
        relation_counts: counts,
        evidence,
        coverage: NeighborhoodCoverage {
            graph_nodes: by_id.len(),
            graph_edges: graph.edges.len(),
            direct_relations,
            returned: 0,
            truncated: false,
        },
        next_action: None,
    };
    apply_subgraph_budget(&mut response, options.budget_tokens);
    Ok(response)
}

fn relation_label(kind: &EdgeKind, direction: &str) -> &'static str {
    match (kind, direction) {
        (EdgeKind::Calls, "out") => "calls",
        (EdgeKind::Calls, _) => "called_by",
        (EdgeKind::Uses, "out") => "uses",
        (EdgeKind::Uses, _) => "used_by",
        (EdgeKind::Implements, "out") => "implements",
        (EdgeKind::Implements, _) => "implemented_by",
        (EdgeKind::Imports, "out") => "imports",
        (EdgeKind::Imports, _) => "imported_by",
        (EdgeKind::Contains, "out") => "contains",
        (EdgeKind::Contains, _) => "contained_by",
        (EdgeKind::Inherits, "out") => "inherits",
        (EdgeKind::Inherits, _) => "inherited_by",
        (EdgeKind::References, "out") => "references",
        (EdgeKind::References, _) => "referenced_by",
        _ => "related",
    }
}

fn relation_rank(relation: &str) -> u8 {
    match relation {
        "called_by" | "calls" => 0,
        "used_by" | "uses" => 1,
        "implemented_by" | "implements" | "inherited_by" | "inherits" => 2,
        "imported_by" | "imports" => 3,
        "contained_by" | "contains" => 4,
        _ => 5,
    }
}

fn confidence_label_rank(confidence: &str) -> u8 {
    match confidence {
        "extracted" => 0,
        "resolved" => 1,
        _ => 2,
    }
}

fn resolve_symbol<S: GraphStore + ?Sized>(
    store: &S,
    branch: &str,
    query: &str,
) -> Result<Resolution> {
    let query = query.trim();
    let mut exact = store.lookup_symbol(branch, query, false)?;
    exact.retain(is_code_node);

    // A qualified query may not match `lookup_symbol`, which is intentionally
    // short-name based. Search a bounded candidate set and compare exactly.
    let mut searched = store.search_nodes(branch, query, 50)?;
    searched.retain(is_code_node);
    if query.contains("::") || query.contains('.') {
        let qualified: Vec<Node> = searched
            .iter()
            .filter(|node| node.qualified_name.eq_ignore_ascii_case(query))
            .cloned()
            .collect();
        if qualified.len() == 1 {
            return Ok(Resolution::Exact(Box::new(qualified[0].clone())));
        }
        if qualified.len() > 1 {
            return Ok(Resolution::Ambiguous(qualified));
        }
    }

    dedup_nodes(&mut exact);
    match exact.len() {
        1 => Ok(Resolution::Exact(Box::new(exact.remove(0)))),
        n if n > 1 => Ok(Resolution::Ambiguous(exact)),
        _ => {
            searched.sort_by(rank_candidates);
            dedup_nodes(&mut searched);
            Ok(Resolution::NotFound(searched))
        }
    }
}

fn is_code_node(node: &Node) -> bool {
    !matches!(
        node.kind,
        NodeKind::Section | NodeKind::File | NodeKind::Folder | NodeKind::Module
    )
}

fn dedup_nodes(nodes: &mut Vec<Node>) {
    let mut seen = HashSet::new();
    nodes.retain(|node| seen.insert(node.id.as_str()));
}

fn candidate_head(mut nodes: Vec<Node>, limit: usize) -> Vec<SymbolCandidate> {
    nodes.sort_by(rank_candidates);
    nodes
        .into_iter()
        .take(limit)
        .map(|n| to_candidate(&n))
        .collect()
}

fn rank_candidates(a: &Node, b: &Node) -> std::cmp::Ordering {
    candidate_rank(a)
        .cmp(&candidate_rank(b))
        .then_with(|| a.file.cmp(&b.file))
        .then_with(|| a.qualified_name.cmp(&b.qualified_name))
}

fn candidate_rank(node: &Node) -> (u8, u8) {
    let test = is_test_file(&node.file) as u8;
    let visibility = match node.metadata.visibility {
        Visibility::Pub => 0,
        Visibility::PubCrate => 1,
        Visibility::Private => 2,
    };
    (test, visibility)
}

fn rank_callers(
    (a, ac): &(Node, EdgeConfidence),
    (b, bc): &(Node, EdgeConfidence),
) -> std::cmp::Ordering {
    confidence_rank(ac)
        .cmp(&confidence_rank(bc))
        .then_with(|| candidate_rank(a).cmp(&candidate_rank(b)))
        .then_with(|| a.file.cmp(&b.file))
        .then_with(|| a.qualified_name.cmp(&b.qualified_name))
}

fn to_candidate(node: &Node) -> SymbolCandidate {
    SymbolCandidate {
        id: node.id.as_str(),
        name: node.name.clone(),
        qualified_name: node.qualified_name.clone(),
        kind: node.kind.to_string(),
        file: node.file.display().to_string(),
        start_line: node.span.start_line,
        visibility: node.metadata.visibility.to_string(),
    }
}

fn to_evidence(node: Node, confidence: EdgeConfidence, hop: u8) -> CallerEvidence {
    CallerEvidence {
        hop,
        symbol: node.name.clone(),
        qualified_name: node.qualified_name.clone(),
        kind: node.kind.to_string(),
        file: node.file.display().to_string(),
        line: node.span.start_line,
        signature: sig_line(&node),
        confidence: confidence.to_string(),
        is_test: is_test_file(&node.file),
    }
}

fn apply_subgraph_budget(response: &mut AgentSubgraphResponse, budget_tokens: usize) {
    let budget_bytes = budget_tokens * 4;
    while !response.evidence.is_empty()
        && serde_json::to_vec(response)
            .map(|bytes| bytes.len() > budget_bytes)
            .unwrap_or(false)
    {
        response.evidence.pop();
    }
    response.coverage.returned = response.evidence.len();
    response.coverage.truncated = response.coverage.returned < response.coverage.direct_relations;
}

fn apply_budget(response: &mut AgentCallersResponse, budget_tokens: usize) {
    let budget_bytes = budget_tokens * 4;
    while !response.evidence.is_empty()
        && serde_json::to_vec(response)
            .map(|bytes| bytes.len() > budget_bytes)
            .unwrap_or(false)
    {
        response.evidence.pop();
    }
    response.coverage.returned = response.evidence.len();
    response.coverage.truncated = response.coverage.returned < response.coverage.total;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_detection_covers_supported_languages() {
        for path in [
            "tests/api.rs",
            "src/api_test.go",
            "src/api.test.ts",
            "src/__tests__/api.tsx",
            "src/ApiTest.java",
        ] {
            assert!(
                is_test_file(std::path::Path::new(path)),
                "expected test path: {path}"
            );
        }
        assert!(!is_test_file(std::path::Path::new("src/api.rs")));
    }

    #[test]
    fn confidence_order_is_strongest_first() {
        assert!(
            confidence_rank(&EdgeConfidence::Extracted)
                < confidence_rank(&EdgeConfidence::Resolved)
        );
        assert!(
            confidence_rank(&EdgeConfidence::Resolved) < confidence_rank(&EdgeConfidence::Inferred)
        );
    }
}
