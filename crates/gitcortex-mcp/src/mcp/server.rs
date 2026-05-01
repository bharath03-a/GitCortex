use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::{transport::io::stdio, ServiceExt};

use crate::mcp::tools::GitCortexServer;

pub async fn serve(repo_root: PathBuf) -> Result<()> {
    let handler = GitCortexServer::new(&repo_root).context("failed to open graph store")?;

    let transport = stdio();
    tracing::info!("GitCortex MCP server started (stdio)");

    handler.serve(transport).await.context("MCP server error")?;

    Ok(())
}
