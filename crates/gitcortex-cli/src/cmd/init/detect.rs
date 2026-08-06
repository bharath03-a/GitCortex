use std::env;

use anyhow::{bail, Result};

use super::editors::EditorKind;

/// Detect active AI editors by inspecting environment variables. An empty
/// result is intentional: failing detection must never configure every editor.
pub fn detect_editors() -> Vec<EditorKind> {
    let mut detected = Vec::new();

    if env_prefix(&["CLAUDECODE", "CLAUDE_CODE_"]) {
        detected.push(EditorKind::ClaudeCode);
    }
    if env_prefix(&["CURSOR_TRACE_ID", "CURSOR_"]) {
        detected.push(EditorKind::Cursor);
    }
    if env_prefix(&["WINDSURF_", "CODEIUM_"]) {
        detected.push(EditorKind::Windsurf);
    }
    if env_prefix(&["GITHUB_COPILOT_"]) {
        detected.push(EditorKind::Copilot);
    }
    if env_prefix(&["ANTIGRAVITY_"]) {
        detected.push(EditorKind::Antigravity);
    }
    if env_prefix(&["CODEX_HOME", "CODEX_CLI_PATH", "CODEX_"]) {
        detected.push(EditorKind::Codex);
    }

    detected
}

/// Parse the `--editor` flag value into a list of EditorKind. Unknown values
/// are errors rather than permission to modify every supported editor.
pub fn parse_editor_flag(value: &str) -> Result<Vec<EditorKind>> {
    let editors = match value.to_ascii_lowercase().as_str() {
        "none" => Vec::new(),
        "auto" => detect_editors(),
        "all" => EditorKind::all(),
        "claude" | "claudecode" | "claude-code" => vec![EditorKind::ClaudeCode],
        "cursor" => vec![EditorKind::Cursor],
        "windsurf" => vec![EditorKind::Windsurf],
        "copilot" | "github-copilot" => vec![EditorKind::Copilot],
        "antigravity" => vec![EditorKind::Antigravity],
        "codex" | "openai-codex" => vec![EditorKind::Codex],
        other => bail!(
            "unknown editor '{other}'; expected one of: none, auto, claude, cursor, windsurf, copilot, antigravity, codex, all"
        ),
    };
    Ok(editors)
}

fn env_prefix(prefixes: &[&str]) -> bool {
    for (key, _) in env::vars() {
        for prefix in prefixes {
            if key == *prefix || key.starts_with(prefix) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_an_explicit_safe_choice() {
        assert!(parse_editor_flag("none").expect("parse none").is_empty());
    }

    #[test]
    fn unknown_editor_is_an_error() {
        assert!(parse_editor_flag("mystery").is_err());
    }

    #[test]
    fn all_remains_explicit() {
        assert_eq!(
            parse_editor_flag("all").expect("parse all").len(),
            EditorKind::all().len()
        );
    }
}
