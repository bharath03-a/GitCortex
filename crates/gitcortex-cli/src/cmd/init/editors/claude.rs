#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::cmd::init::helpers::{home_dir, require_json_object, write_atomic};

pub(crate) const CLAUDE_MD_SECTION: &str = r#"
<!-- >>> gitcortex claude integration >>> -->
## GitCortex Knowledge Graph

This repo is indexed by [GitCortex](https://github.com/bharath03-a/GitCortex).
Use the MCP server (`gcx serve`, configured in `.mcp.json`) or these slash commands:

- `/gcx-lookup <name>` — find all definitions matching a name
- `/gcx-callers <name>` — find all callers of a function
- `/gcx-file <path>` — list all definitions in a file
- `/gcx-blast-radius` — show blast radius of changes vs main
<!-- <<< gitcortex claude integration <<< -->
"#;

const PRE_TOOL_USE_HOOK: &str = r#"#!/usr/bin/env sh
# GitCortex PreToolUse hook — appends call-graph context when Claude reads a source file.
set -e
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

input=$(cat)

# Extract file_path from the JSON input (uses python3 for reliable JSON parsing)
file_path=$(printf '%s' "$input" | python3 -c \
  "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" \
  2>/dev/null || true)

[ -z "$file_path" ] && exit 0
command -v gcx >/dev/null 2>&1 || exit 0

# Silent — only prints when the file is indexed; ignored otherwise
gcx query context "$file_path" 2>/dev/null || true
"#;

/// (skill name, one-line description for frontmatter, body). Claude Code
/// discovers skills at `.claude/skills/<name>/SKILL.md` with YAML
/// frontmatter — a bare `.claude/skills/gcx/<name>.md` file (the previous
/// shape here) is not a recognized skill location and was never discovered.
pub(crate) const SKILLS: &[(&str, &str, &str)] = &[
    (
        "exploring",
        "Navigate unfamiliar code fast using the GitCortex knowledge graph",
        r#"Use the GitCortex knowledge graph to navigate unfamiliar parts of the codebase fast.

## Workflow

1. **Find a symbol** — `lookup_symbol` to locate any struct, function, or trait by name
2. **See a file's shape** — `list_definitions` on any file to get all definitions at a glance
3. **Trace callers** — `find_callers` to understand who calls a function and build a call chain
4. **Visualise** — run `gcx viz` to open the interactive graph in the browser

## When to use
- Starting a task in an unfamiliar module
- Understanding how a piece of code fits into the larger system
- Navigating a large codebase without reading every file
"#,
    ),
    (
        "debugging",
        "Trace bugs backward through the call chain using the GitCortex knowledge graph",
        r#"Trace bugs backward through the call chain using the knowledge graph.

## Workflow

1. **Locate the failing function** — `lookup_symbol` to find it and confirm the file/line
2. **Find direct callers** — `find_callers` to identify what triggered the bad code path
3. **Walk up the chain** — repeat `find_callers` on each caller to reach the entry point
4. **Check file context** — `list_definitions` on the relevant file to see surrounding code
"#,
    ),
    (
        "impact-analysis",
        "Understand everything that depends on a symbol before changing it",
        r#"Before modifying a function, struct, or trait — understand everything that depends on it.

## Workflow

1. **Look up the symbol** — `lookup_symbol(name: "YourSymbol")`
2. **Find direct callers** — `find_callers(function_name: "your_function")`
3. **Walk the blast radius** — repeat `find_callers` on each caller; stop when callers are entry points
4. **After changes** — run `gcx blast-radius --base main --head HEAD` for a full risk report
"#,
    ),
    (
        "refactoring",
        "Plan refactors in the right order using the dependency graph",
        r#"Use the knowledge graph to plan refactors in the right order and avoid breaking changes.

## Workflow

1. **Map current structure** — `list_definitions` on every file in the module being refactored
2. **Find all dependents** — `find_callers` and `lookup_symbol` to identify callers and uses
3. **Check trait implementations** — look for structs that implement traits you're changing
4. **Plan the order** — change leaf nodes first (no callers), then work toward roots
"#,
    ),
];

const SLASH_COMMANDS: &[(&str, &str)] = &[
    (
        "gcx-lookup.md",
        "Run `gcx query lookup-symbol $ARGUMENTS` and show the results. \
Display each match with its kind, qualified name, file path, and line number.",
    ),
    (
        "gcx-callers.md",
        "Run `gcx query find-callers $ARGUMENTS` and show the results. \
List every caller with its kind, name, file, and line number. Briefly describe the call chain.",
    ),
    (
        "gcx-file.md",
        "Run `gcx query list-definitions $ARGUMENTS` and show the results. \
Display all definitions ordered by line number with their kind, name, visibility, and location.",
    ),
    (
        "gcx-blast-radius.md",
        "Run `gcx blast-radius --base main --head HEAD --format text` and show the results. \
Summarise which functions changed and which callers are affected, and highlight the risk level.",
    ),
];

pub fn install(repo_root: &Path, global_editor_config: bool) -> Result<()> {
    write_project_mcp_json(repo_root)?;
    if global_editor_config {
        write_mcp_json()?;
    }
    write_slash_commands(repo_root)?;
    write_skills(repo_root)?;
    update_claude_md(repo_root)?;
    write_pre_tool_use_hook(repo_root)?;
    write_claude_settings(repo_root)?;
    Ok(())
}

fn write_project_mcp_json(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".mcp.json");
    let mut root: Value = if path.exists() {
        let text = fs::read_to_string(&path).context("read .mcp.json")?;
        serde_json::from_str(&text).context("parse existing .mcp.json")?
    } else {
        json!({})
    };
    require_json_object(&root, &path)?;
    if root.pointer("/mcpServers/gitcortex").is_none() {
        root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
        let text = serde_json::to_string_pretty(&root).context("serialize .mcp.json")?;
        write_atomic(&path, &text).context("write .mcp.json")?;
    }
    Ok(())
}

