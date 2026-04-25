use std::path::PathBuf;

use thiserror::Error;

/// Central error type for all GitCortex crates.
///
/// Each variant is a distinct failure domain. Crates that wrap external
/// library errors (e.g. git2, kuzu) convert them to the appropriate variant
/// at their own boundary — keeping this crate free of I/O dependencies.
#[derive(Debug, Error)]
pub enum GitCortexError {
    #[error("parse error in {file}: {message}")]
    Parse { file: PathBuf, message: String },

    /// Git operation failed. Populated by gitcortex-indexer.
    #[error("git error: {0}")]
    Git(String),

    /// Graph store operation failed. Populated by gitcortex-store.
    #[error("store error: {0}")]
    Store(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("branch '{branch}' not found in store")]
    BranchNotFound { branch: String },

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, GitCortexError>;
