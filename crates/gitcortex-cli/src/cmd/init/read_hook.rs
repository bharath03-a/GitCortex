//! PreToolUse Read hook shared by Claude Code, Codex, and Antigravity. All
//! three accept `hookSpecificOutput.additionalContext`; `gcx __hook-context`
//! emits it and exits 0 so a hook can never block a read.

use std::{fs, path::Path};

use anyhow::{Context, Result};

pub const SCRIPT: &str = r#"#!/usr/bin/env sh
# GitCortex: adds call-graph context when the agent reads a source file.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"
command -v gcx >/dev/null 2>&1 || exit 0
# Exit 2 would block the read; an older gcx without this command exits 2.
gcx __hook-context 2>/dev/null || true
"#;

/// Earlier releases generated scripts calling `gcx query context`, a command
/// that never existed; current scripts call `gcx __hook-context`.
const MANAGED_MARKERS: &[&str] = &["gcx query context", "gcx __hook-context"];

/// Write or refresh the hook script unless the user replaced it with their own.
pub fn install(path: &Path) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        let managed = MANAGED_MARKERS
            .iter()
            .any(|marker| existing.contains(marker));
        if existing == SCRIPT || !managed {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, SCRIPT).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_cannot_block_a_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake_bin = dir.path().join("bin");
        fs::create_dir_all(&fake_bin).expect("bin");
        let fake_gcx = fake_bin.join("gcx");
        fs::write(
            &fake_gcx,
            "#!/bin/sh\necho 'unrecognized subcommand' >&2\nexit 2\n",
        )
        .expect("fake gcx");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake_gcx, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let script = dir.path().join("hook.sh");
        fs::write(&script, SCRIPT).expect("script");
        let output = std::process::Command::new("sh")
            .arg(&script)
            .env("HOME", dir.path())
            .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
            .output()
            .expect("run hook");
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn replaces_legacy_script_but_keeps_user_script() {
        let dir = tempfile::tempdir().expect("tempdir");
        let legacy = dir.path().join("legacy.sh");
        fs::write(&legacy, "#!/bin/sh\ngcx query context \"$f\" || true\n").expect("seed");
        install(&legacy).expect("install");
        assert_eq!(fs::read_to_string(&legacy).expect("read"), SCRIPT);

        let custom = dir.path().join("custom.sh");
        fs::write(&custom, "#!/bin/sh\necho mine\n").expect("seed");
        install(&custom).expect("install");
        assert_eq!(
            fs::read_to_string(&custom).expect("read"),
            "#!/bin/sh\necho mine\n"
        );
    }
}
