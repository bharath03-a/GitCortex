use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use gitcortex_core::store::GraphStore;
use gitcortex_indexer::indexer::IncrementalIndexer;
use gitcortex_store::kuzu::KuzuGraphStore;
use notify::event::{EventKind, ModifyKind};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::{interval, Instant, MissedTickBehavior};
use tracing::{info, warn};

const DEBOUNCE_MS: u64 = 500;
const MAX_BATCH_MS: u64 = 1_000;
const MAX_PENDING: usize = 4096;

fn queue_pending(pending: &mut HashSet<PathBuf>, path: PathBuf, rescan: &AtomicBool) {
    if pending.contains(&path) || pending.len() < MAX_PENDING {
        pending.insert(path);
    } else {
        rescan.store(true, Ordering::Release);
    }
}

fn indexes_event(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any | ModifyKind::Name(_))
            | EventKind::Create(_)
            | EventKind::Remove(_)
    )
}

/// Keep ingress and pending sets bounded. A dropped event triggers explicit
/// reconciliation; it is never treated as a successful incremental update.
pub fn spawn_file_watcher(
    repo_root: PathBuf,
    store_arc: Arc<Mutex<KuzuGraphStore>>,
    branch: Arc<Mutex<String>>,
    graph_revision: Arc<AtomicU64>,
    unhealthy: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    unhealthy.store(true, Ordering::Release);
    tokio::spawn(async move {
        if let Err(error) = run_watcher(
            repo_root,
            store_arc,
            branch,
            graph_revision,
            unhealthy.clone(),
        )
        .await
        {
            unhealthy.store(true, Ordering::Release);
            warn!("file watcher stopped: {error}");
        }
    })
}

