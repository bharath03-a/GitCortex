use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::Result;
use dialoguer::{
    theme::{ColorfulTheme, SimpleTheme, Theme},
    MultiSelect,
};
use gitcortex_store::branch;
use indicatif::{ProgressBar, ProgressStyle};

mod detect;
pub mod editors;
pub(crate) mod helpers;
pub(crate) mod universal;

use super::serve_lock;
use crate::style;
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
            "cannot initialise while the repository graph is active; close editor MCP sessions and stop `gcx viz`, then retry"
        );
    }
    ensure_hooks_scope(&repo_root, shared_git_hooks)?;
    let repo_id = branch::storage_repo_id(&repo_root);
    let start = Instant::now();

    print_banner();

    let already_prompted = branch::has_prompted_for_editor(&repo_id);
    let (editors, should_mark_prompted): (Vec<EditorKind>, bool) = match editor {
        Some(flag) => (parse_editor_flag(flag)?, false),
        // No --editor given: ask interactively, but only once ever per repo
        // (the choice — including "none" — is remembered) and only when
        // there's a human to ask; CI and piped invocations stay non-interactive.
        None if !already_prompted
            && std::io::stdin().is_terminal()
            && std::io::stdout().is_terminal() =>
        {
            (prompt_editor_choice().unwrap_or_default(), true)
        }
        None => (Vec::new(), false),
    };

    // Write exclusions before the first index so build output is never scanned.
    write_gitcortex_ignore(&repo_root)?;
    let spinner = start_spinner("Indexing repository…");
    let (nodes, edges) = initial_index(&repo_root)?;
    finish_spinner(spinner, "Repository indexed");
    // Marked after indexing, not before: an incompatible on-disk schema
    // makes indexing wipe this repo's whole data directory, which would
    // silently erase an earlier mark and defeat the "only ask once" guarantee.
    if should_mark_prompted {
        branch::mark_editor_prompted(&repo_id)?;
    }
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
    println!(
        "{} GitCortex initialised  {}",
        style::paint(style::success_style(), "✔"),
        style::paint(style::hint_style(), &format!("({ms}ms)"))
    );
    print_field("Graph", &format!("{nodes} nodes | {edges} edges"));
    print_field("Hooks", &format!("{hooks} git hooks installed"));
    if editor_names.is_empty() {
        print_field(
            "Editors",
            "none (use `gcx init --editor <name>` to configure one)",
        );
    } else {
        print_field("Editors", &editor_names.join(", "));
    }
    if !global_editor_config
        && editors
            .iter()
            .any(|editor| matches!(editor, EditorKind::Windsurf))
    {
        print_field(
            "Note",
            "global MCP registration skipped; rerun with --global-editor-config to enable it",
        );
    }
    print_field("Universal", ".gitcortex/AGENT_GUIDE.md, .gitcortex/ignore");
    if ci {
        print_field("CI", ".github/workflows/gcx-blast-radius.yml");
    }
    println!();

    Ok(())
}

// Classic 5×7 LED dot-matrix glyphs, one letter per row group
// ('#' = lit, ' ' = off) — enough resolution for the X's diagonals to
// actually read as an X instead of a scatter of dots.
const GLYPH_G: [&str; 7] = [
    " ### ", "#   #", "#    ", "#  ##", "#   #", "#   #", " ### ",
];
const GLYPH_C: [&str; 7] = [
    " ####", "#    ", "#    ", "#    ", "#    ", "#    ", " ####",
];
const GLYPH_X: [&str; 7] = [
    "#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #",
];
// Double-width solid block per lit pixel — bolder and wider than a round
// dot or a single block, closer to how neofetch-style splash logos
// (Ubuntu, R, Moonshot) render block-letter wordmarks.
const BLOCK: &str = "██";
const BLANK: &str = "  ";

// Unicode bracket-extension pieces, stacked to build a tall `[` / `]` that
// matches the glyph height — mirrors the project's [GCX] wordmark.
const LEFT_BRACKET: [&str; 7] = ["⎡", "⎢", "⎢", "⎢", "⎢", "⎢", "⎣"];
const RIGHT_BRACKET: [&str; 7] = ["⎤", "⎥", "⎥", "⎥", "⎥", "⎥", "⎦"];

