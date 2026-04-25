use gitcortex_core::error::{GitCortexError, Result};
use kuzu::Connection;

use crate::branch;

// ── Table name helpers ────────────────────────────────────────────────────────

/// Node table name for a branch: `{sanitized_branch}_nodes`
pub fn node_table(branch: &str) -> String {
    format!("{}_nodes", branch::sanitize(branch))
}

/// Relationship table name for a branch: `{sanitized_branch}_edges`
pub fn edge_table(branch: &str) -> String {
    format!("{}_edges", branch::sanitize(branch))
}

// ── DDL ───────────────────────────────────────────────────────────────────────

/// Create the node and edge tables for `branch` if they don't already exist.
/// Safe to call on every operation — idempotent.
pub fn ensure_branch(conn: &mut Connection, branch: &str) -> Result<()> {
    let nt = node_table(branch);
    let et = edge_table(branch);

    conn.query(&format!(
        "CREATE NODE TABLE IF NOT EXISTS {nt} (\
            id             STRING, \
            kind           STRING, \
            name           STRING, \
            qualified_name STRING, \
            file           STRING, \
            start_line     INT64,  \
            end_line       INT64,  \
            loc            INT64,  \
            visibility     STRING, \
            is_async       BOOL,   \
            is_unsafe      BOOL,   \
            PRIMARY KEY(id)\
        )"
    ))
    .map_err(|e| GitCortexError::Store(format!("create node table: {e}")))?;

    conn.query(&format!(
        "CREATE REL TABLE IF NOT EXISTS {et} (\
            FROM {nt} TO {nt},\
            kind STRING\
        )"
    ))
    .map_err(|e| GitCortexError::Store(format!("create edge table: {e}")))?;

    Ok(())
}
