use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use tokio::io::AsyncReadExt;

use crate::embeddings::{node_text, Embedder, SemanticIndex};
use crate::mcp::tools::{GitCortexServer, SemanticState};
use gitcortex_core::store::GraphStore;
use gitcortex_store::branch;

const COMPACT_MODE: u8 = 0;
const FULL_MODE: u8 = 1;
const STARTUP_IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_millis(500);

struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Socket used by the machine-local repository daemon. The stable repository
/// ID keeps the path short enough for Unix-domain socket limits.
pub fn daemon_socket_path(repo_root: &Path) -> PathBuf {
    let repo_id = branch::storage_repo_id(repo_root);
    branch::data_dir(&repo_id).join("mcp.sock")
}

pub fn daemon_log_path(repo_root: &Path) -> PathBuf {
    let repo_id = branch::storage_repo_id(repo_root);
    branch::data_dir(&repo_id).join("daemon.log")
}

/// Own the embedded graph once and multiplex any number of local MCP clients.
/// Client proxies send one mode byte before the newline-delimited MCP stream.
pub async fn serve_daemon(repo_root: PathBuf) -> Result<()> {
    // Open Kuzu before advertising the socket. A successful client connect then
    // means the graph owner is fully ready, not merely that startup has begun.
    let handler = GitCortexServer::new_daemon(&repo_root).context("failed to open graph store")?;

    let socket_path = daemon_socket_path(&repo_root);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)
            .with_context(|| format!("remove stale socket {}", socket_path.display()))?;
    }
    let listener = tokio::net::UnixListener::bind(&socket_path)
        .with_context(|| format!("bind repository daemon socket {}", socket_path.display()))?;
    let _socket_guard = SocketGuard {
        path: socket_path.clone(),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    // The base handler owns Kuzu and all branch/semantic state. Per-client
    // clones differ only in whether they expose compact or full tool schemas.
    spawn_background_services(&repo_root, &handler);

    let clients = Arc::new(AtomicUsize::new(0));
    let changed = Arc::new(tokio::sync::Notify::new());
    let mut ever_connected = false;
    tracing::info!(
        "GitCortex repository daemon listening at {}",
        socket_path.display()
    );

    loop {
        let idle_timeout = if ever_connected {
            CLIENT_IDLE_TIMEOUT
        } else {
            STARTUP_IDLE_TIMEOUT
        };
        if clients.load(Ordering::Acquire) == 0 {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted.context("accept MCP client")?;
                    ever_connected = true;
                    spawn_client(handler.clone(), stream, clients.clone(), changed.clone());
                }
                _ = changed.notified() => {}
                _ = tokio::time::sleep(idle_timeout) => {
                    if clients.load(Ordering::Acquire) == 0 {
                        break;
                    }
                }
            }
        } else {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted.context("accept MCP client")?;
                    ever_connected = true;
                    spawn_client(handler.clone(), stream, clients.clone(), changed.clone());
                }
                _ = changed.notified() => {}
            }
        }
    }

    tracing::info!("GitCortex repository daemon stopped after its last client disconnected");
    Ok(())
}

fn spawn_client(
    handler: GitCortexServer,
    mut stream: tokio::net::UnixStream,
    clients: Arc<AtomicUsize>,
    changed: Arc<tokio::sync::Notify>,
) {
    clients.fetch_add(1, Ordering::AcqRel);
    tokio::spawn(async move {
        let result = async {
            let mode = stream.read_u8().await.context("read MCP client mode")?;
            let compact = match mode {
                COMPACT_MODE => true,
                FULL_MODE => false,
                other => anyhow::bail!("unsupported MCP client mode byte {other}"),
            };
            let service = handler
                .clone_with_mode(compact)
                .serve(stream)
                .await
                .context("start MCP client")?;
            service.waiting().await.context("MCP client stopped")?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!("MCP client connection ended with an error: {error:#}");
        }
        clients.fetch_sub(1, Ordering::AcqRel);
        changed.notify_one();
    });
}

fn spawn_background_services(repo_root: &Path, handler: &GitCortexServer) {
    // The watcher and semantic indexer run once per repository daemon, not once
    // per editor connection.
    let (watch_store, watch_branch, graph_revision) = handler.store_context();
    crate::mcp::watcher::spawn_file_watcher(
        repo_root.to_owned(),
        watch_store,
        watch_branch.clone(),
        graph_revision.clone(),
    );

    let (sem_arc, store_arc, default_branch) = handler.semantic_context();
    if std::env::var_os("GCX_DISABLE_SEMANTIC").is_some() {
        if let Ok(mut state) = sem_arc.lock() {
            *state = SemanticState::Disabled;
        }
        tracing::info!("semantic search disabled by GCX_DISABLE_SEMANTIC");
        return;
    }
    let repo_id = branch::storage_repo_id(repo_root);
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
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
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
