use std::time::Instant;

use anyhow::Result;

mod detect;
pub mod editors;
pub(crate) mod helpers;
pub(crate) mod universal;

use super::serve_lock;
use detect::parse_editor_flag;
use editors::{install_for_editor, EditorKind};
use helpers::repo_root;
use universal::{
    ensure_hooks_scope, initial_index, install_hooks, write_agent_guide, write_ci_workflow,
    write_gitcortex_ignore,
};

pub fn run(
    ci: bool,
    editor: Option<&str>,
    global_editor_config: bool,
    shared_git_hooks: bool,
) -> Result<()> {
    let repo_root = repo_root()?;
    if serve_lock::is_active(&repo_root)? {
        anyhow::bail!(
            "cannot initialise while `gcx serve` is active; stop the editor MCP server first"
        );
    }
    ensure_hooks_scope(&repo_root, shared_git_hooks)?;
    let start = Instant::now();

    let editors: Vec<EditorKind> = match editor {
        Some(flag) => parse_editor_flag(flag)?,
        None => Vec::new(),
    };

    // Write exclusions before the first index so build output is never scanned.
    write_gitcortex_ignore(&repo_root)?;
    let (nodes, edges) = initial_index(&repo_root)?;
    write_agent_guide(&repo_root)?;

    for ed in &editors {
        install_for_editor(ed, &repo_root, global_editor_config)?;
    }

    if ci {
        write_ci_workflow(&repo_root)?;
    }
    // Hooks are installed last: a failed index or editor setup must not leave
    // a repository running a partially configured hook on every Git action.
    let hooks = install_hooks(&repo_root, shared_git_hooks)?;

    let editor_names: Vec<&str> = editors.iter().map(|e| e.display_name()).collect();
    let ms = start.elapsed().as_millis();

    println!();
    println!("GitCortex initialised  ({ms}ms)");
    println!("  Graph:     {nodes} nodes | {edges} edges");
    println!("  Hooks:     {hooks} git hooks installed");
    if editor_names.is_empty() {
        println!("  Editors:   none (use `gcx init --editor <name>` to configure one)");
    } else {
        println!("  Editors:   {}", editor_names.join(", "));
    }
    if !global_editor_config
        && editors
            .iter()
            .any(|editor| matches!(editor, EditorKind::Windsurf | EditorKind::Antigravity))
    {
        println!(
            "  Note:      global MCP registration skipped; rerun with --global-editor-config to enable it"
        );
    }
    println!("  Universal: .gitcortex/AGENT_GUIDE.md, .gitcortex/ignore");
    if ci {
        println!("  CI:        .github/workflows/gcx-blast-radius.yml");
    }
    println!();

    Ok(())
}
