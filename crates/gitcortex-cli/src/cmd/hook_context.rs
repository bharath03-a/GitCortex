//! `gcx __hook-context`: answer an editor's PreToolUse Read hook with the
//! file's graph outline. It only asks a running daemon (never opens Kuzu or
//! starts one) and always exits 0 so a hook cannot block or slow a read.

use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use serde_json::{json, Value};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;
const DAEMON_TIMEOUT: Duration = Duration::from_millis(1_500);
const MAX_NAMES: usize = 12;
const MAX_CONTEXT_CHARS: usize = 700;
const DEFINITION_KINDS: &[&str] = &[
    "function",
    "method",
    "struct",
    "enum",
    "trait",
    "class",
    "interface",
];

pub fn run() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .take(MAX_INPUT_BYTES)
        .read_to_string(&mut input)
        .ok();
    match context_reply(&input) {
        Ok(Some(reply)) => println!("{reply}"),
        Ok(None) => {}
        Err(error) => tracing::debug!("hook context skipped: {error:#}"),
    }
    Ok(())
}

fn context_reply(input: &str) -> Result<Option<String>> {
    let event: Value = serde_json::from_str(input).context("parse hook input")?;
    let Some(file) = read_target(&event) else {
        return Ok(None);
    };
    let cwd = event
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .map_or_else(std::env::current_dir, Ok)?;
    let repo_root = repo_root(&cwd)?;
    let Some(relative) = repo_relative(&repo_root, &cwd.join(file)) else {
        return Ok(None);
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let data = runtime.block_on(gitcortex_mcp::mcp::server::daemon_call(
        &repo_root,
        "list_definitions",
        json!({ "file": relative }),
        DAEMON_TIMEOUT,
    ))?;
    Ok(outline_text(&relative, &data).map(|text| hook_output(&text).to_string()))
}

fn read_target(event: &Value) -> Option<&str> {
    let input = event.get("tool_input")?;
    ["file_path", "path", "filePath"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .filter(|path| !path.is_empty())
}

fn repo_root(cwd: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .context("run git rev-parse")?;
    anyhow::ensure!(output.status.success(), "not inside a git repository");
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn repo_relative(repo_root: &Path, file: &Path) -> Option<String> {
    let root = repo_root.canonicalize().ok()?;
    let file = file.canonicalize().ok()?;
    let relative = file.strip_prefix(&root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

/// Only a `ready` or `unverified` graph is worth injecting; silence otherwise.
fn outline_text(file: &str, data: &Value) -> Option<String> {
    let state = data.pointer("/index/state").and_then(Value::as_str)?;
    if !matches!(state, "ready" | "unverified") {
        return None;
    }
    let items = data.get("items").and_then(Value::as_array)?;
    let names: Vec<&str> = items
        .iter()
        .filter(|item| {
            item.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| DEFINITION_KINDS.contains(&kind.to_lowercase().as_str()))
        })
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .collect();
    if names.is_empty() {
        return None;
    }
    let shown = names[..names.len().min(MAX_NAMES)].join(", ");
    let more = names.len().saturating_sub(MAX_NAMES);
    let more = if more > 0 {
        format!(" (+{more} more)")
    } else {
        String::new()
    };
    let caveat = if state == "unverified" {
        " Local edits may be missing from the graph."
    } else {
        ""
    };
    let text = format!(
        "GitCortex: {file} defines {shown}{more}. Before changing one, run gcx pre_edit_impact \
        (who calls it, what breaks); use find_callers or trace_path for cross-file questions.{caveat}"
    );
    Some(text.chars().take(MAX_CONTEXT_CHARS).collect())
}

fn hook_output(text: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": text,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(state: &str, kinds: &[(&str, &str)]) -> Value {
        json!({
            "index": {"state": state},
            "items": kinds.iter().map(|(kind, name)| json!({"kind": kind, "name": name})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn outline_names_definitions_and_a_real_action() {
        let text = outline_text(
            "src/lib.rs",
            &data("ready", &[("Function", "parse"), ("Constant", "MAX")]),
        )
        .expect("text");
        assert!(text.contains("src/lib.rs defines parse."), "{text}");
        assert!(!text.contains("MAX"));
        assert!(text.contains("pre_edit_impact"));
    }

    #[test]
    fn outline_is_silent_when_the_graph_cannot_vouch() {
        for state in ["missing", "stale", "indexing", "unavailable"] {
            assert!(outline_text("a.rs", &data(state, &[("Function", "f")])).is_none());
        }
        assert!(outline_text("a.rs", &data("ready", &[])).is_none());
    }

    #[test]
    fn outline_is_bounded() {
        let many: Vec<(String, String)> = (0..200)
            .map(|i| ("Function".to_owned(), format!("function_number_{i}")))
            .collect();
        let pairs: Vec<(&str, &str)> = many.iter().map(|(k, n)| (k.as_str(), n.as_str())).collect();
        let text = outline_text("a.rs", &data("ready", &pairs)).expect("text");
        assert!(text.chars().count() <= MAX_CONTEXT_CHARS);
        assert!(text.contains("(+188 more)"));
    }

    #[test]
    fn hook_output_uses_the_pre_tool_use_context_contract() {
        let output = hook_output("ctx");
        assert_eq!(output["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(output["hookSpecificOutput"]["additionalContext"], "ctx");
        assert!(output.get("additionalContext").is_none());
        assert!(output["hookSpecificOutput"]
            .get("permissionDecision")
            .is_none());
    }

    #[test]
    fn non_read_events_are_ignored() {
        assert!(context_reply(r#"{"tool_input":{"command":"ls"}}"#)
            .expect("ok")
            .is_none());
        assert!(context_reply("not json").is_err());
    }
}
