use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gitcortex_core::store::GraphStore;
use gitcortex_indexer::indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;
use notify::event::{EventKind, ModifyKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{info, warn};

const DEBOUNCE_MS: u64 = 500;

/// Spawn a background file watcher that re-indexes changed source files while
/// `gcx serve` runs. Debounces bursts to a single sync after 500 ms of quiet.
pub fn spawn_file_watcher(
    repo_root: PathBuf,
    store_arc: Arc<Mutex<KuzuGraphStore>>,
    branch: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_watcher(repo_root, store_arc, branch).await {
            warn!("file watcher stopped: {e}");
        }
    })
}

async fn run_watcher(
    repo_root: PathBuf,
    store_arc: Arc<Mutex<KuzuGraphStore>>,
    branch: String,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();

    let tx_clone = tx.clone();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };
            match event.kind {
                EventKind::Modify(ModifyKind::Data(_))
                | EventKind::Modify(ModifyKind::Any)
                | EventKind::Create(_) => {}
                _ => return,
            }
            for path in event.paths {
                if should_watch(&path) {
                    let _ = tx_clone.send(path);
                }
            }
        })?;
    watcher.watch(&repo_root, RecursiveMode::Recursive)?;
    info!("file watcher active on {}", repo_root.display());

    let mut pending: Vec<PathBuf> = Vec::new();
    loop {
        tokio::select! {
            path = rx.recv() => {
                match path {
                    Some(p) => {
                        if !pending.contains(&p) {
                            pending.push(p);
                        }
                    }
                    None => break,
                }
            }
            _ = sleep(Duration::from_millis(DEBOUNCE_MS)), if !pending.is_empty() => {
                let batch = std::mem::take(&mut pending);
                reindex_batch(repo_root.as_path(), &store_arc, &branch, batch);
            }
        }
    }
    Ok(())
}

fn reindex_batch(
    repo_root: &Path,
    store_arc: &Arc<Mutex<KuzuGraphStore>>,
    branch: &str,
    paths: Vec<PathBuf>,
) {
    let indexer = match IncrementalIndexer::new(repo_root) {
        Ok(i) => i,
        Err(e) => {
            warn!("watcher: indexer init failed: {e}");
            return;
        }
    };
    match indexer.index_files_from_disk(&paths) {
        Ok(diff) if diff.is_empty() => {}
        Ok(diff) => {
            let n_add = diff.added_nodes.len();
            let n_edge = diff.added_edges.len();
            let n_del = diff.removed_files.len();
            let mut store = match store_arc.lock() {
                Ok(s) => s,
                Err(_) => {
                    warn!("watcher: store mutex poisoned");
                    return;
                }
            };
            match store.apply_diff(branch, &diff) {
                Ok(()) => info!(
                    "watcher: re-indexed {} file(s) → +{n_add} nodes, +{n_edge} edges, -{n_del} stale",
                    paths.len()
                ),
                Err(e) => warn!("watcher: apply_diff failed: {e}"),
            }
        }
        Err(e) => warn!("watcher: index_files_from_disk failed: {e}"),
    }
}

fn should_watch(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/target/")
        || s.contains("/.git/")
        || s.contains("/node_modules/")
        || s.contains("/.gitcortex/")
        || s.ends_with(".lock")
        || s.ends_with(".tmp")
    {
        return false;
    }
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "go" | "java" | "md")
    )
}
