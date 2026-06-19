use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::{transport::io::stdio, ServiceExt};

use crate::embeddings::{node_text, Embedder, SemanticIndex};
use crate::mcp::tools::{GitCortexServer, SemanticState};
use gitcortex_core::store::GraphStore;
use gitcortex_store::branch;

pub async fn serve(repo_root: PathBuf, compact: bool) -> Result<()> {
    let handler = GitCortexServer::new_with_mode(&repo_root, compact)
        .context("failed to open graph store")?;

    // Spawn background semantic indexer. Initialises the embedding model
    // (~23 MB download on first run, cached after), loads persisted vectors,
    // embeds missing nodes, then flips SemanticState to Ready.
    // MCP calls proceed text-only until it finishes.
    let (sem_arc, store_arc, default_branch) = handler.semantic_context();
    let repo_id = branch::repo_id(&repo_root);

    tokio::task::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            run_background_indexer(sem_arc, store_arc, &default_branch, &repo_id)
        })
        .await;

        match result {
            Ok(Ok(())) => tracing::info!("semantic indexer finished"),
            Ok(Err(e)) => tracing::warn!("semantic indexer failed: {e}"),
            Err(e) => tracing::warn!("semantic indexer panicked: {e}"),
        }
    });

    let transport = stdio();
    tracing::info!("GitCortex MCP server started (stdio, compact={compact})");

    let service = handler.serve(transport).await.context("MCP server error")?;
    service.waiting().await.context("MCP server stopped")?;

    Ok(())
}

fn run_background_indexer(
    sem_arc: std::sync::Arc<std::sync::Mutex<SemanticState>>,
    store_arc: std::sync::Arc<std::sync::Mutex<gitcortex_store::kuzu::KuzuGraphStore>>,
    branch: &str,
    repo_id: &str,
) -> anyhow::Result<()> {
    // 1. Initialise the embedding model (downloads on first run).
    let embedder = match Embedder::new() {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("semantic search disabled: {e}");
            if let Ok(mut s) = sem_arc.lock() {
                *s = SemanticState::Disabled;
            }
            return Ok(());
        }
    };

    // 2. Load or create per-branch vector index.
    let index_path =
        branch::data_dir(repo_id).join(format!("embeddings_{}.bin", branch::sanitize(branch)));
    let mut index = SemanticIndex::load_or_create(&index_path);

    // 3. Embed nodes that don't yet have a vector.
    let nodes = {
        let store = store_arc
            .lock()
            .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
        store.list_all_nodes(branch).unwrap_or_default()
    };

    // Prune vectors for nodes that no longer exist (UUIDs change on re-index).
    let live_ids: std::collections::HashSet<String> = nodes.iter().map(|n| n.id.as_str()).collect();
    let pruned = index.retain_ids(&live_ids);
    if pruned > 0 {
        tracing::info!("semantic index: pruned {pruned} stale vectors");
    }

    let missing: Vec<_> = nodes
        .iter()
        .filter(|n| !index.has(&n.id.as_str()))
        .collect();

    if !missing.is_empty() {
        tracing::info!(
            "semantic indexer: embedding {} new nodes on branch '{branch}'",
            missing.len()
        );
        const BATCH: usize = 32;
        for chunk in missing.chunks(BATCH) {
            let texts: Vec<String> = chunk.iter().map(|n| node_text(n)).collect();
            let ids: Vec<String> = chunk.iter().map(|n| n.id.as_str()).collect();
            match embedder.embed_batch(texts) {
                Ok(vecs) => {
                    for (id, vec) in ids.into_iter().zip(vecs) {
                        index.insert(id, vec);
                    }
                }
                Err(e) => tracing::warn!("embedding batch failed: {e}"),
            }
        }
        index.save();
        tracing::info!("semantic index: {} vectors", index.len());
    } else if pruned > 0 {
        index.save();
    } else {
        tracing::info!("semantic index up-to-date: {} vectors", index.len());
    }

    // 4. Flip to Ready.
    if let Ok(mut s) = sem_arc.lock() {
        *s = SemanticState::Ready {
            embedder: Box::new(embedder),
            index: Box::new(index),
        };
    }

    Ok(())
}
