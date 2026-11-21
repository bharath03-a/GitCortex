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

    // Doctor must interrogate the live graph owner, not infer freshness from
    // its lock. Startup reconciliation can briefly report degraded readiness.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let doctor = gcx(repo.path(), data.path(), cache.path())
            .arg("doctor")
            .output()
            .expect("doctor while active");
        if doctor.status.success() {
            assert!(String::from_utf8_lossy(&doctor.stderr)
                .contains("active MCP server verified index readiness"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{}",
            String::from_utf8_lossy(&doctor.stderr)
        );
        std::thread::sleep(Duration::from_millis(100));
    }

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

/// Hold `GCX_STORE_PATH` steady for in-process lock holders in this binary.
static STORE_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Fixture {
    repo: tempfile::TempDir,
    data: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Fixture {
    fn indexed() -> Self {
        let fixture = Self {
            repo: tempfile::tempdir().expect("repo tempdir"),
            data: tempfile::tempdir().expect("data tempdir"),
            cache: tempfile::tempdir().expect("cache tempdir"),
        };
        let repo = fixture.repo.path();
        git(repo, &["init", "-q"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("main.rs"), "fn main() {}\n").expect("fixture");
        git(repo, &["add", "main.rs"]);
        git(repo, &["commit", "-qm", "initial"]);
        let init = fixture
            .gcx()
            .args(["init", "--editor", "none"])
            .output()
            .expect("gcx init");
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        fixture
    }

    fn gcx(&self) -> Command {
        gcx(self.repo.path(), self.data.path(), self.cache.path())
    }

    fn client(&self) -> McpClient {
        McpClient::start(self.repo.path(), self.data.path(), self.cache.path(), false)
    }

    /// The single per-repository data directory `gcx init` created.
    fn repo_data_dir(&self) -> std::path::PathBuf {
        std::fs::read_dir(self.data.path())
            .expect("data dir")
            .map(|entry| entry.expect("entry").path())
            .find(|path| path.is_dir())
            .expect("repository data dir")
    }

    fn daemon_pid(&self) -> Option<String> {
        let owner = std::fs::read_to_string(self.repo_data_dir().join("serve.lock")).ok()?;
        let pid = owner.strip_prefix("pid ")?.split_whitespace().next()?;
        owner.contains("repository daemon").then(|| pid.to_owned())
    }
}

#[test]
fn client_connect_survives_daemon_restart() {
    let fixture = Fixture::indexed();
    let mut first = fixture.client();
    assert_eq!(first.initialize_and_list_tools(), ["gcx"]);

    // A crashed daemon leaves its socket file behind; the next client must
    // replace it rather than connect to a dead endpoint.
    let pid = fixture.daemon_pid().expect("daemon pid");
    assert!(Command::new("kill")
        .args(["-9", &pid])
        .status()
        .expect("kill daemon")
        .success());
    drop(first.stdin.take());
    let _ = first.child.wait();

    let mut restarted = fixture.client();
    assert_eq!(restarted.initialize_and_list_tools(), ["gcx"]);
    restarted.close();

    // Reconnect around the daemon's idle-exit deadline, when an exiting
    // daemon can drop a freshly queued connection.
    for delay_ms in [0, 400, 450, 480, 500, 520, 550, 600, 700, 900] {
        std::thread::sleep(Duration::from_millis(delay_ms));
        let mut client = fixture.client();
        assert_eq!(
            client.initialize_and_list_tools(),
            ["gcx"],
            "reconnect {delay_ms}ms after the previous client left"
        );
        client.close();
    }
}

#[test]
fn client_connect_waits_for_a_hook_holding_the_graph() {
    let fixture = Fixture::indexed();
    let _env = STORE_ENV
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    std::env::set_var("GCX_STORE_PATH", fixture.data.path());
    let repo = fixture.repo.path().canonicalize().expect("canonical repo");
    let mut lock = gitcortex_store::branch::RepositoryLock::try_acquire(&repo)
        .expect("lock")
        .expect("lock is free");
    lock.set_owner("pid 1 (graph operation)").expect("owner");
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1_500));
        drop(lock);
    });

    let mut client = fixture.client();
    assert_eq!(client.initialize_and_list_tools(), ["gcx"]);
    client.close();
    holder.join().expect("lock holder");
}

