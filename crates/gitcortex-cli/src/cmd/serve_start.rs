use std::{
    fs::File,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use gitcortex_store::branch::RepositoryLock;
use tokio::net::UnixStream;

/// Covers `LOCK_WAIT` plus Kuzu open; editors allow ~30s for MCP startup.
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// A hook re-index or an exiting daemon holds the graph lock briefly.
const LOCK_WAIT: Duration = Duration::from_secs(15);
/// Another racing daemon may still bind after ours lost and exited.
const EXIT_GRACE: Duration = Duration::from_secs(1);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const LOCK_POLL: Duration = Duration::from_millis(100);
const LOG_TAIL_CHARS: usize = 800;

pub enum Ownership {
    Acquirable,
    AlreadyServed,
}

/// Wait out short-lived graph owners instead of failing the editor's connect.
pub fn wait_for_repository(repo_root: &Path) -> Result<Ownership> {
    let socket = gitcortex_mcp::mcp::server::daemon_socket_path(repo_root);
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            return Ok(Ownership::AlreadyServed);
        }
        if RepositoryLock::try_acquire(repo_root)?.is_some() || Instant::now() >= deadline {
            // Past the deadline the store open reports who holds the lock.
            return Ok(Ownership::Acquirable);
        }
        std::thread::sleep(LOCK_POLL);
    }
}

pub async fn connect_or_start(repo_root: &Path) -> Result<UnixStream> {
    let socket_path = gitcortex_mcp::mcp::server::daemon_socket_path(repo_root);
    if let Ok(stream) = UnixStream::connect(&socket_path).await {
        return Ok(stream);
    }

    let log_path = gitcortex_mcp::mcp::server::daemon_log_path(repo_root);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = File::create(&log_path)
        .with_context(|| format!("create daemon log {}", log_path.display()))?;
    let stderr = log.try_clone()?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("__serve-daemon")
        .arg("--repo-root")
        .arg(repo_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("start GitCortex repository daemon")?;

    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    let mut give_up = deadline;
    let mut exited: Option<ExitStatus> = None;
    loop {
        match UnixStream::connect(&socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                if exited.is_none() {
                    if let Ok(Some(status)) = child.try_wait() {
                        exited = Some(status);
                        give_up = (tokio::time::Instant::now() + EXIT_GRACE).min(deadline);
                    }
                }
                if tokio::time::Instant::now() >= give_up {
                    anyhow::bail!(startup_failure(&log_path, exited, &error));
                }
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
        }
    }
}

fn startup_failure(log_path: &Path, exited: Option<ExitStatus>, error: &std::io::Error) -> String {
    let headline = match exited {
        Some(status) => format!("repository daemon exited during startup ({status})"),
        None => format!(
            "repository daemon did not become ready within {}s",
            START_TIMEOUT.as_secs()
        ),
    };
    let detail = std::fs::read_to_string(log_path)
        .ok()
        .map(|text| log_tail(&text))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| error.to_string());
    format!("{headline}: {detail}")
}

fn log_tail(text: &str) -> String {
    let trimmed = text.trim();
    let start = trimmed
        .char_indices()
        .rev()
        .nth(LOG_TAIL_CHARS)
        .map_or(0, |(index, _)| index);
    trimmed[start..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::log_tail;

    #[test]
    fn log_tail_keeps_the_last_error_and_stays_bounded() {
        let long = format!("{}\nerror: schema mismatch", "x".repeat(5_000));
        let tail = log_tail(&long);
        assert!(tail.ends_with("error: schema mismatch"));
        assert!(tail.chars().count() <= super::LOG_TAIL_CHARS + 1);
    }
}
