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
            is_static      BOOL,   \
            is_abstract    BOOL,   \
            is_final       BOOL,   \
            is_property    BOOL,   \
            is_generator   BOOL,   \
            is_const       BOOL,   \
            generic_bounds STRING, \
            def_signature  STRING, \
            def_body       STRING, \
            def_doc        STRING, \
            def_start_byte INT64,  \
            def_end_byte   INT64,  \
            complexity     INT64,  \
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

    // Secondary indexes on columns hit by every deferred-resolution WHERE clause.
    // Best-effort: KuzuDB auto-indexes PKs; secondary index support depends on
    // the runtime version. Warn and continue rather than fail init.
    for (idx, col) in [
        (format!("{nt}_name_idx"), "name"),
        (format!("{nt}_qname_idx"), "qualified_name"),
        (format!("{nt}_file_idx"), "file"),
    ] {
        if let Err(e) = conn.query(&format!("CREATE INDEX IF NOT EXISTS {idx} ON {nt}({col})")) {
            tracing::debug!("secondary index {idx} skipped: {e}");
        }
    }

    Ok(())
}
