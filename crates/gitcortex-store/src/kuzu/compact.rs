use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    schema::SCHEMA_VERSION,
};
use kuzu::{Connection, Database, SystemConfig, Value};

use super::{acquire_repository_lock, escape::esc, queries::NODE_COLS, KuzuGraphStore};
use crate::{branch, meta, schema as db_schema};

/// Checkpoints inside [`KuzuGraphStore::compact_with`] used for fault-injection
/// and lock-contention tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactPhase {
    Exported,
    Imported,
    OldMovedAside,
    NewInPlace,
    Verified,
}

/// Before/after payload sizes (in bytes) reported by [`KuzuGraphStore::compact`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactReport {
    pub before: u64,
    pub after: u64,
}

fn payload_size_bytes(dir: &Path) -> u64 {
    branch::payload_files_in_dir(dir)
        .into_iter()
        .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
        .sum()
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(GitCortexError::Io(err)),
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(GitCortexError::Io(err)),
    }
}

/// If a previous compaction crashed between moving the live payload aside and
/// renaming the compacted payload into place (`graph.kuzu` missing while a
/// `graph.kuzu.pre-compact-*` backup exists), restore the backup before opening.
///
/// Callers must hold the exclusive repository lock before calling this.
pub(super) fn recover_pre_compact_if_needed(repo_id: &str) -> Result<bool> {
    let dir = branch::data_dir(repo_id);
    let [live_db, live_wal] = branch::payload_files_in_dir(&dir);
    if live_db.exists() {
        return Ok(false);
    }

    let read_dir = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(GitCortexError::Io(err)),
    };

    let mut candidates: Vec<(Option<u64>, String)> = Vec::new();
    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let Some(suffix) = name_str.strip_prefix("graph.kuzu.pre-compact-") else {
            continue;
        };
        if suffix.is_empty() || suffix.ends_with(".wal") {
            continue;
        }
        candidates.push((suffix.parse::<u64>().ok(), suffix.to_owned()));
    }

    if candidates.is_empty() {
        return Ok(false);
    }

    candidates.sort();
    let (_, suffix) = candidates.pop().expect("non-empty candidates");
    let bak_db = dir.join(format!("graph.kuzu.pre-compact-{suffix}"));
    let bak_wal = dir.join(format!("graph.kuzu.pre-compact-{suffix}.wal"));

    fs::rename(&bak_db, &live_db)?;
    if bak_wal.exists() {
        fs::rename(&bak_wal, &live_wal)?;
    }

    // Remove the matching uncommitted staging dir, if any, so recovery leaves
    // the data directory clean.
    let staging = dir.join(format!("compact-{suffix}"));
    let _ = remove_dir_if_exists(&staging);

    tracing::warn!(
        repo_id,
        backup = %suffix,
        "restored graph.kuzu from interrupted compaction backup"
    );
    Ok(true)
}