/// Prints the `[GCX]` wordmark in a single brand orange (`--brand:
/// #C4633F`, the same hex used across the GitHub Pages site and the
/// README logo) so the CLI and the web presence read as one identity.
/// Falls back to a single compact line outside a real terminal so piped
/// output and CI logs stay quiet.
fn print_banner() {
    if !std::io::stdout().is_terminal() {
        println!(
            "{} {}",
            style::paint(style::brand_style(), "gcx"),
            style::paint(style::hint_style(), "GitCortex knowledge graph")
        );
        return;
    }

    // Whole mark is one brand orange — matching the README logo, which is
    // also uniformly #C4633F (icon and text), not a mix of colors.
    let brand = style::brand_style();

    println!();
    for row in 0..7 {
        let l = style::paint(brand, LEFT_BRACKET[row]);
        let g = render_glyph_row(GLYPH_G[row], brand);
        let c = render_glyph_row(GLYPH_C[row], brand);
        let x = render_glyph_row(GLYPH_X[row], brand);
        let r = style::paint(brand, RIGHT_BRACKET[row]);
        println!("      {l}    {g}    {c}    {x}    {r}");
    }
    println!();
    println!(
        "        {}",
        style::paint(style::hint_style(), "GitCortex knowledge graph")
    );
    println!();
}

fn render_glyph_row(row: &str, glyph_style: anstyle::Style) -> String {
    let painted: String = row
        .chars()
        .map(|c| if c == '#' { BLOCK } else { BLANK })
        .collect();
    style::paint(glyph_style, &painted)
}

fn print_field(label: &str, value: &str) {
    println!(
        "  {}{}",
        style::paint(style::label_style(), &format!("{label:<10} ")),
        value
    );
}

/// A spinner while the initial index runs — this step alone can take
/// several seconds on a large repository, and a silent terminal during
/// that time reads as a hang. Suppressed outside a real terminal (CI logs,
/// piped output) so it never leaves stray control codes in captured text.
fn start_spinner(message: &str) -> Option<ProgressBar> {
    if !std::io::stderr().is_terminal() {
        return None;
    }
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(Duration::from_millis(80));
    // Match the CLI's own --color/NO_COLOR policy — indicatif has no idea
    // about it otherwise and would colour the spinner unconditionally.
    let template = if style::enabled() {
        "{spinner:.cyan} {msg}"
    } else {
        "{spinner} {msg}"
    };
    bar.set_style(
        ProgressStyle::with_template(template).unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    bar.set_message(message.to_owned());
    Some(bar)
}

fn finish_spinner(bar: Option<ProgressBar>, message: &str) {
    if let Some(bar) = bar {
        bar.finish_and_clear();
        println!("{} {message}", style::paint(style::success_style(), "✔"));
    }
}

/// Interactive fallback when `gcx init` runs with no `--editor` flag in a
/// real terminal: a checkbox menu (space to toggle, enter to confirm) since
/// people commonly use more than one assistant (e.g. Claude Code and
/// Antigravity together) — a single-choice picker couldn't express that.
/// Nothing toggled, Esc/Ctrl-C, or a read error all fall back to no editor
/// configured, matching the pre-existing non-interactive default rather
/// than failing `init` outright.
fn prompt_editor_choice() -> Result<Vec<EditorKind>> {
    const CHOICES: &[(&str, &str)] = &[
        ("Claude Code", "claude"),
        ("Cursor", "cursor"),
        ("Windsurf", "windsurf"),
        ("GitHub Copilot", "copilot"),
        ("Antigravity", "antigravity"),
        ("Codex", "codex"),
    ];
    let labels: Vec<&str> = CHOICES.iter().map(|(label, _)| *label).collect();

    // Respect the same --color/NO_COLOR policy as the rest of the CLI
    // instead of always emitting dialoguer's default bright theme.
    let colorful = ColorfulTheme::default();
    let plain = SimpleTheme;
    let theme: &dyn Theme = if style::enabled() { &colorful } else { &plain };

    let selection = MultiSelect::with_theme(theme)
        .with_prompt("Which AI assistant(s) do you use? (space to toggle, enter to confirm)")
        .items(&labels)
        .interact_opt();

    let indices = match selection {
        Ok(Some(indices)) => indices,
        Ok(None) | Err(_) => return Ok(Vec::new()),
    };

    let mut editors = Vec::new();
    for index in indices {
        editors.extend(parse_editor_flag(CHOICES[index].1)?);
    }
    Ok(editors)
}
