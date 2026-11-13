use anyhow::{Context, Result};
use gitcortex_core::store::GraphStore;
use gitcortex_mcp::mcp::tree_jobs::{
    apply_tree_index, missing_branch_message, resolve_local_branch,
};
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
    let sha = resolve_local_branch(&root, &branch)?
        .with_context(|| missing_branch_message(&root, &branch))?;
    let mut store = KuzuGraphStore::open(&root).with_context(|| {
        format!(
            "could not open the graph to index branch {branch} (a running MCP session can index it: call any tool with branch \"{branch}\")"
        )
    })?;
    let outcome = apply_tree_index(&root, &mut store, &branch, &sha)?;
    let nodes = store.list_all_nodes(&branch)?.len();
    let edges = store.list_all_edges(&branch)?.len();
    if outcome.unchanged {
        println!(
            "{branch} already indexed at {}: {nodes} nodes, {edges} edges (unchanged)",
            outcome.sha
        );
    } else {
        println!(
            "Indexed {branch} at {}: {nodes} nodes, {edges} edges",
            outcome.sha
        );
    }
    Ok(())
}
