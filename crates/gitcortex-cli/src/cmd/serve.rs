use std::{
    fs::File,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

const COMPACT_MODE: u8 = 0;
const FULL_MODE: u8 = 1;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

pub fn run(compact: bool) -> Result<()> {
    let repo_root = repo_root()?;
    let rt = tokio::runtime::Runtime::new().context("failed to build tokio runtime")?;
    rt.block_on(proxy_stdio(repo_root, compact))
}

/// Entry point for the hidden daemon subprocess. The public `gcx serve`
/// command remains a stdio MCP process from the editor's perspective.
pub fn run_daemon(repo_root: PathBuf) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("failed to build tokio runtime")?;
    let result = rt.block_on(gitcortex_mcp::mcp::server::serve_daemon(repo_root));
    // A semantic embedding batch runs on Tokio's blocking pool and cannot be
    // cancelled mid-inference. Do not keep an otherwise idle daemon (and its
    // Kuzu lock) alive waiting for that replaceable background work to finish.
    rt.shutdown_timeout(Duration::from_millis(100));
    result
}

async fn proxy_stdio(repo_root: PathBuf, compact: bool) -> Result<()> {
    let mut stream = connect_or_start(&repo_root).await?;
    stream
        .write_u8(if compact { COMPACT_MODE } else { FULL_MODE })
        .await
        .context("select repository daemon MCP mode")?;

    let (mut socket_read, mut socket_write) = stream.into_split();
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let upstream = async {
        tokio::io::copy(&mut stdin, &mut socket_write)
            .await
            .context("forward MCP requests to repository daemon")?;
        socket_write.shutdown().await?;
        Ok::<_, anyhow::Error>(())
    };
    let downstream = async {
        tokio::io::copy(&mut socket_read, &mut stdout)
            .await
            .context("forward MCP responses from repository daemon")?;
        stdout.flush().await?;
        Ok::<_, anyhow::Error>(())
    };
    tokio::pin!(upstream, downstream);

    // If the daemon side closes unexpectedly, do not leave an editor waiting
    // forever for the still-open stdin stream. On normal stdin EOF, wait for
    // the daemon to flush the final MCP response before returning.
    tokio::select! {
        result = &mut upstream => {
            result?;
            downstream.await?;
        }
        result = &mut downstream => {
            result?;
        }
    }
    Ok(())
}

async fn connect_or_start(repo_root: &Path) -> Result<UnixStream> {
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
    loop {
        match UnixStream::connect(&socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(error) if tokio::time::Instant::now() >= deadline => {
                let status = child.try_wait().ok().flatten();
                let detail = std::fs::read_to_string(&log_path)
                    .ok()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| format!("\n{}", text.trim()))
                    .unwrap_or_default();
                anyhow::bail!(
                    "repository daemon did not become ready within {}s{}: {error}{detail}",
                    START_TIMEOUT.as_secs(),
                    status
                        .map(|value| format!(" (child exited with {value})"))
                        .unwrap_or_default(),
                );
            }
            Err(_) => tokio::time::sleep(RETRY_INTERVAL).await,
        }
    }
}

fn repo_root() -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git rev-parse failed")?;
    if !out.status.success() {
        anyhow::bail!("not inside a Git repository");
    }
    Ok(PathBuf::from(
        String::from_utf8(out.stdout)?.trim().to_owned(),
    ))
}
