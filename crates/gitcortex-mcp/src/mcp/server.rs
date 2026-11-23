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
/// How long an idle daemon outlives its clients to finish a branch index. Past
/// this the job is abandoned: its dirty marker keeps the key from reading ready.
const TREE_JOB_DRAIN_CAP: Duration = Duration::from_secs(600);

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

/// Ask the live graph owner for observed readiness instead of equating an
/// advisory lock with a healthy watcher. The probe has a bounded timeout.
pub async fn daemon_readiness(repo_root: &Path) -> Result<serde_json::Value> {
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut stream = tokio::net::UnixStream::connect(daemon_socket_path(repo_root)).await?;
        stream.write_all(&[COMPACT_MODE]).await?;
        let client = ().serve(stream).await?;
        let response = client
            .call_tool(
                rmcp::model::CallToolRequestParams::new("gcx").with_arguments(
                    serde_json::json!({"action":"graph_stats", "params":{}})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )
            .await;
        client.cancel().await?;
        let response = response?;
        anyhow::ensure!(
            response.is_error != Some(true),
            "daemon readiness query failed"
        );
        let index = response
            .structured_content
            .and_then(|data| data.get("index").cloned())
            .context("daemon did not return index readiness; restart it with the current binary")?;
        Ok(index)
    })
    .await
    .context("daemon readiness probe timed out")?
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

    tracing::info!(
        "GitCortex repository daemon listening at {}",
        socket_path.display()
    );
    accept_until_idle(listener, handler, TREE_JOB_DRAIN_CAP).await?;
    tracing::info!("GitCortex repository daemon stopped after its last client disconnected");
    Ok(())
}

async fn accept_until_idle(
    listener: tokio::net::UnixListener,
    handler: GitCortexServer,
    drain_cap: Duration,
) -> Result<()> {
    let clients = Arc::new(AtomicUsize::new(0));
    let changed = Arc::new(tokio::sync::Notify::new());
    let mut ever_connected = false;
    let mut draining_since = None;

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
                    draining_since = None;
                    spawn_client(handler.clone(), stream, clients.clone(), changed.clone());
                }
                _ = changed.notified() => {}
                _ = tokio::time::sleep(idle_timeout) => {
                    if clients.load(Ordering::Acquire) == 0 {
                        if !handler.has_active_tree_job() {
                            break;
                        }
                        let since = *draining_since.get_or_insert_with(std::time::Instant::now);
                        if since.elapsed() >= drain_cap {
                            tracing::warn!("abandoning a branch index after the idle drain limit");
                            break;
                        }
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
    let watch = handler.store_context();
    let watch_branch = watch.branch;
    let graph_revision = watch.graph_revision;
    crate::mcp::watcher::spawn_file_watcher(
        repo_root.to_owned(),
        watch.store,
        watch_branch.clone(),
        graph_revision.clone(),
        watch.unhealthy,
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
        .filter(|node| index.needs_embedding(node))
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
            let fingerprints: Vec<_> = chunk
                .iter()
                .map(|node| crate::embeddings::node_fingerprint(node))
                .collect();
            match embedder.embed_batch(texts) {
                Ok(vectors) => {
                    for ((id, fingerprint), vector) in
                        ids.into_iter().zip(fingerprints).zip(vectors)
                    {
                        index.insert(id, vector, fingerprint);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::readiness::inspect_commit;
    use crate::mcp::test_support::{commit_file, git, run_async, TempRepo};
    use rmcp::model::CallToolRequestParams;
    use tokio::io::AsyncWriteExt;

    const DEADLINE: Duration = Duration::from_secs(15);

    struct Idling {
        handler: GitCortexServer,
        daemon: tokio::task::JoinHandle<Result<()>>,
        permit: tokio::sync::OwnedSemaphorePermit,
        feature: String,
        _socket: tempfile::TempDir,
    }

    // A client starts a branch job held behind the permit, then disconnects.
    async fn client_leaves_during_job(repo: Arc<TempRepo>, drain_cap: Duration) -> Idling {
        let head = commit_file(repo.path(), "a.rs", "pub fn base() {}\n", "base");
        git(repo.path(), &["switch", "-c", "feat/x"]);
        let feature = commit_file(repo.path(), "b.rs", "pub fn feature() {}\n", "feature");
        git(repo.path(), &["switch", "main"]);
        let handler = GitCortexServer::new_daemon(repo.path()).expect("server");
        crate::mcp::tree_jobs::apply_tree_index(
            repo.path(),
            &mut *handler.store_context().store.lock().expect("store"),
            "main",
            &head,
        )
        .expect("index main");
        let permit = handler.tree_jobs().hold_permit().await;
        let socket = tempfile::tempdir_in("/tmp").expect("socket dir");
        let path = socket.path().join("mcp.sock");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind");
        let daemon = tokio::spawn(accept_until_idle(listener, handler.clone(), drain_cap));
        let mut stream = tokio::net::UnixStream::connect(&path)
            .await
            .expect("connect");
        stream.write_all(&[COMPACT_MODE]).await.expect("mode");
        let client = ().serve(stream).await.expect("client");
        let data = client
            .call_tool(
                CallToolRequestParams::new("gcx").with_arguments(
                    serde_json::json!({"action": "graph_stats", "params": {"branch": "feat/x"}})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )
            .await
            .expect("call")
            .structured_content
            .expect("structured");
        assert_eq!(data["index"]["state"], "indexing", "{data}");
        client.cancel().await.expect("cancel");
        Idling {
            handler,
            daemon,
            permit,
            feature,
            _socket: socket,
        }
    }

    #[test]
    fn idle_daemon_finishes_an_active_tree_job_before_exiting() {
        run_async(|repo| async move {
            let idling = client_leaves_during_job(repo, Duration::from_secs(600)).await;
            tokio::time::sleep(CLIENT_IDLE_TIMEOUT * 4).await;
            assert!(!idling.daemon.is_finished(), "daemon abandoned the job");
            drop(idling.permit);
            tokio::time::timeout(DEADLINE, idling.daemon)
                .await
                .expect("daemon exits once the job is done")
                .expect("task")
                .expect("daemon");
            let store = idling.handler.store_context().store;
            let store = store.lock().expect("store");
            let state = inspect_commit(&*store, "feat/x", &idling.feature);
            assert_eq!(state.state, "ready");
        });
    }

    #[test]
    fn idle_drain_is_capped_when_a_job_never_finishes() {
        run_async(|repo| async move {
            let idling = client_leaves_during_job(repo, Duration::from_millis(1)).await;
            tokio::time::timeout(DEADLINE, idling.daemon)
                .await
                .expect("the drain cap ends the wait")
                .expect("task")
                .expect("daemon");
            assert!(idling.handler.has_active_tree_job());
            drop(idling.permit);
        });
    }
}
