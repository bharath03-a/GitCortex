#![cfg(unix)]

use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

const GCX: &str = env!("CARGO_BIN_EXE_gcx");

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl McpClient {
    fn start(repo: &Path, data: &Path, cache: &Path, full: bool) -> Self {
        let mut command = Command::new(GCX);
        command
            .arg("serve")
            .current_dir(repo)
            .env("GCX_STORE_PATH", data)
            .env("GCX_CACHE_PATH", cache)
            .env("GCX_DISABLE_SEMANTIC", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if full {
            command.arg("--full");
        }
        let mut child = command.spawn().expect("spawn gcx serve");
        let stdin = child.stdin.take().expect("client stdin");
        let stdout = child.stdout.take().expect("client stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            lines,
        }
    }

    fn send(&mut self, value: Value) {
        let stdin = self.stdin.as_mut().expect("open client stdin");
        writeln!(stdin, "{value}").expect("write MCP message");
        stdin.flush().expect("flush MCP message");
    }

    fn receive(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(15))
            .expect("receive MCP response");
        serde_json::from_str(&line).expect("valid MCP JSON")
    }

    fn initialize_and_list_tools(&mut self) -> Vec<String> {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "daemon-test", "version": "1"}
            }
        }));
        assert_eq!(self.receive()["id"], 1);
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }));
        self.receive()["result"]["tools"]
            .as_array()
            .expect("tool list")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name").to_owned())
            .collect()
    }

    fn close(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("wait for MCP proxy");
        assert!(status.success(), "MCP proxy failed: {status}");
    }
}

#[test]
fn compact_and_full_clients_share_one_repository_daemon() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let data = tempfile::tempdir().expect("data tempdir");
    let cache = tempfile::tempdir().expect("cache tempdir");
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["config", "user.email", "test@example.com"]);
    git(repo.path(), &["config", "user.name", "Test"]);
    std::fs::write(repo.path().join("main.rs"), "fn main() {}\n").expect("fixture");
    git(repo.path(), &["add", "main.rs"]);
    git(repo.path(), &["commit", "-qm", "initial"]);

    let init = gcx(repo.path(), data.path(), cache.path())
        .args(["init", "--editor", "none"])
        .output()
        .expect("gcx init");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let mut compact = McpClient::start(repo.path(), data.path(), cache.path(), false);
    let compact_tools = compact.initialize_and_list_tools();
    assert_eq!(compact_tools, ["gcx"]);

    let mut full = McpClient::start(repo.path(), data.path(), cache.path(), true);
    let full_tools = full.initialize_and_list_tools();
    assert!(full_tools.len() > 10);
    assert!(full_tools.iter().any(|name| name == "gcx"));

    let clean = gcx(repo.path(), data.path(), cache.path())
        .arg("clean")
        .output()
        .expect("gcx clean while active");
    assert!(!clean.status.success());
    assert!(String::from_utf8_lossy(&clean.stderr).contains("repository graph is active"));
    let preview = gcx(repo.path(), data.path(), cache.path())
        .args(["deinit", "--dry-run", "--purge"])
        .output()
        .expect("deinit dry run while active");
    assert!(
        preview.status.success(),
        "dry-run should not require graph ownership: {}",
        String::from_utf8_lossy(&preview.stderr)
    );

    // Disconnecting one editor must not disrupt another client sharing the DB.
    compact.close();
    full.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/list",
        "params": {}
    }));
    assert_eq!(
        full.receive()["result"]["tools"]
            .as_array()
            .expect("second tool list")
            .len(),
        full_tools.len()
    );
    full.close();

    // The daemon releases Kuzu promptly after the final proxy disconnects.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = gcx(repo.path(), data.path(), cache.path())
            .arg("status")
            .output()
            .expect("gcx status");
        if output.status.success() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "repository daemon retained Kuzu lock"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn gcx(repo: &Path, data: &Path, cache: &Path) -> Command {
    let mut command = Command::new(GCX);
    command
        .current_dir(repo)
        .env("GCX_STORE_PATH", data)
        .env("GCX_CACHE_PATH", cache)
        .env("GCX_DISABLE_SEMANTIC", "1");
    command
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git command");
    assert!(status.success(), "git {args:?} failed");
}
