//! Fixtures for store-backed unit tests. Stores live under the test data root
//! and are removed with the repository that owns them.
use std::{
    future::Future,
    path::Path,
    process::Command,
    sync::{Mutex, MutexGuard},
};

use gitcortex_store::branch::{data_dir, storage_repo_id};

/// Each open Kuzu DB reserves a large address-space range; serialize fixtures.
pub(crate) static STORE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock_stores() -> MutexGuard<'static, ()> {
    STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct TempRepo(tempfile::TempDir);

impl TempRepo {
    /// An empty directory at its canonical path, which store identity hashes.
    pub(crate) fn bare_tempdir() -> Self {
        let parent = std::env::temp_dir().canonicalize().expect("canonical tmp");
        Self(tempfile::tempdir_in(parent).expect("tempdir"))
    }

    pub(crate) fn init() -> Self {
        let repo = Self::bare_tempdir();
        for args in [
            &["init", "-b", "main"][..],
            &["config", "user.name", "Test"],
            &["config", "user.email", "test@example.test"],
            &["config", "core.hooksPath", "/dev/null"],
            &["config", "commit.gpgsign", "false"],
        ] {
            git(repo.path(), args);
        }
        repo
    }

    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(data_dir(&storage_repo_id(self.path())));
    }
}

pub(crate) fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git command");
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf8")
        .trim()
        .to_owned()
}

pub(crate) fn commit_file(root: &Path, rel: &str, contents: &str, msg: &str) -> String {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
    git(root, &["add", "-A"]);
    git(root, &["commit", "--allow-empty", "-m", msg]);
    git(root, &["rev-parse", "HEAD"])
}

/// Runs an async test holding the store lock outside the future.
pub(crate) fn run_async<F: Future>(test: F) -> F::Output {
    let _stores = lock_stores();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(test)
}
