use std::fmt;

/// Typed process exit classes shared across CLI subcommands.
///
/// Process exit codes:
/// - `2`: invalid user input (unknown/ambiguous seed, invalid git ref or revision, bad value)
/// - `3`: index not ready (`missing`, `stale`, `unverified`, `unavailable`, etc.)
/// - `1`: any other operational or internal error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExit {
    InvalidInput,
    NotReady,
}

impl fmt::Display for CliExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => f.write_str("invalid input"),
            Self::NotReady => f.write_str("index not ready"),
        }
    }
}

impl std::error::Error for CliExit {}

/// Map an [`anyhow::Error`] to its process exit code by walking the full
/// error context chain (both root errors and attached context values).
pub fn exit_code(error: &anyhow::Error) -> i32 {
    if let Some(kind) = error.downcast_ref::<CliExit>() {
        return match kind {
            CliExit::InvalidInput => 2,
            CliExit::NotReady => 3,
        };
    }
    for cause in error.chain() {
        if let Some(kind) = cause.downcast_ref::<CliExit>() {
            return match kind {
                CliExit::InvalidInput => 2,
                CliExit::NotReady => 3,
            };
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_downcasts_through_context() {
        let not_ready = anyhow::Error::new(CliExit::NotReady)
            .context("index is stale on branch 'main'")
            .context("export aborted");
        assert_eq!(exit_code(&not_ready), 3);

        let invalid_root = anyhow::Error::new(CliExit::InvalidInput)
            .context("unknown branch ref")
            .context("export aborted");
        assert_eq!(exit_code(&invalid_root), 2);

        let invalid_context = anyhow::anyhow!("ambiguous seed")
            .context(CliExit::InvalidInput)
            .context("export aborted");
        assert_eq!(exit_code(&invalid_context), 2);

        let plain = anyhow::anyhow!("failed to open graph store");
        assert_eq!(exit_code(&plain), 1);
    }
}
