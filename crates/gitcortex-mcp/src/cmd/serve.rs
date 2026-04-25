use anyhow::{Context, Result};

pub fn run() -> Result<()> {
    let repo_root = {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("git rev-parse failed")?;
        std::path::PathBuf::from(String::from_utf8(out.stdout)?.trim().to_owned())
    };

    let rt = tokio::runtime::Runtime::new().context("failed to build tokio runtime")?;
    rt.block_on(crate::mcp::server::serve(repo_root))
}
