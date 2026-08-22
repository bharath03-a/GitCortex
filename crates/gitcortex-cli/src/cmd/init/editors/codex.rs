use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::cmd::init::helpers::write_atomic;

pub(crate) const AGENTS_SECTION: &str = r#"<!-- >>> gitcortex codex integration >>> -->
## GitCortex knowledge graph

This repository is indexed by GitCortex. For structural questions, prefer these
over broad file scans — run them directly as shell commands, or via the `gcx`
MCP tool if `mcp_servers.gitcortex` is configured:

- `gcx query lookup-symbol <name>` locates a definition.
- `gcx query find-callers <name>` / `gcx query find-callees <name>` trace call relationships.
- `gcx query get-subgraph <name>` maps a symbol neighbourhood.
- `gcx query tour` summarizes unfamiliar areas.
- `gcx blast-radius --base <branch> --head <branch>` assesses change impact.

Treat graph output as navigation evidence and confirm behavior in source and
tests before editing. If the index is stale, run `gcx hook`.
<!-- <<< gitcortex codex integration <<< -->
"#;

pub(crate) const CODEX_MCP_SECTION: &str = r#"# >>> gitcortex codex MCP >>>
[mcp_servers.gitcortex]
command = "gcx"
args = ["serve"]
startup_timeout_sec = 30
# <<< gitcortex codex MCP <<<
"#;

pub fn install(repo_root: &Path, _global_editor_config: bool) -> Result<()> {
    write_agents_md(repo_root)?;
    write_codex_config(repo_root)?;
    Ok(())
}

fn write_agents_md(repo_root: &Path) -> Result<()> {
    let path = repo_root.join("AGENTS.md");

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read AGENTS.md")?;
        if existing.contains("gitcortex codex integration") {
            return Ok(());
        }
        if let Some(start) = existing.find("# GitCortex - Codex Guide") {
            let prefix = existing[..start].trim_end();
            let migrated = if prefix.is_empty() {
                AGENTS_SECTION.to_owned()
            } else {
                format!("{prefix}\n\n{AGENTS_SECTION}")
            };
            write_atomic(&path, &migrated).context("migrate AGENTS.md")?;
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}\n\n{AGENTS_SECTION}"))
            .context("update AGENTS.md")?;
    } else {
        fs::write(&path, AGENTS_SECTION).context("write AGENTS.md")?;
    }
    Ok(())
}

fn write_codex_config(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".codex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    const CODEX_MCP: &str = CODEX_MCP_SECTION;

    if path.exists() {
        let existing = fs::read_to_string(&path).context("read .codex/config.toml")?;
        if existing.contains("[mcp_servers.gitcortex]") {
            if existing.contains("args = [\"serve\", \"--compact\"]") {
                let migrated =
                    existing.replace("args = [\"serve\", \"--compact\"]", "args = [\"serve\"]");
                write_atomic(&path, &migrated).context("migrate .codex/config.toml")?;
            }
            return Ok(());
        }
        write_atomic(&path, &format!("{}\n\n{CODEX_MCP}", existing.trim_end()))
            .context("update .codex/config.toml")?;
    } else {
        fs::write(&path, CODEX_MCP).context("write .codex/config.toml")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_codex_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("AGENTS.md"),
            "# Existing\n\n# GitCortex - Codex Guide\nlegacy generated body\n",
        )
        .expect("write AGENTS");
        fs::create_dir(temp.path().join(".codex")).expect("create codex dir");
        fs::write(
            temp.path().join(".codex/config.toml"),
            "[mcp_servers.gitcortex]\ncommand = \"gcx\"\nargs = [\"serve\", \"--compact\"]\n",
        )
        .expect("write config");

        install(temp.path(), false).expect("install");
        let agents = fs::read_to_string(temp.path().join("AGENTS.md")).expect("read AGENTS");
        assert!(agents.contains("# Existing"));
        assert!(agents.contains("gitcortex codex integration"));
        assert!(!agents.contains("legacy generated body"));
        let config =
            fs::read_to_string(temp.path().join(".codex/config.toml")).expect("read config");
        assert!(config.contains("args = [\"serve\"]"));
        assert!(!config.contains("--compact"));
    }
}
