use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::{transport::io::stdio, ServiceExt};

use crate::mcp::tools::GitCortexServer;

pub async fn serve(repo_root: PathBuf) -> Result<()> {
    let handler = GitCortexServer::new(&repo_root).context("failed to open graph store")?;

    let transport = stdio();
    tracing::info!("GitCortex MCP server started (stdio)");

    // `serve` returns a `RunningService` that owns the message loop. Dropping
    // it immediately tears the connection down — the client only ever sees the
    // `initialize` response, then the process exits. Hold it and `waiting()`
    // until the transport closes (client disconnects / EOF on stdin).
    let service = handler.serve(transport).await.context("MCP server error")?;
    service.waiting().await.context("MCP server stopped")?;

    Ok(())
}
