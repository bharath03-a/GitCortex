use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::{schema::NodeKind, store::GraphStore};
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::QueryCmd;

pub fn run(cmd: QueryCmd) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    match cmd {
        QueryCmd::LookupSymbol { name, branch } => {
            let nodes = store.lookup_symbol(&branch, &name, false)?;
            if nodes.is_empty() {
                println!("no results for '{name}' on branch '{branch}'");
            }
            for n in nodes {
                println!(
                    "{} ({})  {}:{}",
                    n.name,
                    n.kind,
                    n.file.display(),
                    n.span.start_line
                );
            }
        }

        QueryCmd::FindCallers { name, depth, branch } => {
            if depth <= 1 {
                let nodes = store.find_callers(&branch, &name)?;
                if nodes.is_empty() {
                    println!("no callers of '{name}' on branch '{branch}'");
                }
                for n in nodes {
                    println!(
                        "{} ({})  {}:{}",
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            } else {
                let result = store.find_callers_deep(&branch, &name, depth)?;
                if result.hops.iter().all(|h| h.is_empty()) {
                    println!("no callers of '{name}' on branch '{branch}'");
                } else {
                    println!("callers of '{name}'  (risk: {}):", result.risk_level);
                    for (i, hop_nodes) in result.hops.iter().enumerate() {
                        if !hop_nodes.is_empty() {
                            println!("  hop {}:", i + 1);
                            for n in hop_nodes {
                                println!(
                                    "    {} ({})  {}:{}",
                                    n.name,
                                    n.kind,
                                    n.file.display(),
                                    n.span.start_line
                                );
                            }
                        }
                    }
                }
            }
        }

        QueryCmd::ListDefinitions { file, branch } => {
            let nodes = store.list_definitions(&branch, &PathBuf::from(&file))?;
            if nodes.is_empty() {
                println!("no definitions in '{file}' on branch '{branch}'");
            }
            for n in nodes {
                println!("{:>5}  {} ({})", n.span.start_line, n.name, n.kind);
            }
        }

        QueryCmd::SymbolContext { name, branch } => {
            let ctx = store.symbol_context(&branch, &name)?;
            println!("[GitCortex] {} ({})", name, branch);
            println!(
                "  definition: {} ({})  {}:{}",
                ctx.definition.name,
                ctx.definition.kind,
                ctx.definition.file.display(),
                ctx.definition.span.start_line,
            );
            if !ctx.callers.is_empty() {
                println!("  callers ({}):", ctx.callers.len());
                for n in &ctx.callers {
                    println!(
                        "    {} ({})  {}:{}",
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            }
            if !ctx.callees.is_empty() {
                println!("  callees ({}):", ctx.callees.len());
                for n in &ctx.callees {
                    println!(
                        "    {} ({})  {}:{}",
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            }
            if !ctx.used_by.is_empty() {
                println!("  used_by ({}):", ctx.used_by.len());
                for n in &ctx.used_by {
                    println!(
                        "    {} ({})  {}:{}",
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            }
        }

        QueryCmd::FindCallees { name, depth, branch } => {
            let result = store.find_callees(&branch, &name, depth)?;
            if result.hops.iter().all(|h| h.is_empty()) {
                println!("no callees of '{name}' on branch '{branch}'");
            } else {
                println!("callees of '{name}':");
                for (i, hop_nodes) in result.hops.iter().enumerate() {
                    if !hop_nodes.is_empty() {
                        println!("  hop {}:", i + 1);
                        for n in hop_nodes {
                            println!(
                                "    {} ({})  {}:{}",
                                n.name,
                                n.kind,
                                n.file.display(),
                                n.span.start_line
                            );
                        }
                    }
                }
            }
        }

        QueryCmd::FindImplementors { name, branch } => {
            let nodes = store.find_implementors(&branch, &name)?;
            if nodes.is_empty() {
                println!("no implementors of '{name}' on branch '{branch}'");
            }
            for n in nodes {
                println!(
                    "{} ({})  {}:{}",
                    n.name,
                    n.kind,
                    n.file.display(),
                    n.span.start_line
                );
            }
        }

        QueryCmd::TracePath { from, to, branch } => {
            let path = store.trace_path(&branch, &from, &to)?;
            if path.is_empty() {
                println!("no path from '{from}' to '{to}' on branch '{branch}' (max 6 hops)");
            } else {
                for (i, n) in path.iter().enumerate() {
                    let prefix = if i == 0 { "  " } else { "  →  " };
                    println!(
                        "{}{} ({})  {}:{}",
                        prefix,
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            }
        }

        QueryCmd::FindUnused { kind, branch } => {
            let kind_filter = kind.as_deref().and_then(parse_node_kind);
            let nodes = store.find_unused_symbols(&branch, kind_filter)?;
            if nodes.is_empty() {
                let qualifier = kind.as_deref().unwrap_or("symbol");
                println!("no unused {qualifier}s on branch '{branch}'");
            }
            for n in nodes {
                println!(
                    "{} ({})  {}:{}  [{}]",
                    n.name,
                    n.kind,
                    n.file.display(),
                    n.span.start_line,
                    n.metadata.visibility,
                );
            }
        }

        QueryCmd::GetSubgraph { name, depth, direction, branch } => {
            let sg = store.get_subgraph(&branch, &name, depth, &direction)?;
            if sg.nodes.is_empty() {
                println!("no subgraph for '{name}' on branch '{branch}'");
            } else {
                println!(
                    "{} nodes, {} edges  (seed={name}, depth={depth}, direction={direction})",
                    sg.nodes.len(),
                    sg.edges.len()
                );
                for n in &sg.nodes {
                    println!(
                        "  {} ({})  {}:{}",
                        n.name,
                        n.kind,
                        n.file.display(),
                        n.span.start_line
                    );
                }
            }
        }
    }
    Ok(())
}

fn parse_node_kind(s: &str) -> Option<NodeKind> {
    match s {
        "function"  => Some(NodeKind::Function),
        "method"    => Some(NodeKind::Method),
        "struct"    => Some(NodeKind::Struct),
        "trait"     => Some(NodeKind::Trait),
        "interface" => Some(NodeKind::Interface),
        "enum"      => Some(NodeKind::Enum),
        "constant"  => Some(NodeKind::Constant),
        _           => None,
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