async fn run_watcher(
    repo_root: PathBuf,
    store_arc: Arc<Mutex<KuzuGraphStore>>,
    branch: Arc<Mutex<String>>,
    graph_revision: Arc<AtomicU64>,
    unhealthy: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::channel::<PathBuf>(MAX_PENDING);
    let rescan = Arc::new(AtomicBool::new(true));
    let callback_rescan = rescan.clone();
    let callback_health = unhealthy.clone();
    let event_epoch = Arc::new(AtomicU64::new(0));
    let callback_epoch = event_epoch.clone();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<Event>| {
            let event = match res {
                Ok(event) => event,
                Err(_) => {
                    callback_rescan.store(true, Ordering::Release);
                    callback_health.store(true, Ordering::Release);
                    return;
                }
            };
            if event.need_rescan() {
                callback_rescan.store(true, Ordering::Release);
                callback_health.store(true, Ordering::Release);
            }
            if !indexes_event(event.kind) {
                return;
            }
            for path in event.paths {
                // Directory removal/rename may hide many source files. Reconcile
                // once rather than guessing which descendants still exist.
                if !should_watch(&path) {
                    if matches!(
                        event.kind,
                        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
                    ) && !ignored_path(&path)
                    {
                        callback_rescan.store(true, Ordering::Release);
                        callback_health.store(true, Ordering::Release);
                    }
                    continue;
                }
                callback_epoch.fetch_add(1, Ordering::AcqRel);
                callback_health.store(true, Ordering::Release);
                if tx.try_send(path).is_err() {
                    callback_rescan.store(true, Ordering::Release);
                    callback_health.store(true, Ordering::Release);
                }
            }
        })?;
    watcher.watch(&repo_root, RecursiveMode::Recursive)?;
    info!("file watcher active on {}", repo_root.display());

    let mut pending = HashSet::new();
    let mut snapshots = HashMap::new();
    let mut snapshot_limit = MAX_PENDING;

    let mut first_event = None;
    let mut last_event = Instant::now();
    let mut previous_scope: Option<(String, String)> = None;
    let mut observed_disk: HashMap<PathBuf, Option<(u64, Option<std::time::SystemTime>)>> =
        HashMap::new();
    let mut last_probe = Instant::now();
    let mut tick = interval(Duration::from_millis(DEBOUNCE_MS));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            path = rx.recv() => {
                let Some(path) = path else { break; };
                first_event.get_or_insert_with(Instant::now);
                last_event = Instant::now();
                queue_pending(&mut pending, path, &rescan);
                if rescan.load(Ordering::Acquire) { unhealthy.store(true, Ordering::Release); }
            }
            _ = tick.tick() => {
                let Some(current) = super::helpers::detect_current_branch(&repo_root) else {
                    unhealthy.store(true, Ordering::Release);
                    continue;
                };
                let Some(head) = git(&repo_root, &["rev-parse", "--verify", "HEAD"]) else {
                    unhealthy.store(true, Ordering::Release);
                    continue;
                };
                let scope = (current.clone(), head.clone());
                let branch_changed = previous_scope.as_ref().map_or(true, |previous| previous.0 != current);
                let head_changed = previous_scope.as_ref().is_some_and(|previous| previous.1 != head);
                let due = first_event.is_some_and(|first: Instant| first.elapsed() >= Duration::from_millis(MAX_BATCH_MS))
                    || (!pending.is_empty() && last_event.elapsed() >= Duration::from_millis(DEBOUNCE_MS));
                // Coalescing OS backends can omit a create event when a rename
                // happens in the same directory. Probe changed source metadata
                // on active batches, with an idle fallback every two seconds.
                if due || head_changed || branch_changed || last_probe.elapsed() >= Duration::from_secs(2) {
                    match super::readiness::changed_sources(&repo_root) {
                        Ok(paths) => {
                            let mut observed = HashMap::new();
                            for relative in paths {
                                let path = repo_root.join(relative);
                                let stamp = std::fs::symlink_metadata(&path).ok().map(|metadata| (metadata.len(), metadata.modified().ok()));
                                if observed_disk.get(&path) != Some(&stamp) { queue_pending(&mut pending, path.clone(), &rescan); }
                                observed.insert(path, stamp);
                            }
                            for old in observed_disk.keys() { if !observed.contains_key(old) { queue_pending(&mut pending, old.clone(), &rescan); } }
                            observed_disk = observed;
                            if !pending.is_empty() { first_event.get_or_insert_with(Instant::now); }
                        }
                        Err(_) => { rescan.store(true, Ordering::Release); }
                    }
                    last_probe = Instant::now();
                }
                let due = due || first_event.is_some_and(|first: Instant| first.elapsed() >= Duration::from_millis(MAX_BATCH_MS));
                let recover = rescan.swap(false, Ordering::AcqRel) || branch_changed;
                if !recover && !due && !head_changed { continue; }
                unhealthy.store(true, Ordering::Release);
                let epoch = event_epoch.load(Ordering::Acquire);
                if recover { snapshots.clear(); }
                let result = if recover {
                    reconcile_disk(&repo_root, &store_arc, &current, &head, &graph_revision, &mut snapshots)
                } else {
                    let mut paths: Vec<_> = pending.iter().cloned().collect();
                    paths.sort();
                    sync_commit(&repo_root, &store_arc, &current, &head, head_changed, &mut snapshots)
                        .and_then(|()| apply_disk_batch(&repo_root, &store_arc, &current, &head, &paths, &graph_revision, &mut snapshots))
                };
                match result {
                    Ok(()) => {
                        // Bound historical tombstones during long dirty coding
                        // sessions. Reconciliation compacts to the current
                        // source set, rather than retaining every past path.
                        if recover { snapshot_limit = snapshots.len().saturating_add(MAX_PENDING); }
                        else if snapshots.len() > snapshot_limit { rescan.store(true, Ordering::Release); }
                        previous_scope = Some(scope);
                        if let Ok(mut active) = branch.lock() { *active = current; }
                        pending.clear(); first_event = None;
                        // Never clear an overflow that arrived while applying.
                        unhealthy.store(rescan.load(Ordering::Acquire) || event_epoch.load(Ordering::Acquire) != epoch || !rx.is_empty(), Ordering::Release);
                    }
                    Err(error) => {
                        rescan.store(true, Ordering::Release);
                        warn!("watcher update failed; evidence remains degraded: {error}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn reconcile_disk(
    root: &Path,
    store: &Arc<Mutex<KuzuGraphStore>>,
    branch: &str,
    head: &str,
    revision: &AtomicU64,
    snapshots: &mut HashMap<PathBuf, Option<String>>,
) -> anyhow::Result<()> {
    // Broad work only on startup, scope change, or explicit event-loss recovery.
    let out = std::process::Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()?;
    anyhow::ensure!(out.status.success(), "could not enumerate source files");
    let mut paths: HashSet<PathBuf> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| root.join(std::ffi::OsStr::from_bytes(bytes)))
        .filter(|path| should_watch(path))
        .collect();
    {
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
        paths.extend(
            store
                .list_all_nodes(branch)?
                .into_iter()
                .map(|node| root.join(node.file))
                .filter(|path| should_watch(path)),
        );
    }
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    apply_disk_batch(root, store, branch, head, &paths, revision, snapshots)
}

fn apply_disk_batch(
    root: &Path,
    store: &Arc<Mutex<KuzuGraphStore>>,
    branch: &str,
    head: &str,
    paths: &[PathBuf],
    revision: &AtomicU64,
    snapshots: &mut HashMap<PathBuf, Option<String>>,
) -> anyhow::Result<()> {
    let indexer = IncrementalIndexer::new(root)?;
    let clean_before = super::readiness::changed_sources(root)?.is_empty();
    let diff = indexer.index_files_from_disk(paths)?;
    anyhow::ensure!(
        super::helpers::detect_current_branch(root).as_deref() == Some(branch)
            && git(root, &["rev-parse", "--verify", "HEAD"]).as_deref() == Some(head),
        "checkout changed while indexing"
    );
    let clean = super::readiness::changed_sources(root)?.is_empty();
    let mut store = store
        .lock()
        .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
    store.set_working_tree_dirty(branch, true)?;
    store.apply_diff(branch, &diff)?;
    for (path, hash) in &diff.source_hashes {
        snapshots.insert(path.clone(), hash.clone());
    }
    let verified = clean_before && clean && snapshots_match_commit(root, head, snapshots)?;
    // Persist uncertainty across daemon restarts and historical branch queries.
    // A dirty graph is useful evidence, but cannot justify a safety conclusion.
    store.set_last_indexed_sha(branch, head)?;
    store.set_working_tree_dirty(branch, !verified)?;
    if verified {
        snapshots.clear();
    }
    revision.fetch_add(1, Ordering::Release);
    anyhow::ensure!(
        super::helpers::detect_current_branch(root).as_deref() == Some(branch)
            && git(root, &["rev-parse", "--verify", "HEAD"]).as_deref() == Some(head),
        "checkout changed while applying index"
    );
    Ok(())
}

fn sync_commit(
    root: &Path,
    store: &Arc<Mutex<KuzuGraphStore>>,
    branch: &str,
    head: &str,
    changed: bool,
    snapshots: &mut HashMap<PathBuf, Option<String>>,
) -> anyhow::Result<()> {
    if !changed {
        return Ok(());
    }
    let mut store = store
        .lock()
        .map_err(|_| anyhow::anyhow!("store mutex poisoned"))?;
    let last = store.last_indexed_sha(branch)?;
    let base = last.as_deref();
    let (diff, indexed_head) = IncrementalIndexer::new(root)?.run(base)?;
    anyhow::ensure!(
        indexed_head == head
            && super::helpers::detect_current_branch(root).as_deref() == Some(branch),
        "checkout changed during commit synchronization"
    );
    store.set_working_tree_dirty(branch, true)?;
    store.apply_diff(branch, &diff)?;
    for (path, hash) in &diff.source_hashes {
        snapshots.insert(path.clone(), hash.clone());
    }
    Ok(())
}

use super::readiness::snapshots_match_commit;

use std::os::unix::ffi::OsStrExt;

fn ignored_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("target" | ".git" | "node_modules" | ".gitcortex")
        )
    })
}

