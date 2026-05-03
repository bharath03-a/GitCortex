use std::env;

use super::editors::EditorKind;

/// Detect which AI editors are active by inspecting environment variables.
/// Returns all known editors if none are detected (idempotent install).
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

    if detected.is_empty() {
        EditorKind::all()
    } else {
        detected
    }
}

/// Parse the `--editor` flag value into a list of EditorKind.
pub fn parse_editor_flag(value: &str) -> Vec<EditorKind> {
    match value.to_ascii_lowercase().as_str() {
        "all" => EditorKind::all(),
        "claude" | "claudecode" | "claude-code" => vec![EditorKind::ClaudeCode],
        "cursor" => vec![EditorKind::Cursor],
        "windsurf" => vec![EditorKind::Windsurf],
        "copilot" | "github-copilot" => vec![EditorKind::Copilot],
        "antigravity" => vec![EditorKind::Antigravity],
        other => {
            eprintln!("warning: unknown editor '{other}', installing for all editors");
            EditorKind::all()
        }
    }
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
