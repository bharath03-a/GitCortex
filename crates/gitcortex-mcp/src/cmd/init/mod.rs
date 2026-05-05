use std::time::Instant;

use anyhow::Result;

mod detect;
pub mod editors;
mod helpers;
mod universal;

use detect::{detect_editors, parse_editor_flag};
use editors::{install_for_editor, EditorKind};
use helpers::repo_root;
use universal::{initial_index, install_hooks, write_agent_guide, write_ci_workflow};

pub fn run(ci: bool, editor: Option<&str>) -> Result<()> {
    let repo_root = repo_root()?;
    let start = Instant::now();

    let editors: Vec<EditorKind> = match editor {
        Some(flag) => parse_editor_flag(flag),
        None => detect_editors(),
    };

    let hooks = install_hooks(&repo_root)?;
    let (nodes, edges) = initial_index(&repo_root)?;
    write_agent_guide(&repo_root)?;

    for ed in &editors {
        install_for_editor(ed, &repo_root)?;
    }

    if ci {
        write_ci_workflow(&repo_root)?;
    }

    let editor_names: Vec<&str> = editors.iter().map(|e| e.display_name()).collect();
    let ms = start.elapsed().as_millis();

    println!();
    println!("GitCortex initialised  ({ms}ms)");
    println!("  Graph:     {nodes} nodes | {edges} edges");
    println!("  Hooks:     {hooks} git hooks installed");
    println!("  Editors:   {}", editor_names.join(", "));
    println!("  Universal: .gitcortex/AGENT_GUIDE.md");
    if ci {
        println!("  CI:        .github/workflows/gcx-blast-radius.yml");
    }
    println!();

    Ok(())
}