fn should_watch(path: &std::path::Path) -> bool {
    if ignored_path(path)
        || matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("lock" | "tmp")
        )
    {
        return false;
    }
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "py"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "go"
                | "java"
                | "md"
                | "markdown"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{RemoveKind, RenameMode};

    #[test]
    fn deletion_and_rename_events_trigger_updates() {
        assert!(indexes_event(EventKind::Remove(RemoveKind::File)));
        assert!(indexes_event(EventKind::Modify(ModifyKind::Name(
            RenameMode::Both
        ))));
        assert!(!indexes_event(EventKind::Access(
            notify::event::AccessKind::Any
        )));
    }

    #[test]
    fn source_probe_flood_is_bounded_and_requests_reconciliation() {
        let mut pending = HashSet::new();
        let rescan = AtomicBool::new(false);
        for index in 0..MAX_PENDING + 100 {
            queue_pending(
                &mut pending,
                PathBuf::from(format!("file_{index}.rs")),
                &rescan,
            );
        }
        assert_eq!(pending.len(), MAX_PENDING);
        assert!(rescan.load(Ordering::Acquire));
        rescan.store(false, Ordering::Release);
        queue_pending(&mut pending, PathBuf::from("file_0.rs"), &rescan);
        assert!(
            !rescan.load(Ordering::Acquire),
            "duplicates need no recovery"
        );
    }

    #[test]
    fn parsed_dirty_bytes_cannot_be_certified_after_a_revert() {
        let tmp = tempfile::tempdir().expect("repo");
        let root = tmp.path();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "t@t"],
        ] {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git")
                .status
                .success());
        }
        let path = root.join("a.rs");
        std::fs::write(&path, "pub fn original() {}\n").expect("source");
        for args in [vec!["add", "a.rs"], vec!["commit", "-m", "start"]] {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git")
                .status
                .success());
        }
        let head = git(root, &["rev-parse", "HEAD"]).expect("head");
        std::fs::write(&path, "pub fn dirty() {}\n").expect("edit");
        let diff = IncrementalIndexer::new(root)
            .expect("indexer")
            .index_files_from_disk(std::slice::from_ref(&path))
            .expect("parse");
        std::fs::write(path, "pub fn original() {}\n").expect("revert");
        assert!(super::super::readiness::changed_sources(root)
            .expect("status")
            .is_empty());
        let snapshots = diff.source_hashes.into_iter().collect();
        assert!(!snapshots_match_commit(root, &head, &snapshots).expect("verify snapshot"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn continuous_edits_rename_delete_and_branch_switch_converge() {
        fn git(root: &Path, args: &[&str]) {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git");
            assert!(output.status.success(), "{args:?}: {:?}", output.stderr);
        }
        async fn wait_for(
            store: &Arc<Mutex<KuzuGraphStore>>,
            branch: &str,
            present: &str,
            absent: &[&str],
        ) {
            let started = Instant::now();
            loop {
                {
                    let store = store.lock().expect("store");
                    if !store
                        .lookup_symbol(branch, present, false)
                        .expect("lookup")
                        .is_empty()
                        && absent.iter().all(|name| {
                            store
                                .lookup_symbol(branch, name, false)
                                .expect("lookup")
                                .is_empty()
                        })
                    {
                        println!(
                            "checkpoint {branch}/{present}: {} ms",
                            started.elapsed().as_millis()
                        );
                        return;
                    }
                }
                assert!(
                    started.elapsed() < Duration::from_secs(12),
                    "watcher did not converge for {branch}/{present}"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().canonicalize().expect("root");
        git(&root, &["init", "-b", "main"]);
        std::fs::write(root.join("a.rs"), "pub fn original() {}\n").expect("source");
        git(&root, &["add", "a.rs"]);
        git(
            &root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@t",
                "commit",
                "-m",
                "start",
            ],
        );
        let store = Arc::new(Mutex::new(KuzuGraphStore::open(&root).expect("store")));
        let branch = Arc::new(Mutex::new("main".to_owned()));
        let revision = Arc::new(AtomicU64::new(0));
        let unhealthy = Arc::new(AtomicBool::new(false));
        let watcher = spawn_file_watcher(
            root.clone(),
            store.clone(),
            branch.clone(),
            revision.clone(),
            unhealthy.clone(),
        );
        wait_for(&store, "main", "original", &[]).await;
        std::fs::write(root.join("a.rs"), "pub fn edited() {}\n").expect("edit");
        wait_for(&store, "main", "edited", &["original"]).await;
        std::fs::rename(root.join("a.rs"), root.join("b.rs")).expect("rename");
        std::fs::write(root.join("c.rs"), "pub fn companion() {}\n").expect("create");
        wait_for(&store, "main", "companion", &[]).await;
        {
            let store = store.lock().expect("store");
            assert!(store
                .list_definitions("main", Path::new("a.rs"))
                .expect("old file")
                .is_empty());
            assert!(!store
                .list_definitions("main", Path::new("b.rs"))
                .expect("new file")
                .is_empty());
        }
        std::fs::remove_file(root.join("b.rs")).expect("delete");
        wait_for(&store, "main", "companion", &["edited"]).await;
        git(&root, &["add", "-A"]);
        git(
            &root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@t",
                "commit",
                "-m",
                "evolve",
            ],
        );
        git(&root, &["checkout", "-b", "feature"]);
        std::fs::write(root.join("c.rs"), "pub fn feature_only() {}\n").expect("feature");
        wait_for(&store, "feature", "feature_only", &["companion"]).await;
        git(&root, &["reset", "--hard", "HEAD"]);
        git(&root, &["checkout", "main"]);
        wait_for(
            &store,
            "main",
            "companion",
            &["feature_only", "original", "edited"],
        )
        .await;
        watcher.abort();
        let _ = watcher.await;
        // Restart reconciles an existing graph, including prior disk overlays.
        let watcher = spawn_file_watcher(root, store.clone(), branch, revision, unhealthy);
        wait_for(&store, "main", "companion", &["feature_only"]).await;
        watcher.abort();
        let _ = watcher.await;
    }
}
