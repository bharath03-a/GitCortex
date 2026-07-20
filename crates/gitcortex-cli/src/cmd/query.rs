use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::{schema::NodeKind, store::GraphStore};
use gitcortex_mcp::mcp::{centrality, clustering, search, tour, wiki};
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::style::{
    arrow, header_style, hint_style, kind_style, kind_style_from_str, name_style, node_line, paint,
    path_style, risk_style, score_style,
};
use crate::{AgentOutputFormat, QueryCmd};

pub fn run(cmd: QueryCmd) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    match cmd {
        QueryCmd::LookupSymbol { name, branch } => {
            let nodes = store.lookup_symbol(&branch, &name, false)?;
            if nodes.is_empty() {
                println!(
                    "{}",
                    empty_msg(&format!("no results for '{name}'"), &branch)
                );
            }
            for n in nodes {
                println!("{}", node_line(&n));
            }
        }

        QueryCmd::FindCallers {
            name,
            depth,
            limit,
            budget_tokens,
            format,
            branch,
        } => {
            let response = gitcortex_mcp::mcp::agent::find_callers(
                &store,
                &branch,
                &name,
                depth,
                gitcortex_mcp::mcp::agent::AgentQueryOptions {
                    limit,
                    budget_tokens,
                },
            )?;
            match format {
                AgentOutputFormat::AgentJson => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                AgentOutputFormat::Text => print_agent_callers(&response),
            }
        }

        QueryCmd::ListDefinitions { file, branch } => {
            let nodes = store.list_definitions(&branch, &PathBuf::from(&file))?;
            if nodes.is_empty() {
                println!(
                    "{}",
                    empty_msg(&format!("no definitions in '{file}'"), &branch)
                );
            }
            for n in nodes {
                println!(
                    "{}  {} {}",
                    paint(hint_style(), &format!("{:>5}", n.span.start_line)),
                    paint(name_style(), &n.name),
                    paint(kind_style(&n.kind), &format!("({})", n.kind)),
                );
            }
        }

        QueryCmd::SymbolContext { name, branch } => {
            let ctx = store.symbol_context(&branch, &name)?;
            println!(
                "{} {} {}",
                paint(hint_style(), "[GitCortex]"),
                paint(name_style(), &name),
                paint(hint_style(), &format!("({branch})")),
            );
            println!(
                "  {} {}",
                paint(header_style(), "definition:"),
                node_line(&ctx.definition),
            );
            print_section("callers", &ctx.callers);
            print_section("callees", &ctx.callees);
            print_section("used_by", &ctx.used_by);
        }

        QueryCmd::FindCallees {
            name,
            depth,
            branch,
        } => {
            let result = store.find_callees(&branch, &name, depth)?;
            if result.hops.iter().all(|h| h.is_empty()) {
                println!("{}", empty_msg(&format!("no callees of '{name}'"), &branch));
            } else {
                println!(
                    "{} {}",
                    paint(header_style(), "callees of"),
                    paint(name_style(), &format!("'{name}'")),
                );
                for (i, hop_nodes) in result.hops.iter().enumerate() {
                    if !hop_nodes.is_empty() {
                        println!("  {} {}:", paint(hint_style(), "hop"), i + 1);
                        for n in hop_nodes {
                            println!("    {}", node_line(n));
                        }
                    }
                }
            }
        }

        QueryCmd::FindImplementors { name, branch } => {
            let nodes = store.find_implementors(&branch, &name)?;
            if nodes.is_empty() {
                println!(
                    "{}",
                    empty_msg(&format!("no implementors of '{name}'"), &branch)
                );
            }
            for n in nodes {
                println!("{}", node_line(&n));
            }
        }

        QueryCmd::TracePath { from, to, branch } => {
            let path = store.trace_path(&branch, &from, &to)?;
            if path.is_empty() {
                println!(
                    "{} {} {} {} {} {}",
                    paint(hint_style(), "no path from"),
                    paint(name_style(), &format!("'{from}'")),
                    paint(hint_style(), "to"),
                    paint(name_style(), &format!("'{to}'")),
                    paint(hint_style(), &format!("on branch '{branch}'")),
                    paint(hint_style(), "(max 6 hops)"),
                );
            } else {
                for (i, n) in path.iter().enumerate() {
                    if i == 0 {
                        println!("  {}", node_line(n));
                    } else {
                        println!("  {}  {}", arrow(), node_line(n));
                    }
                }
            }
        }

        QueryCmd::FindUnused { kind, branch } => {
            let kind_filter = kind.as_deref().and_then(parse_node_kind);
            let nodes = store.find_unused_symbols(&branch, kind_filter)?;
            if nodes.is_empty() {
                let qualifier = kind.as_deref().unwrap_or("symbol");
                println!("{}", empty_msg(&format!("no unused {qualifier}s"), &branch));
            }
            for n in nodes {
                println!(
                    "{}  {}",
                    node_line(&n),
                    paint(hint_style(), &format!("[{}]", n.metadata.visibility)),
                );
            }
        }

        QueryCmd::GetSubgraph {
            name,
            depth,
            direction,
            limit,
            branch,
        } => {
            let sg = store.get_subgraph(&branch, &name, depth, &direction)?;
            if sg.nodes.is_empty() {
                println!(
                    "{}",
                    empty_msg(&format!("no subgraph for '{name}'"), &branch)
                );
            } else {
                println!(
                    "{} {} {}",
                    paint(
                        header_style(),
                        &format!("{} nodes, {} edges", sg.nodes.len(), sg.edges.len()),
                    ),
                    paint(hint_style(), "—"),
                    paint(
                        hint_style(),
                        &format!("seed={name}, depth={depth}, direction={direction}"),
                    ),
                );
                for n in sg.nodes.iter().take(limit) {
                    println!("  {}", node_line(n));
                }
                if sg.nodes.len() > limit {
                    println!(
                        "  {}",
                        paint(
                            hint_style(),
                            &format!("showing {limit} of {} nodes", sg.nodes.len())
                        )
                    );
                }
            }
        }

        QueryCmd::Wiki { name, branch } => {
            let md = wiki::render_symbol(&store, &branch, &name)?;
            print!("{md}");
        }

        QueryCmd::Search {
            query,
            limit,
            branch,
        } => {
            let hits = search::search(&store, &branch, &query, Some(limit))?;
            if hits.is_empty() {
                println!(
                    "{}",
                    empty_msg(&format!("no matches for '{query}'"), &branch)
                );
            }
            for h in hits {
                println!(
                    "{}  {} {}  {}{}{}  {}",
                    paint(score_style(), &format!("{:>4}", h.score)),
                    paint(name_style(), &h.name),
                    paint(kind_style_from_str(&h.kind), &format!("({})", h.kind)),
                    paint(path_style(), &h.file),
                    paint(path_style(), ":"),
                    paint(path_style(), &h.start_line.to_string()),
                    paint(hint_style(), &format!("[{}]", h.qualified_name)),
                );
            }
        }

        QueryCmd::Tour {
            seed,
            limit,
            branch,
        } => {
            let t = tour::generate(&store, &branch, seed.as_deref(), Some(limit))?;
            print!("{}", tour::render_markdown(&t));
        }
        QueryCmd::FindGodNodes {
            min_in_degree,
            limit,
            branch,
        } => {
            let nodes =
                centrality::find_god_nodes(&store, &branch, Some(min_in_degree), Some(limit))?;
            if nodes.is_empty() {
                println!(
                    "{}",
                    paint(
                        hint_style(),
                        &format!(
                            "no symbols with ≥{min_in_degree} inbound calls on branch '{branch}'"
                        )
                    )
                );
            } else {
                println!(
                    "  {} {}",
                    paint(header_style(), "Hub symbols:"),
                    paint(hint_style(), &format!("({})", nodes.len()))
                );
                for n in &nodes {
                    println!(
                        "    {} {} {}  {}",
                        paint(kind_style_from_str(&n.kind), &n.kind),
                        paint(name_style(), &n.name),
                        paint(hint_style(), &format!("({} callers)", n.in_degree)),
                        paint(path_style(), &format!("{}:{}", n.file, n.start_line)),
                    );
                }
            }
        }
        QueryCmd::FindClusters {
            min_cluster_size,
            limit,
            branch,
        } => {
            let clusters =
                clustering::find_clusters(&store, &branch, Some(min_cluster_size), Some(limit))?;
            if clusters.is_empty() {
                println!(
                    "{}",
                    paint(
                        hint_style(),
                        &format!(
                            "no clusters of ≥{min_cluster_size} members found on branch '{branch}'"
                        )
                    )
                );
            } else {
                println!(
                    "  {} {}",
                    paint(header_style(), "Clusters:"),
                    paint(hint_style(), &format!("({})", clusters.len()))
                );
                for c in &clusters {
                    println!(
                        "\n  {} {} {}",
                        paint(header_style(), &c.label),
                        paint(hint_style(), &format!("({} members)", c.size)),
                        if c.size > c.members.len() {
                            paint(hint_style(), &format!("[showing {}]", c.members.len()))
                        } else {
                            String::new()
                        }
                    );
                    for m in &c.members {
                        println!(
                            "    {} {}  {}",
                            paint(kind_style_from_str(&m.kind), &m.kind),
                            paint(name_style(), &m.name),
                            paint(path_style(), &format!("{}:{}", m.file, m.start_line)),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_agent_callers(response: &gitcortex_mcp::mcp::agent::AgentCallersResponse) {
    use gitcortex_mcp::mcp::agent::AgentStatus;

    println!(
        "{} {}",
        paint(header_style(), "[GitCortex]"),
        paint(
            if response.status == AgentStatus::Ok {
                risk_style(&response.risk_level)
            } else {
                hint_style()
            },
            &response.answer,
        )
    );
    if !response.candidates.is_empty() {
        println!("  {}", paint(header_style(), "qualified candidates:"));
        for candidate in &response.candidates {
            println!(
                "    {} {}  {}",
                paint(kind_style_from_str(&candidate.kind), &candidate.kind),
                paint(name_style(), &candidate.qualified_name),
                paint(
                    path_style(),
                    &format!("{}:{}", candidate.file, candidate.start_line)
                )
            );
        }
    }
    for item in &response.evidence {
        println!(
            "  {} {} {}  {}  {}",
            paint(hint_style(), &format!("hop {}", item.hop)),
            paint(name_style(), &item.qualified_name),
            paint(kind_style_from_str(&item.kind), &format!("({})", item.kind)),
            paint(path_style(), &format!("{}:{}", item.file, item.line)),
            paint(hint_style(), &format!("[{}]", item.confidence)),
        );
    }
    if response.coverage.truncated {
        println!(
            "  {}",
            paint(
                hint_style(),
                &format!(
                    "showing {} of {} ranked callers",
                    response.coverage.returned, response.coverage.total
                )
            )
        );
    }
    if let Some(next) = &response.next_action {
        println!("  {}", paint(hint_style(), next));
    }
}

fn empty_msg(prefix: &str, branch: &str) -> String {
    format!(
        "{} {}",
        paint(hint_style(), prefix),
        paint(hint_style(), &format!("on branch '{branch}'"))
    )
}

fn print_section(label: &str, nodes: &[gitcortex_core::graph::Node]) {
    if nodes.is_empty() {
        return;
    }
    println!(
        "  {} {}",
        paint(header_style(), &format!("{label}:")),
        paint(hint_style(), &format!("({})", nodes.len()))
    );
    for n in nodes {
        println!("    {}", node_line(n));
    }
}

fn parse_node_kind(s: &str) -> Option<NodeKind> {
    match s {
        "file" => Some(NodeKind::File),
        "folder" => Some(NodeKind::Folder),
        "module" => Some(NodeKind::Module),
        "struct" => Some(NodeKind::Struct),
        "enum" => Some(NodeKind::Enum),
        "enum_member" | "enum-member" => Some(NodeKind::EnumMember),
        "trait" => Some(NodeKind::Trait),
        "interface" => Some(NodeKind::Interface),
        "type_alias" | "type-alias" => Some(NodeKind::TypeAlias),
        "function" => Some(NodeKind::Function),
        "method" => Some(NodeKind::Method),
        "property" => Some(NodeKind::Property),
        "constant" => Some(NodeKind::Constant),
        "macro" => Some(NodeKind::Macro),
        "annotation" => Some(NodeKind::Annotation),
        "section" => Some(NodeKind::Section),
        _ => None,
    }
}

fn repo_root() -> Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}
