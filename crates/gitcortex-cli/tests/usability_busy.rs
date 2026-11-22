mod common;
use common::*;

use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_viz(f: &Fixture, port: u16) -> (std::process::Child, String) {
    let mut child = Command::new(GCX)
        .args(["viz", "--port", &port.to_string()])
        .current_dir(f.repo.path())
        .env("GCX_STORE_PATH", f.data.path())
        .env("HOME", f.home.path())
        .env("NO_COLOR", "1")
        .env("GCX_NO_BROWSER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn gcx viz");

    let stderr = child.stderr.take().expect("viz stderr");
    let mut reader = BufReader::new(stderr);
    let mut banner = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                banner.push_str(&line);
                if line.contains("GitCortex Viz") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (child, banner)
}

#[test]
fn busy_reader_names_owner_and_retry() {
    let f = Fixture::new();
    f.init();

    // Baseline clean counts before contention.
    let before = f.run(&["status"]);
    assert!(before.status.success(), "{}", message(&before));
    let before_msg = message(&before);

    // Case 1: a real child process holds the isolated repository lock with a
    // `graph operation` owner and waits on a stdin barrier.
    let canonical_repo =
        std::fs::canonicalize(f.repo.path()).unwrap_or_else(|_| f.repo.path().to_path_buf());
    let repo_id = gitcortex_store::branch::repo_id(&canonical_repo);
    let lock_path = f.data.path().join(&repo_id).join("serve.lock");
    assert!(
        lock_path.exists(),
        "expected isolated serve.lock at {}",
        lock_path.display()
    );

    let mut holder = Command::new("python3")
        .args([
            "-c",
            "import fcntl, os, sys\n\
             p = sys.argv[1]\n\
             f = open(p, 'r+')\n\
             fcntl.flock(f, fcntl.LOCK_EX)\n\
             f.seek(0); f.truncate()\n\
             f.write(f'pid {os.getpid()} (graph operation)')\n\
             f.flush()\n\
             print('LOCKED', flush=True)\n\
             sys.stdin.read()\n",
            lock_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lock holder");
    let holder_pid = holder.id();
    let mut holder_out = BufReader::new(holder.stdout.take().unwrap());
    let mut ready_line = String::new();
    holder_out.read_line(&mut ready_line).unwrap();
    assert_eq!(ready_line.trim(), "LOCKED", "lock holder failed to lock");
    let expected_owner = format!("pid {holder_pid} (graph operation)");

    let start = Instant::now();
    let busy = f.run(&["status"]);
    let elapsed = start.elapsed();

    // Release the barrier and wait for the child before asserting so we never
    // leak a child process on failure.
    drop(holder.stdin.take());
    let _ = holder.wait();

    assert!(!busy.status.success(), "must fail while lock is held");
    assert!(
        elapsed < Duration::from_secs(5),
        "busy check took too long: {elapsed:?}"
    );
    let busy_msg = message(&busy);
    assert!(
        busy_msg.contains(&expected_owner),
        "expected owner '{expected_owner}' in:\n{busy_msg}"
    );
    assert!(
        busy_msg.contains("retry after current command completes"),
        "expected retry guidance in:\n{busy_msg}"
    );

    // Next read after release must succeed with identical counts.
    let after = f.run(&["status"]);
    assert!(after.status.success(), "{}", message(&after));
    assert_eq!(message(&after), before_msg);

    // Case 2: live `gcx viz` process holds the lock with owner `(viz)` and Ctrl-C advice.
    let port = free_port();
    let (mut viz, banner) = spawn_viz(&f, port);
    assert!(
        banner.contains("GitCortex Viz"),
        "viz failed to start:\n{banner}"
    );
    let viz_pid = viz.id();

    let start = Instant::now();
    let busy_viz = f.run(&["status"]);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "viz busy check took too long"
    );
    let busy_viz_msg = message(&busy_viz);
    let _ = viz.kill();
    let _ = viz.wait();

    assert!(!busy_viz.status.success(), "must fail while viz holds lock");
    assert!(
        busy_viz_msg.contains(&format!("pid {viz_pid} (viz)")),
        "expected viz owner pid {viz_pid} in:\n{busy_viz_msg}"
    );
    assert!(
        busy_viz_msg.contains("Ctrl-C"),
        "expected Ctrl-C advice for viz owner in:\n{busy_viz_msg}"
    );

    let final_status = f.run(&["status"]);
    assert!(final_status.status.success(), "{}", message(&final_status));
    assert_eq!(message(&final_status), before_msg);
}

#[test]
fn two_readers_never_corrupt_store() {
    let f = Fixture::new();
    f.init();

    let baseline = f.run(&["status"]);
    assert!(baseline.status.success(), "{}", message(&baseline));
    let baseline_msg = message(&baseline);

    let c1 = Command::new(GCX)
        .args(["query", "symbol-context", "greet"])
        .current_dir(f.repo.path())
        .env("GCX_STORE_PATH", f.data.path())
        .env("HOME", f.home.path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn reader 1");

    let c2 = Command::new(GCX)
        .args(["query", "symbol-context", "greet"])
        .current_dir(f.repo.path())
        .env("GCX_STORE_PATH", f.data.path())
        .env("HOME", f.home.path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn reader 2");

    let o1 = c1.wait_with_output().unwrap();
    let o2 = c2.wait_with_output().unwrap();
    // At least one reader must succeed; any contending reader must fail cleanly
    // with a busy error rather than corrupting the database.
    assert!(
        o1.status.success() || o2.status.success(),
        "at least one concurrent reader must succeed:\no1: {}\no2: {}",
        message(&o1),
        message(&o2)
    );
    for out in [&o1, &o2] {
        if !out.status.success() {
            assert!(
                message(out).contains("busy"),
                "contending reader failed with non-busy error:\n{}",
                message(out)
            );
        }
    }

    let after = f.run(&["status"]);
    assert!(after.status.success(), "{}", message(&after));
    assert_eq!(message(&after), baseline_msg);
}

#[test]
fn viz_port_collision_suggests_alternate_port() {
    let f = Fixture::new();
    f.init();

    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();

    let o = Command::new(GCX)
        .args(["viz", "--port", &port.to_string()])
        .current_dir(f.repo.path())
        .env("GCX_STORE_PATH", f.data.path())
        .env("HOME", f.home.path())
        .env("NO_COLOR", "1")
        .env("GCX_NO_BROWSER", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    drop(occupied);
    let m = message(&o);
    assert!(!o.status.success(), "must fail on occupied port:\n{m}");
    assert!(
        m.contains(&port.to_string()),
        "error must name occupied port {port}:\n{m}"
    );
    assert!(
        m.contains("gcx viz --port "),
        "error must suggest concrete alternate `gcx viz --port <free-port>`:\n{m}"
    );
}

#[test]
fn viz_startup_explains_lifetime() {
    let f = Fixture::new();
    f.init();

    let port = free_port();
    let (mut viz, banner) = spawn_viz(&f, port);
    let _ = viz.kill();
    let _ = viz.wait();

    assert!(
        banner.contains(&format!("http://127.0.0.1:{port}")),
        "startup must print URL:\n{banner}"
    );
    assert!(
        banner.contains("Ctrl-C"),
        "startup must explain Ctrl-C stop:\n{banner}"
    );
    assert!(
        banner.contains("busy") || banner.contains("lock"),
        "startup must explain standalone CLI lock contention:\n{banner}"
    );
}