fn encode_value(val: &Value, out: &mut Vec<u8>) {
    match val {
        Value::Null(_) => out.push(0),
        Value::Bool(b) => {
            out.push(1);
            out.push(u8::from(*b));
        }
        Value::Int64(i) => {
            out.push(2);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Value::Int32(i) => {
            out.push(3);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Value::String(s) => {
            out.push(4);
            let bytes = s.as_bytes();
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        other => {
            out.push(255);
            let s = format!("{other:?}");
            let bytes = s.as_bytes();
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
        }
    }
}

fn hash_sorted_rows(rows: &mut [Vec<u8>], hasher: &mut blake3::Hasher) {
    rows.sort();
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for row in rows.iter() {
        hasher.update(&(row.len() as u64).to_le_bytes());
        hasher.update(row);
    }
}

fn list_all_table_names(conn: &Connection) -> Result<HashSet<String>> {
    let mut rows = conn
        .query("CALL show_tables() RETURN name")
        .map_err(|e| GitCortexError::Store(format!("list tables: {e}")))?;
    let mut names = HashSet::new();
    for row in rows.by_ref() {
        if let Some(Value::String(name)) = row.first() {
            names.insert(name.clone());
        }
    }
    Ok(names)
}

/// Compute a deterministic blake3 digest over all nodes, edges, and reference
/// facts for each branch table key in `table_keys`.
fn compute_db_fingerprints(
    conn: &Connection,
    table_keys: &[String],
) -> Result<BTreeMap<String, [u8; 32]>> {
    let existing_tables = list_all_table_names(conn)?;
    let mut digests = BTreeMap::new();

    for key in table_keys {
        let nt = format!("{key}_nodes");
        let et = format!("{key}_edges");
        let rt = format!("{key}_references");
        let mut hasher = blake3::Hasher::new();

        // Nodes
        let mut node_rows = Vec::new();
        let mut rows = conn
            .query(&format!("MATCH (n:{nt}) RETURN {NODE_COLS}"))
            .map_err(|e| GitCortexError::Store(format!("fingerprint nodes ({key}): {e}")))?;
        for row in rows.by_ref() {
            let mut buf = Vec::new();
            for val in &row {
                encode_value(val, &mut buf);
            }
            node_rows.push(buf);
        }
        hasher.update(b"NODES");
        hash_sorted_rows(&mut node_rows, &mut hasher);

        // Edges
        let mut edge_rows = Vec::new();
        if existing_tables.contains(&et) {
            let mut rows = conn
                .query(&format!(
                    "MATCH (s:{nt})-[e:{et}]->(d:{nt}) \
                     RETURN s.id, d.id, e.kind, e.line, e.confidence"
                ))
                .map_err(|e| GitCortexError::Store(format!("fingerprint edges ({key}): {e}")))?;
            for row in rows.by_ref() {
                let mut buf = Vec::new();
                for val in &row {
                    encode_value(val, &mut buf);
                }
                edge_rows.push(buf);
            }
        }
        hasher.update(b"EDGES");
        hash_sorted_rows(&mut edge_rows, &mut hasher);

        // References
        let mut ref_rows = Vec::new();
        if existing_tables.contains(&rt) {
            let mut rows = conn
                .query(&format!(
                    "MATCH (r:{rt}) \
                     RETURN r.id, r.src_id, r.src_file, r.target_name, r.kind, r.line"
                ))
                .map_err(|e| {
                    GitCortexError::Store(format!("fingerprint references ({key}): {e}"))
                })?;
            for row in rows.by_ref() {
                let mut buf = Vec::new();
                for val in &row {
                    encode_value(val, &mut buf);
                }
                ref_rows.push(buf);
            }
        }
        hasher.update(b"REFS");
        hash_sorted_rows(&mut ref_rows, &mut hasher);

        digests.insert(key.clone(), *hasher.finalize().as_bytes());
    }

    Ok(digests)
}

fn allocate_compact_ts(repo_id: &str) -> u64 {
    let mut ts = meta::now_unix_secs();
    loop {
        let staging = branch::staging_dir(repo_id, ts);
        let [bak_db, bak_wal] = branch::pre_compact_payload_files(repo_id, ts);
        if !staging.exists() && !bak_db.exists() && !bak_wal.exists() {
            return ts;
        }
        ts = ts.saturating_add(1);
    }
}

impl KuzuGraphStore {
    /// Reclaim unused space inside `graph.kuzu` by exporting to a same-filesystem
    /// staging directory, re-importing into a fresh Kuzu database, atomically
    /// swapping only the payload files under the repository lock, and verifying
    /// every table and branch fingerprint before deleting the pre-compact backup.
    pub fn compact(repo_root: &Path) -> Result<CompactReport> {
        Self::compact_with(repo_root, None)
    }

    /// Fault-injection entry point for compaction tests. When `fail_after` is
    /// `Some(phase)`, compaction aborts immediately after completing `phase`
    /// and rolls back so the original store and all sidecars remain intact.
    #[doc(hidden)]
    pub fn compact_with(
        repo_root: &Path,
        fail_after: Option<CompactPhase>,
    ) -> Result<CompactReport> {
        Self::compact_with_hook(repo_root, |phase| {
            if fail_after == Some(phase) {
                Err(GitCortexError::Store(format!(
                    "injected compaction failure after {phase:?}"
                )))
            } else {
                Ok(())
            }
        })
    }

    /// Phase-hook entry point used by integration tests to observe or inject
    /// failures at each [`CompactPhase`] while holding the repository lock.
    #[doc(hidden)]
    pub fn compact_with_hook<F>(repo_root: &Path, mut on_phase: F) -> Result<CompactReport>
    where
        F: FnMut(CompactPhase) -> Result<()>,
    {
        // Acquire ONE exclusive repository lock and keep it alive (on its
        // original `serve.lock` inode) for the entire compaction.
        let _lock = acquire_repository_lock(repo_root, "storage compact")?;

        let repo_id = branch::storage_repo_id(repo_root);
        let dir = branch::data_dir(&repo_id);

        // If a prior compaction crashed mid-swap, restore before proceeding.
        recover_pre_compact_if_needed(&repo_id)?;

        let [live_db, live_wal] = branch::payload_files_in_dir(&dir);
        if !live_db.exists() {
            return Err(GitCortexError::Store(
                "no graph store found for repository; run `gcx init` first".to_owned(),
            ));
        }

        let persisted_schema = branch::read_schema_version(&repo_id);
        if persisted_schema != SCHEMA_VERSION {
            return Err(GitCortexError::Store(format!(
                "graph schema version {persisted_schema} is incompatible with expected version {SCHEMA_VERSION}; run `gcx init` to rebuild the local index"
            )));
        }

        let before = payload_size_bytes(&dir);
        let ts = allocate_compact_ts(&repo_id);
        let staging = branch::staging_dir(&repo_id, ts);
        let export_dir = staging.join("export");
        let [staged_db, staged_wal] = branch::payload_files_in_dir(&staging);
        let [bak_db, bak_wal] = branch::pre_compact_payload_files_in_dir(&dir, ts);

        let mut old_moved_aside = false;
        let mut new_in_place = false;

        let outcome: Result<()> = (|| {
            fs::create_dir_all(&staging)?;

            // Phase 1: open live DB, capture pre-compact fingerprints, EXPORT DATABASE.
            let (pre_tables, pre_fingerprints, expected_registered) = {
                let live = Database::new(&live_db, SystemConfig::default())
                    .map_err(|e| GitCortexError::Store(format!("open db for compact: {e}")))?;
                let conn = Connection::new(&live)
                    .map_err(|e| GitCortexError::Store(format!("connect db for compact: {e}")))?;

                let pre_tables = db_schema::list_branch_table_keys(&conn)?;
                let pre_table_set: HashSet<&str> = pre_tables.iter().map(String::as_str).collect();
                let registry = meta::read_branch_registry(&repo_id)?;
                let expected_registered: Vec<String> = registry
                    .into_iter()
                    .map(|e| e.key)
                    .filter(|k| pre_table_set.contains(k.as_str()))
                    .collect();
                let pre_fingerprints = compute_db_fingerprints(&conn, &pre_tables)?;

                let export_escaped = esc(&export_dir.to_string_lossy());
                conn.query(&format!("EXPORT DATABASE '{export_escaped}'"))
                    .map_err(|e| GitCortexError::Store(format!("compact export database: {e}")))?;

                (pre_tables, pre_fingerprints, expected_registered)
            };
            // Live DB handle is dropped here before signaling Exported.
            on_phase(CompactPhase::Exported)?;

            // Phase 2: IMPORT DATABASE into staging/graph.kuzu, then drop handle.
            {
                let staged = Database::new(&staged_db, SystemConfig::default()).map_err(|e| {
                    GitCortexError::Store(format!("create staged db for compact: {e}"))
                })?;
                let conn = Connection::new(&staged).map_err(|e| {
                    GitCortexError::Store(format!("connect staged db for compact: {e}"))
                })?;
                let export_escaped = esc(&export_dir.to_string_lossy());
                conn.query(&format!("IMPORT DATABASE '{export_escaped}'"))
                    .map_err(|e| GitCortexError::Store(format!("compact import database: {e}")))?;
            }
            // Staged DB handle is dropped here before moving files.
            on_phase(CompactPhase::Imported)?;

            // Phase 3: rename live payload aside to graph.kuzu.pre-compact-<ts>{,.wal}.
            fs::rename(&live_db, &bak_db)?;
            old_moved_aside = true;
            if live_wal.exists() {
                fs::rename(&live_wal, &bak_wal)?;
            }
            on_phase(CompactPhase::OldMovedAside)?;

            // Phase 4: rename staged payload into live position.
            fs::rename(&staged_db, &live_db)?;
            new_in_place = true;
            if staged_wal.exists() {
                fs::rename(&staged_wal, &live_wal)?;
            }
            on_phase(CompactPhase::NewInPlace)?;

            // Phase 5: reopen live payload and verify schema version, registry keys,
            // table set, and per-branch structural fingerprints.
            {
                let post_schema = branch::read_schema_version(&repo_id);
                if post_schema != SCHEMA_VERSION {
                    return Err(GitCortexError::Store(format!(
                        "compact verification failed: schema version {post_schema} != {SCHEMA_VERSION}"
                    )));
                }
                let reopened = Database::new(&live_db, SystemConfig::default()).map_err(|e| {
                    GitCortexError::Store(format!("reopen compacted db for verification: {e}"))
                })?;
                let conn = Connection::new(&reopened).map_err(|e| {
                    GitCortexError::Store(format!("connect compacted db for verification: {e}"))
                })?;
                let post_tables = db_schema::list_branch_table_keys(&conn)?;
                if post_tables != pre_tables {
                    return Err(GitCortexError::Store(format!(
                        "compact verification failed: branch tables mismatch (before={pre_tables:?}, after={post_tables:?})"
                    )));
                }
                let post_table_set: HashSet<&str> =
                    post_tables.iter().map(String::as_str).collect();
                for key in &expected_registered {
                    if !post_table_set.contains(key.as_str()) {
                        return Err(GitCortexError::Store(format!(
                            "compact verification failed: registered branch key '{key}' missing after import"
                        )));
                    }
                }
                let post_fingerprints = compute_db_fingerprints(&conn, &post_tables)?;
                if post_fingerprints != pre_fingerprints {
                    return Err(GitCortexError::Store(
                        "compact verification failed: branch content fingerprint mismatch"
                            .to_owned(),
                    ));
                }
            }
            on_phase(CompactPhase::Verified)?;

            Ok(())
        })();

        if let Err(err) = outcome {
            // Best-effort rollback to the untouched pre-compact payload and clean up
            // staging artifacts so the store remains openable with zero Unknown files.
            if new_in_place {
                let _ = remove_if_exists(&live_db);
                let _ = remove_if_exists(&live_wal);
            }
            if old_moved_aside {
                if bak_db.exists() {
                    let _ = fs::rename(&bak_db, &live_db);
                }
                if bak_wal.exists() {
                    let _ = fs::rename(&bak_wal, &live_wal);
                }
            }
            let _ = remove_dir_if_exists(&staging);
            return Err(err);
        }

        // Verification succeeded: delete pre-compact backup and staging dir.
        remove_if_exists(&bak_db)?;
        remove_if_exists(&bak_wal)?;
        remove_dir_if_exists(&staging)?;

        let after = payload_size_bytes(&dir);
        Ok(CompactReport { before, after })
    }
}
