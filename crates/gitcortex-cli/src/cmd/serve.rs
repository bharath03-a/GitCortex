use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use super::serve_start::{self, Ownership};

const COMPACT_MODE: u8 = 0;
const FULL_MODE: u8 = 1;
/// A daemon past its idle deadline can drop a just-queued connection unread.
const HANDSHAKE_ATTEMPTS: usize = 4;
const HANDSHAKE_RETRY: Duration = Duration::from_millis(100);
const STARTUP_ERROR_CODE: i64 = -32603;

pub fn run(compact: bool) -> Result<()> {
    let repo_root = repo_root()?;
    let rt = tokio::runtime::Runtime::new().context("failed to build tokio runtime")?;
    rt.block_on(proxy_stdio(repo_root, compact))
}

/// Entry point for the hidden daemon subprocess. The public `gcx serve`
/// command remains a stdio MCP process from the editor's perspective.
pub fn run_daemon(repo_root: PathBuf) -> Result<()> {
    if let Ownership::AlreadyServed = serve_start::wait_for_repository(&repo_root)? {
        return Ok(());
    }
    let rt = tokio::runtime::Runtime::new().context("failed to build tokio runtime")?;
    let result = rt.block_on(gitcortex_mcp::mcp::server::serve_daemon(repo_root));
    // A semantic embedding batch runs on Tokio's blocking pool and cannot be
    // cancelled mid-inference. Do not keep an otherwise idle daemon (and its
    // Kuzu lock) alive waiting for that replaceable background work to finish.
    rt.shutdown_timeout(Duration::from_millis(100));
    result
}

async fn proxy_stdio(repo_root: PathBuf, compact: bool) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let Some(first) = lines
        .next_line()
        .await
        .context("read MCP request from editor")?
    else {
        return Ok(());
    };
    let first = rewrite_handshake(&first);
    let mut stdout = tokio::io::stdout();
    let (mut socket_read, mut socket_write, first_reply) =
        match handshake(&repo_root, compact, &first).await {
            Ok(parts) => parts,
            Err(error) => {
                // An unanswered `initialize` surfaces as CONNECTION_CLOSED with
                // no cause; a JSON-RPC error carries the cause to the editor.
                if let Some(reply) = startup_error_reply(&first, &error) {
                    stdout.write_all(reply.as_bytes()).await.ok();
                    stdout.flush().await.ok();
                }
                return Err(error);
            }
        };
    if let Some(reply) = first_reply {
        stdout.write_all(reply.as_bytes()).await?;
        stdout.flush().await?;
    }

    let upstream = async {
        while let Some(line) = lines
            .next_line()
            .await
            .context("read MCP request from editor")?
        {
            forward_line(&mut socket_write, &rewrite_handshake(&line))
                .await
                .context("forward MCP requests to repository daemon")?;
        }
        socket_write.shutdown().await?;
        Ok::<_, anyhow::Error>(())
    };
    let downstream = async {
        tokio::io::copy_buf(&mut socket_read, &mut stdout)
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

type Session = (BufReader<OwnedReadHalf>, OwnedWriteHalf, Option<String>);

/// Deliver the first request and, when it expects a reply, wait for any line
/// back: only then has the daemon accepted this client instead of exiting.
async fn handshake(repo_root: &std::path::Path, compact: bool, first: &str) -> Result<Session> {
    let expects_reply = serde_json::from_str::<serde_json::Value>(first)
        .ok()
        .is_some_and(|value| value.get("id").is_some());
    let mode = if compact { COMPACT_MODE } else { FULL_MODE };
    for attempt in 1..=HANDSHAKE_ATTEMPTS {
        let (read, mut write) = serve_start::connect_or_start(repo_root).await?.into_split();
        let mut reader = BufReader::new(read);
        let sent = async {
            write.write_u8(mode).await?;
            forward_line(&mut write, first).await
        }
        .await;
        if sent.is_ok() {
            if !expects_reply {
                return Ok((reader, write, None));
            }
            let mut reply = String::new();
            if matches!(reader.read_line(&mut reply).await, Ok(read) if read > 0) {
                return Ok((reader, write, Some(reply)));
            }
        }
        if attempt < HANDSHAKE_ATTEMPTS {
            tokio::time::sleep(HANDSHAKE_RETRY).await;
        }
    }
    anyhow::bail!("repository daemon closed {HANDSHAKE_ATTEMPTS} connections before answering")
}

async fn forward_line(write: &mut OwnedWriteHalf, line: &str) -> std::io::Result<()> {
    write.write_all(line.as_bytes()).await?;
    write.write_all(b"\n").await
}

fn startup_error_reply(first: &str, error: &anyhow::Error) -> Option<String> {
    let request: serde_json::Value = serde_json::from_str(first).ok()?;
    let id = request.get("id")?.clone();
    let reply = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": STARTUP_ERROR_CODE,
            "message": format!("GitCortex MCP server could not start: {error:#}. Next: run `gcx doctor`."),
        },
    });
    Some(format!("{reply}\n"))
}

