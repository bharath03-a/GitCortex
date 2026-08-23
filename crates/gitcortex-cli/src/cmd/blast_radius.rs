use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::{
    graph::Node,
    schema::{EdgeConfidence, EdgeKind},
    store::GraphStore,
};
use gitcortex_store::kuzu::KuzuGraphStore;
use serde_json::{json, Value};

use gitcortex_core::graph::NodeId;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum BlastFormat {
    Text,
    #[value(name = "github-comment")]
    GithubComment,
    Json,
}

pub fn run(base: String, head: String, depth: u8, format: BlastFormat) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    let diff = store
        .branch_diff(&base, &head)
        .context("branch diff failed")?;
    let changed_nodes = diff.added_nodes;

    if changed_nodes.is_empty() {
        match format {
            BlastFormat::Text => println!("No changes between {base} and {head}."),
            BlastFormat::GithubComment => {
                println!("## Blast Radius\n\nNo changes detected between `{base}` and `{head}`.")
            }
            BlastFormat::Json => {
                let out = json!({
                    "base": base, "head": head,
                    "risk": "NONE", "changed_count": 0, "affected_count": 0,
                    "changed_nodes": [], "affected_callers": []
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        }
        return Ok(());
    }

    let all_edges = store.list_all_edges(&head).context("list edges failed")?;
    let all_nodes = store.list_all_nodes(&head).context("list nodes failed")?;

    // node_id → Node lookup
    let node_map: HashMap<NodeId, &Node> = all_nodes.iter().map(|n| (n.id.clone(), n)).collect();

    // Reverse call graph: callee → [(caller, edge confidence)] (Calls edges only)
    let mut reverse_calls: HashMap<NodeId, Vec<(NodeId, EdgeConfidence)>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == EdgeKind::Calls {
            reverse_calls
                .entry(edge.dst.clone())
                .or_default()
                .push((edge.src.clone(), edge.confidence));
        }
    }

    // BFS from each changed node up to `depth` hops through the reverse call graph.
    let changed_ids: HashSet<NodeId> = changed_nodes.iter().map(|n| n.id.clone()).collect();
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<(NodeId, u8)> = VecDeque::new();

    for id in &changed_ids {
        queue.push_back((id.clone(), 0));
    }

    let mut affected: Vec<(Node, u8, &'static str)> = Vec::new();
    // Confidence mix of the edges that pulled each affected node into the
    // blast radius, accumulated for the PR-level risk band below.
    let mut overall_mix = (0usize, 0usize, 0usize);

    while let Some((node_id, hop)) = queue.pop_front() {
        if hop >= depth {
            continue;
        }
        if let Some(callers) = reverse_calls.get(&node_id) {
            for (caller_id, confidence) in callers {
                if !changed_ids.contains(caller_id) && !visited.contains(caller_id) {
                    visited.insert(caller_id.clone());
                    match confidence {
                        EdgeConfidence::Extracted => overall_mix.0 += 1,
                        EdgeConfidence::Resolved => overall_mix.1 += 1,
                        EdgeConfidence::Inferred => overall_mix.2 += 1,
                    }
                    if let Some(&node) = node_map.get(caller_id) {
                        // This affected node's own direct-caller fan-in: a
                        // hub that's itself widely called makes the cascade
                        // through it riskier than a leaf caller, independent
                        // of how far it sits from the actual change.
                        let own_mix = confidence_mix(reverse_calls.get(caller_id));
                        let (_, item_risk) = weighted_risk_band(own_mix.0, own_mix.1, own_mix.2);
                        affected.push((node.clone(), hop + 1, item_risk));
                    }
                    queue.push_back((caller_id.clone(), hop + 1));
                }
            }
        }
    }

    // Same bucketing pre_edit_impact uses for a symbol's own caller count,
    // applied here to the whole PR's affected-caller count, so a "HIGH risk"
    // PR-level report and a "HIGH risk" single-symbol MCP answer mean the
    // same thing.
    let (_, risk) = weighted_risk_band(overall_mix.0, overall_mix.1, overall_mix.2);

    match format {
        BlastFormat::Text => print_text(&changed_nodes, &affected, &base, &head, risk),
        BlastFormat::GithubComment => {
            print_github_comment(&changed_nodes, &affected, &base, &head, risk)
        }
        BlastFormat::Json => print_json(&changed_nodes, &affected, &base, &head, risk)?,
    }

    Ok(())
}

/// Sums the `EdgeConfidence` mix of a caller list into (extracted, resolved,
/// inferred) counts, for feeding into `weighted_risk_band`.
fn confidence_mix(callers: Option<&Vec<(NodeId, EdgeConfidence)>>) -> (usize, usize, usize) {
    let mut mix = (0usize, 0usize, 0usize);
    if let Some(callers) = callers {
        for (_, confidence) in callers {
            match confidence {
                EdgeConfidence::Extracted => mix.0 += 1,
                EdgeConfidence::Resolved => mix.1 += 1,
                EdgeConfidence::Inferred => mix.2 += 1,
            }
        }
    }
    mix
}

/// Confidence-weighted risk band. Kept in sync with
/// `gitcortex_mcp::mcp::agent::weighted_risk_band` and `gitcortex_viz`'s
/// inline copy — all three must move together so a risk label means the
/// same thing everywhere GitCortex reports one.
///
/// Score = extracted*1.0 + resolved*0.6 + inferred*0.3. Thresholds:
/// LOW < 3.0, MEDIUM < 11.0, HIGH < 350.0, else CRITICAL. See
/// `gitcortex_mcp::mcp::agent::weighted_risk_band`'s doc comment for the
/// full reasoning behind these numbers (preserves today's LOW/MEDIUM
/// boundary for confidence-uniform extracted evidence; widens HIGH/CRITICAL
/// so a huge low-confidence-only caller set, like the spike's 1085-caller
/// Django `QuerySet.filter` example, no longer alone reads CRITICAL).
fn weighted_risk_band(extracted: usize, resolved: usize, inferred: usize) -> (f64, &'static str) {
    let score = extracted as f64 + resolved as f64 * 0.6 + inferred as f64 * 0.3;
    let band = if score < 3.0 {
        "LOW"
    } else if score < 11.0 {
        "MEDIUM"
    } else if score < 350.0 {
        "HIGH"
    } else {
        "CRITICAL"
    };
    (score, band)
}

fn print_text(changed: &[Node], affected: &[(Node, u8, &str)], base: &str, head: &str, risk: &str) {
    let sep = "─".repeat(52);
    println!("Blast Radius Report");
    println!("{sep}");
    println!("  {head} → {base}");
    println!(
        "  Changed: {}  |  Affected: {}  |  Risk: {risk}",
        changed.len(),
        affected.len()
    );
    println!("{sep}");

    if !changed.is_empty() {
        println!("Changed nodes:");
        for n in changed {
            println!(
                "  {:10}  {:<30}  {}:{}",
                n.kind,
                n.name,
                n.file.display(),
                n.span.start_line
            );
        }
    }

    if !affected.is_empty() {
        println!("\nAffected callers:");
        for (n, hop, item_risk) in affected {
            println!(
                "  [hop {}] [{item_risk:8}]  {:10}  {:<30}  {}:{}",
                hop,
                n.kind,
                n.name,
                n.file.display(),
                n.span.start_line
            );
        }
    }
}

fn print_github_comment(
    changed: &[Node],
    affected: &[(Node, u8, &str)],
    base: &str,
    head: &str,
    risk: &str,
) {
    let risk_emoji = match risk {
        "LOW" => "🟢",
        "MEDIUM" => "🟡",
        "HIGH" => "🟠",
        _ => "🔴",
    };

    println!("## {risk_emoji} Blast Radius — Risk: **{risk}**");
    println!();
    println!(
        "> `{head}` → `{base}` · {} changed · {} affected",
        changed.len(),
        affected.len()
    );
    println!();

    if !changed.is_empty() {
        println!("### Changed nodes");
        println!("| Kind | Name | File |");
        println!("|------|------|------|");
        for n in changed {
            println!(
                "| `{}` | `{}` | `{}:{}` |",
                n.kind,
                n.name,
                n.file.display(),
                n.span.start_line
            );
        }
        println!();
    }

    if !affected.is_empty() {
        println!("### Affected callers");
        println!("| Hop | Item risk | Kind | Name | File |");
        println!("|-----|-----------|------|------|------|");
        for (n, hop, item_risk) in affected {
            println!(
                "| {} | {} | `{}` | `{}` | `{}:{}` |",
                hop,
                item_risk,
                n.kind,
                n.name,
                n.file.display(),
                n.span.start_line
            );
        }
        println!();
    }

    println!("---");
    println!("_Generated by [GitCortex](https://github.com/bharath03-a/GitCortex)_");
}

fn print_json(
    changed: &[Node],
    affected: &[(Node, u8, &str)],
    base: &str,
    head: &str,
    risk: &str,
) -> Result<()> {
    let changed_json: Vec<Value> = changed.iter().map(node_to_json).collect();
    let affected_json: Vec<Value> = affected
        .iter()
        .map(|(n, hop, item_risk)| {
            let mut v = node_to_json(n);
            v["hop"] = json!(hop);
            v["risk"] = json!(item_risk);
            v
        })
        .collect();

    let out = json!({
        "base": base,
        "head": head,
        "risk": risk,
        "changed_count": changed.len(),
        "affected_count": affected.len(),
        "changed_nodes": changed_json,
        "affected_callers": affected_json,
    });

    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn node_to_json(n: &Node) -> Value {
    json!({
        "id": n.id.as_str(),
        "kind": n.kind.to_string(),
        "name": n.name,
        "qualified_name": n.qualified_name,
        "file": n.file.to_string_lossy(),
        "start_line": n.span.start_line,
    })
}

fn repo_root() -> Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    if !out.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_risk_band_preserves_uniform_extracted_bands() {
        assert_eq!(weighted_risk_band(0, 0, 0).1, "LOW");
        assert_eq!(weighted_risk_band(2, 0, 0).1, "LOW");
        assert_eq!(weighted_risk_band(3, 0, 0).1, "MEDIUM");
        assert_eq!(weighted_risk_band(10, 0, 0).1, "MEDIUM");
        assert_eq!(weighted_risk_band(11, 0, 0).1, "HIGH");
        assert_eq!(weighted_risk_band(31, 0, 0).1, "HIGH");
    }

    #[test]
    fn weighted_risk_band_uniform_inferred_needs_far_more_evidence() {
        assert_eq!(weighted_risk_band(0, 0, 2).1, "LOW");
        assert_eq!(weighted_risk_band(0, 0, 10).1, "MEDIUM");
        assert_eq!(weighted_risk_band(0, 0, 40).1, "HIGH");
        assert_eq!(weighted_risk_band(0, 0, 1200).1, "CRITICAL");
    }

    #[test]
    fn weighted_risk_band_mixed_django_worked_example_is_not_critical() {
        let (score, band) = weighted_risk_band(4, 0, 1081);
        assert!((score - 328.3).abs() < 0.01, "score was {score}");
        assert_eq!(band, "HIGH");
    }
}