fn write_mcp_json() -> Result<()> {
    let path = home_dir().join(".claude.json");

    let mut root: Value = if path.exists() {
        let text = fs::read_to_string(&path).context("read ~/.claude.json")?;
        serde_json::from_str(&text).context("parse existing ~/.claude.json")?
    } else {
        json!({})
    };

    require_json_object(&root, &path)?;
    if root.pointer("/mcpServers/gitcortex").is_some() {
        return Ok(());
    }

    root["mcpServers"]["gitcortex"] = json!({ "command": "gcx", "args": ["serve"] });
    let text = serde_json::to_string_pretty(&root).context("serialize ~/.claude.json")?;
    write_atomic(&path, &text).context("write ~/.claude.json")?;
    Ok(())
}

fn write_slash_commands(repo_root: &Path) -> Result<usize> {
    let dir = repo_root.join(".claude").join("commands").join("gcx");
    fs::create_dir_all(&dir)?;
    let mut written = 0;
    for (filename, content) in SLASH_COMMANDS {
        let path = dir.join(filename);
        if !path.exists() {
            fs::write(&path, content).with_context(|| format!("write {filename}"))?;
            written += 1;
        }
    }
    Ok(written)
}

fn write_skills(repo_root: &Path) -> Result<usize> {
    let skills_dir = repo_root.join(".claude").join("skills");
    let mut written = 0;
    for (name, description, body) in SKILLS {
        let dir = skills_dir.join(name);
        fs::create_dir_all(&dir)?;
        let path = dir.join("SKILL.md");
        if !path.exists() {
            let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}");
            fs::write(&path, content).with_context(|| format!("write skill {name}"))?;
            written += 1;
        }
    }
    Ok(written)
}

fn update_claude_md(repo_root: &Path) -> Result<()> {
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir)?;
    let path = claude_dir.join("CLAUDE.md");

    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if existing.contains("GitCortex Knowledge Graph") {
            return Ok(());
        }
        write_atomic(&path, &format!("{existing}{CLAUDE_MD_SECTION}"))
            .context("update CLAUDE.md")?;
    } else {
        fs::write(&path, CLAUDE_MD_SECTION.trim_start()).context("write CLAUDE.md")?;
    }
    Ok(())
}

fn write_pre_tool_use_hook(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".claude").join("hooks").join("pre-tool-use");
    fs::create_dir_all(&dir)?;
    let path = dir.join("gcx-context.sh");
    if !path.exists() {
        fs::write(&path, PRE_TOOL_USE_HOOK).context("write gcx-context.sh")?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }
    }
    Ok(())
}

fn write_claude_settings(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".claude").join("settings.json");

    let mut root: Value = if path.exists() {
        let text = fs::read_to_string(&path).context("read .claude/settings.json")?;
        if text.contains("gcx-context.sh") {
            return Ok(());
        }
        serde_json::from_str(&text).context("parse existing .claude/settings.json")?
    } else {
        json!({})
    };

    require_json_object(&root, &path)?;
    add_gcx_hook_entry(&mut root);
    let text = serde_json::to_string_pretty(&root)?;
    write_atomic(&path, &text).context("write .claude/settings.json")?;
    Ok(())
}

fn add_gcx_hook_entry(root: &mut Value) {
    let hook = json!({
        "matcher": "Read",
        "hooks": [{ "type": "command", "command": ".claude/hooks/pre-tool-use/gcx-context.sh" }]
    });
    let arr = root["hooks"]["PreToolUse"].as_array_mut().map(|a| {
        a.push(hook.clone());
    });
    if arr.is_none() {
        root["hooks"]["PreToolUse"] = json!([hook]);
    }
}

#[cfg(test)]
mod tests {
    use super::write_skills;

    #[test]
    fn writes_skills_as_skill_md_per_directory() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_skills(dir.path()).unwrap();
        assert_eq!(written, 4);
        let path = dir.path().join(".claude/skills/exploring/SKILL.md");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\nname: exploring\n"));
        assert!(content.contains("description:"));
    }

    #[test]
    fn skills_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_skills(dir.path()).unwrap();
        let second = write_skills(dir.path()).unwrap();
        assert_eq!(second, 0);
    }
}
