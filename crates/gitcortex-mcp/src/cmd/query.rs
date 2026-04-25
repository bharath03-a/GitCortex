use std::path::PathBuf;

use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_store::kuzu::KuzuGraphStore;

use crate::QueryCmd;

pub fn run(cmd: QueryCmd) -> Result<()> {
    let repo_root = repo_root()?;
    let store = KuzuGraphStore::open(&repo_root).context("failed to open graph store")?;

    match cmd {
        QueryCmd::LookupSymbol { name, branch } => {
            let nodes = store.lookup_symbol(&branch, &name)?;
            if nodes.is_empty() {
                println!("no results for '{name}' on branch '{branch}'");
            }
            for n in nodes {
                println!(
                    "{} ({:?})  {}:{}",
                    n.name,
                    n.kind,
                    n.file.display(),
                    n.span.start_line
                );
            }
        }
        QueryCmd::FindCallers { name, branch } => {
            let nodes = store.find_callers(&branch, &name)?;
            if nodes.is_empty() {
                println!("no callers of '{name}' on branch '{branch}'");
            }
            for n in nodes {
                println!(
                    "{} ({:?})  {}:{}",
                    n.name,
                    n.kind,
                    n.file.display(),
                    n.span.start_line
                );
            }
        }
        QueryCmd::ListDefinitions { file, branch } => {
            let nodes = store.list_definitions(&branch, &PathBuf::from(&file))?;
            if nodes.is_empty() {
                println!("no definitions in '{file}' on branch '{branch}'");
            }
            for n in nodes {
                println!("{:>5}  {} ({:?})", n.span.start_line, n.name, n.kind);
            }
        }
    }
    Ok(())
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
