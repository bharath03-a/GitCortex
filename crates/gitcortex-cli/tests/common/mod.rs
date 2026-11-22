#![allow(dead_code)]
use std::{
    path::Path,
    process::{Command, Output},
};

pub const GCX: &str = env!("CARGO_BIN_EXE_gcx");

pub struct Fixture {
    pub repo: tempfile::TempDir,
    pub data: tempfile::TempDir,
    pub home: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let f = Self {
            repo: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
        };
        f.git(&["init", "-b", "main"]);
        std::fs::write(f.repo.path().join("sample.rs"), "pub fn greet() {}\n").unwrap();
        std::fs::write(f.repo.path().join(".gitignore"), ".gitcortex/\n").unwrap();
        f.git(&["add", "."]);
        f.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@t",
            "commit",
            "-m",
            "fixture",
        ]);
        f
    }

    pub fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git")
            .args(args)
            .current_dir(self.repo.path())
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        String::from_utf8(o.stdout).unwrap().trim().into()
    }

    pub fn run_at(&self, root: &Path, args: &[&str]) -> Output {
        Command::new(GCX)
            .args(args)
            .current_dir(root)
            .env("GCX_STORE_PATH", self.data.path())
            .env("HOME", self.home.path())
            .env("NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.run_at(self.repo.path(), args)
    }

    pub fn init(&self) {
        let o = self.run(&["init", "--editor", "none"]);
        assert!(o.status.success(), "{o:?}");
    }
}

pub fn message(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
