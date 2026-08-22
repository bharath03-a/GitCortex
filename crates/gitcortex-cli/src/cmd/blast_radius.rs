use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::{graph::Node, schema::EdgeKind, store::GraphStore};
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

    // Reverse call graph: callee → [callers] (Calls edges only)
    let mut reverse_calls: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == EdgeKind::Calls {
            reverse_calls
                .entry(edge.dst.clone())
                .or_default()
                .push(edge.src.clone());
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

    while let Some((node_id, hop)) = queue.pop_front() {
        if hop >= depth {
            continue;
        }
        if let Some(callers) = reverse_calls.get(&node_id) {
            for caller_id in callers {
                if !changed_ids.contains(caller_id) && !visited.contains(caller_id) {
                    visited.insert(caller_id.clone());
                    if let Some(&node) = node_map.get(caller_id) {
                        // This affected node's own direct-caller fan-in: a
                        // hub that's itself widely called makes the cascade
                        // through it riskier than a leaf caller, independent
                        // of how far it sits from the actual change.
                        let own_callers = reverse_calls.get(caller_id).map_or(0, Vec::len);
                        affected.push((node.clone(), hop + 1, risk_band(own_callers)));
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
    let risk = risk_band(affected.len());

    match format {
        BlastFormat::Text => print_text(&changed_nodes, &affected, &base, &head, risk),
        BlastFormat::GithubComment => {
            print_github_comment(&changed_nodes, &affected, &base, &head, risk)
        }
        BlastFormat::Json => print_json(&changed_nodes, &affected, &base, &head, risk)?,
    }

    Ok(())
}

/// Shared with `gitcortex_mcp::mcp::agent::find_callers`'s risk_level bucketing
/// (0-2 LOW / 3-10 MEDIUM / 11-30 HIGH / 31+ CRITICAL) so a risk label means
/// the same thing everywhere GitCortex reports one, CLI or MCP.
fn risk_band(caller_count: usize) -> &'static str {
    match caller_count {
        0..=2 => "LOW",
        3..=10 => "MEDIUM",
        11..=30 => "HIGH",
        _ => "CRITICAL",
    }
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
    fn risk_band_matches_pre_edit_impact_thresholds() {
        assert_eq!(risk_band(0), "LOW");
        assert_eq!(risk_band(2), "LOW");
        assert_eq!(risk_band(3), "MEDIUM");
        assert_eq!(risk_band(10), "MEDIUM");
        assert_eq!(risk_band(11), "HIGH");
        assert_eq!(risk_band(30), "HIGH");
        assert_eq!(risk_band(31), "CRITICAL");
        assert_eq!(risk_band(1000), "CRITICAL");
    }
}