#[test]
fn client_gets_actionable_error_when_daemon_cannot_start() {
    let fixture = Fixture::indexed();
    std::fs::write(fixture.repo_data_dir().join("schema_version"), "1").expect("stale schema");

    let started = Instant::now();
    let mut client = fixture.client();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "daemon-test", "version": "1"}
        }
    }));
    let reply = client.receive();
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "startup failure took {:?}",
        started.elapsed()
    );
    assert_eq!(reply["id"], 1);
    let message = reply["error"]["message"].as_str().expect("error message");
    assert!(message.contains("schema version"), "{message}");
    assert!(message.contains("gcx doctor"), "{message}");
    drop(client.stdin.take());
    assert!(!client.child.wait().expect("proxy exit").success());
}

/// Run one generated hook script the way an editor does: hook JSON on stdin,
/// repo as cwd, and only the test binary on PATH as `gcx`.
fn run_hook(fixture: &Fixture, home: &Path, script: &str, event: &Value) -> std::process::Output {
    let gcx_dir = Path::new(GCX).parent().expect("gcx dir");
    let path = format!("{}:/usr/bin:/bin", gcx_dir.display());
    let mut child = Command::new("sh")
        .arg(script)
        .current_dir(fixture.repo.path())
        .env("HOME", home)
        .env("PATH", path)
        .env("GCX_STORE_PATH", fixture.data.path())
        .env("GCX_CACHE_PATH", fixture.cache.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");
    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(event.to_string().as_bytes())
        .expect("write hook input");
    child.wait_with_output().expect("hook output")
}

#[test]
fn generated_hooks_run_successfully() {
    let fixture = Fixture::indexed();
    let home = tempfile::tempdir().expect("home");
    let init = fixture
        .gcx()
        .env("HOME", home.path())
        .args(["init", "--editor", "all"])
        .output()
        .expect("gcx init --editor all");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let scripts = [
        ".claude/hooks/pre-tool-use/gcx-context.sh",
        ".codex/hooks/gcx-context.sh",
        ".agents/plugins/gitcortex/gcx-context.sh",
    ];
    let repo = fixture.repo.path();
    let settings = std::fs::read_to_string(repo.join(".claude/settings.json")).expect("settings");
    assert!(settings.contains(scripts[0]), "{settings}");
    let codex = std::fs::read_to_string(repo.join(".codex/hooks.json")).expect("codex hooks");
    assert!(codex.contains(scripts[1]), "{codex}");
    for script in scripts {
        let text = std::fs::read_to_string(repo.join(script)).expect("script");
        assert!(
            !text.contains("gcx query context"),
            "{script} is the broken hook"
        );
    }

    let main_rs = repo.join("main.rs").canonicalize().expect("main.rs");
    let event = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Read",
        "cwd": repo,
        "tool_input": {"file_path": main_rs},
    });
    // No running daemon: the hook stays silent and never blocks the read.
    for script in scripts {
        let output = run_hook(&fixture, home.path(), script, &event);
        assert!(output.status.success(), "{script}: {output:?}");
        assert!(output.stdout.is_empty(), "{script}: {output:?}");
    }

    let mut client = fixture.client();
    client.initialize_and_list_tools();
    for script in scripts {
        let deadline = Instant::now() + Duration::from_secs(10);
        let reply = loop {
            let output = run_hook(&fixture, home.path(), script, &event);
            assert!(output.status.success(), "{script}: {output:?}");
            if !output.stdout.is_empty() {
                break serde_json::from_slice::<Value>(&output.stdout).expect("hook JSON");
            }
            assert!(Instant::now() < deadline, "{script} never produced context");
            std::thread::sleep(Duration::from_millis(200));
        };
        assert_eq!(reply["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        let context = reply["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("additionalContext");
        assert!(context.contains("main.rs defines main"), "{context}");
        assert!(context.contains("pre_edit_impact"), "{context}");
    }
    client.close();
}
