//! Defensive extractors from `kuzu::Value` to native Rust types, with `Null`
//! handled gracefully (returning empty / false rather than failing the whole
//! query — important for rows written before a schema column existed).

use gitcortex_core::error::{GitCortexError, Result};
use kuzu::Value;

pub(super) fn str_val(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        // KuzuDB returns Null(String) for empty-string columns inserted after a
        // DETACH DELETE in a prior transaction. Treat as empty string.
        Value::Null(_) => Ok(String::new()),
        other => Err(GitCortexError::Store(format!(
            "expected String, got {other:?}"
        ))),
    }
}

pub(super) fn i64_val(v: &Value) -> Result<i64> {
    match v {
        Value::Int64(n) => Ok(*n),
        Value::Int32(n) => Ok(*n as i64),
        other => Err(GitCortexError::Store(format!(
            "expected Int64, got {other:?}"
        ))),
    }
}

pub(super) fn bool_val(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        // Null booleans arise from legacy rows written before the column existed;
        // treat them as false rather than failing the whole query.
        Value::Null(_) => Ok(false),
        other => Err(GitCortexError::Store(format!(
            "expected Bool, got {other:?}"
        ))),
    }
}