/// Antigravity's `agy` CLI sends a non-standard `server/discover` handshake
/// instead of the MCP spec's `initialize`, with client capabilities/info
/// namespaced under `params._meta` rather than as top-level params. Rewrite
/// it into a spec-compliant `initialize` request so the daemon's `rmcp`
/// server (which only understands the standard method) actually responds
/// instead of silently ignoring an unrecognized method forever. Any other
/// line passes through unchanged.
fn rewrite_handshake(line: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_owned();
    };
    if value.get("method").and_then(|m| m.as_str()) != Some("server/discover") {
        return line.to_owned();
    }
    let meta = value
        .pointer("/params/_meta")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let capabilities = meta
        .get("io.modelcontextprotocol/clientCapabilities")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let client_info = meta
        .get("io.modelcontextprotocol/clientInfo")
        .cloned()
        .unwrap_or(serde_json::json!({"name": "unknown", "version": "0.0.0"}));

    value["method"] = serde_json::Value::String("initialize".to_owned());
    value["params"] = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": capabilities,
        "clientInfo": client_info,
    });
    serde_json::to_string(&value).unwrap_or_else(|_| line.to_owned())
}

fn repo_root() -> Result<PathBuf> {
    crate::cmd::init::helpers::repo_root()
}

#[cfg(test)]
mod tests {
    use super::{rewrite_handshake, startup_error_reply};

    #[test]
    fn startup_failure_answers_the_pending_request_with_a_next_step() {
        let first = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#;
        let error = anyhow::anyhow!("graph schema version 14 is incompatible");
        let reply: serde_json::Value =
            serde_json::from_str(&startup_error_reply(first, &error).expect("reply"))
                .expect("valid JSON");
        assert_eq!(reply["id"], 7);
        let message = reply["error"]["message"].as_str().expect("message");
        assert!(message.contains("schema version 14"), "{message}");
        assert!(message.contains("gcx doctor"), "{message}");
        let notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(startup_error_reply(notification, &error).is_none());
    }

    #[test]
    fn rewrites_antigravity_server_discover_to_initialize() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/clientCapabilities":{"roots":{"listChanged":true}},"io.modelcontextprotocol/clientInfo":{"name":"antigravity-client","version":"v1.0.0"},"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#;
        let rewritten: serde_json::Value =
            serde_json::from_str(&rewrite_handshake(line)).expect("valid JSON");

        assert_eq!(rewritten["method"], "initialize");
        assert_eq!(rewritten["id"], 1);
        assert_eq!(rewritten["params"]["protocolVersion"], "2024-11-05");
        assert_eq!(
            rewritten["params"]["clientInfo"]["name"],
            "antigravity-client"
        );
        assert_eq!(
            rewritten["params"]["capabilities"]["roots"]["listChanged"],
            true
        );
    }

    #[test]
    fn passes_through_standard_initialize_unchanged() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
        assert_eq!(rewrite_handshake(line), line);
    }

    #[test]
    fn passes_through_non_json_and_other_methods_unchanged() {
        assert_eq!(rewrite_handshake("not json"), "not json");
        let other = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{}}"#;
        assert_eq!(rewrite_handshake(other), other);
    }
}
