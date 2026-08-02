use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use rmcp::{transport::io::stdio, ServiceExt};

use crate::embeddings::{node_text, Embedder, SemanticIndex};
use crate::mcp::tools::{GitCortexServer, SemanticState};
use gitcortex_core::store::GraphStore;
use gitcortex_store::branch;

struct ServeLock {
    _file: File,
}

impl ServeLock {
    fn acquire(repo_root: &std::path::Path) -> Result<Self> {
        let repo_id = branch::storage_repo_id(repo_root);
        let path = branch::data_dir(&repo_id).join("serve.lock");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        if fs2::FileExt::try_lock_exclusive(&file).is_err() {
            let mut owner = String::new();
            let _ = file.read_to_string(&mut owner);
            let owner = owner.trim();
            anyhow::bail!(
                "another `gcx serve` process already owns this repository{}; configure one MCP server per repository or stop the existing process",
                if owner.is_empty() {
                    String::new()
                } else {
                    format!(" ({owner})")
                }
            );
        }
        file.set_len(0)?;
        file.rewind()?;
        write!(file, "pid {}", std::process::id())?;
        file.sync_data()?;
        Ok(Self { _file: file })
    }
}

pub async fn serve(repo_root: PathBuf, compact: bool) -> Result<()> {
    let _serve_lock = ServeLock::acquire(&repo_root).context("claim MCP server ownership")?;
    let handler = GitCortexServer::new_with_mode(&repo_root, compact)
        .context("failed to open graph store")?;

    // Spawn the Git-aware watcher first; it updates both the graph and the
    // shared active-branch state without requiring a second Kuzu process.
    let (watch_store, watch_branch, graph_revision) = handler.store_context();
    crate::mcp::watcher::spawn_file_watcher(
        repo_root.clone(),
        watch_store,
        watch_branch.clone(),
        graph_revision.clone(),
    );

    // Build semantic indexes serially and rebuild when the active branch
    // changes. This prevents vectors from the previous branch leaking into
    // default-branch search results.
    let (sem_arc, store_arc, default_branch) = handler.semantic_context();
    let repo_id = branch::storage_repo_id(&repo_root);
    tokio::task::spawn(async move {
        let mut indexed_branch = String::new();
        let mut indexed_revision = u64::MAX;
        loop {
            let active_branch = watch_branch
                .lock()
                .map(|branch| branch.clone())
                .unwrap_or_else(|_| default_branch.clone());
            let revision = graph_revision.load(std::sync::atomic::Ordering::Acquire);
            if active_branch != indexed_branch || revision != indexed_revision {
                let branch_changed = active_branch != indexed_branch;
                if branch_changed {
                    if let Ok(mut state) = sem_arc.lock() {
                        *state = SemanticState::Pending;
                    }
                }
                let task_sem = sem_arc.clone();
                let task_store = store_arc.clone();
                let task_branch = active_branch.clone();
                let task_repo_id = repo_id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if branch_changed {
                        run_background_indexer(task_sem, task_store, &task_branch, &task_repo_id)
                    } else {
                        refresh_background_indexer(task_sem, task_store, &task_branch)
                    }
                })
                .await;
                match result {
                    Ok(Ok(())) => {
                        tracing::info!("semantic indexer finished for branch '{active_branch}'")
                    }
                    Ok(Err(error)) => tracing::warn!("semantic indexer failed: {error}"),
                    Err(error) => tracing::warn!("semantic indexer panicked: {error}"),
                }
                indexed_branch = active_branch;
                indexed_revision = revision;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
    let embedder = match Embedder::new(&branch::models_dir()) {
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

    update_semantic_index(&embedder, &mut index, &nodes, branch);

    // 4. Flip to Ready.
    if let Ok(mut s) = sem_arc.lock() {
        *s = SemanticState::Ready {
            branch: branch.to_owned(),
            embedder: Box::new(embedder),
            index: Box::new(index),
        };
    }

    Ok(())
}

fn refresh_background_indexer(
    sem_arc: std::sync::Arc<std::sync::Mutex<SemanticState>>,
    store_arc: std::sync::Arc<std::sync::Mutex<gitcortex_store::kuzu::KuzuGraphStore>>,
    branch: &str,
) -> anyhow::Result<()> {
    let nodes = {
        let store = store_arc
            .lock()
            .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
        store.list_all_nodes(branch).unwrap_or_default()
    };
    let mut state = sem_arc
        .lock()
        .map_err(|_| anyhow::anyhow!("semantic mutex poisoned"))?;
    if let SemanticState::Ready {
        branch: indexed_branch,
        embedder,
        index,
    } = &mut *state
    {
        if indexed_branch == branch {
            update_semantic_index(embedder, index, &nodes, branch);
        }
    }
    Ok(())
}

fn update_semantic_index(
    embedder: &Embedder,
    index: &mut SemanticIndex,
    nodes: &[gitcortex_core::graph::Node],
    branch: &str,
) {
    let live_ids: std::collections::HashSet<String> =
        nodes.iter().map(|node| node.id.as_str()).collect();
    let pruned = index.retain_ids(&live_ids);
    if pruned > 0 {
        tracing::info!("semantic index: pruned {pruned} stale vectors");
    }

    let missing: Vec<_> = nodes
        .iter()
        .filter(|node| !index.has(&node.id.as_str()))
        .collect();
    if !missing.is_empty() {
        tracing::info!(
            "semantic indexer: embedding {} new nodes on branch '{branch}'",
            missing.len()
        );
        const BATCH: usize = 32;
        for chunk in missing.chunks(BATCH) {
            let texts: Vec<String> = chunk.iter().map(|node| node_text(node)).collect();
            let ids: Vec<String> = chunk.iter().map(|node| node.id.as_str()).collect();
            match embedder.embed_batch(texts) {
                Ok(vectors) => {
                    for (id, vector) in ids.into_iter().zip(vectors) {
                        index.insert(id, vector);
                    }
                }
                Err(error) => tracing::warn!("embedding batch failed: {error}"),
            }
        }
        index.save();
        tracing::info!("semantic index: {} vectors", index.len());
    } else if pruned > 0 {
        index.save();
    } else {
        tracing::info!("semantic index up-to-date: {} vectors", index.len());
    }
}
