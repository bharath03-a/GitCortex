use anyhow::{Context, Result};
use gitcortex_mcp::mcp::tree_jobs::{apply_tree_index, resolve_commit};
use gitcortex_store::kuzu::KuzuGraphStore;

#[derive(clap::Args)]
#[command(group(clap::ArgGroup::new("revision").required(true).args(["branch"])))]
pub struct IndexArgs {
    /// Index a local branch without checking it out.
    #[arg(long)]
    pub branch: Option<String>,
}
pub fn run(args: IndexArgs) -> Result<()> {
    let branch = args.branch.context("a local branch is required")?;
    let repo = git2::Repository::discover(".")?;
    // git2 returns a trailing slash; store identity is path-based and must
    // match the physical root used by the other CLI commands.
    let root = repo.workdir().unwrap_or(repo.path()).canonicalize()?;
    let sha = resolve_commit(&root, &format!("refs/heads/{}", branch))?;
    let mut store = KuzuGraphStore::open(&root)?;
    let outcome = apply_tree_index(&root, &mut store, &branch, &sha)?;
    println!(
        "Indexed {} at {}: {} nodes, {} edges",
        branch, outcome.sha, outcome.nodes, outcome.edges
    );
    Ok(())
}
