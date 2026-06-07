//! Terminal colour + emphasis helpers.
//!
//! The palette mirrors the Cosmograph viz (`crates/gitcortex-viz` →
//! `kind_dot_color`) so a developer who has both the CLI and the browser
//! open sees the same colour for `struct`, `trait`, `function`, etc.
//!
//! Colouring is auto-disabled when stdout is not a TTY (e.g. piped to
//! `less`, `grep`, an editor), when `NO_COLOR` is set (the
//! [no-color.org](https://no-color.org) convention), when `CLICOLOR=0`,
//! or when `TERM=dumb`. The user can override with `gcx --color always|never`.

use std::io::IsTerminal;
use std::sync::OnceLock;

use anstyle::{AnsiColor, Color, Style};
use gitcortex_core::schema::NodeKind;

// ─── Per-NodeKind colour palette ──────────────────────────────────────────────
//
// Matched 1:1 against the Catppuccin Mocha hex codes used in the viz so
// terminal output and the in-browser graph look like one product.

fn kind_color(k: &NodeKind) -> Option<Color> {
    Some(Color::Ansi(match k {
        NodeKind::Struct => AnsiColor::Green,
        NodeKind::Enum => AnsiColor::Cyan,
        NodeKind::Trait => AnsiColor::Yellow,
        NodeKind::Interface => AnsiColor::BrightCyan,
        NodeKind::Function => AnsiColor::Blue,
        NodeKind::Method => AnsiColor::BrightBlue,
        NodeKind::Constant => AnsiColor::Yellow,
        NodeKind::TypeAlias => AnsiColor::Red,
        NodeKind::Module => AnsiColor::Magenta,
        NodeKind::Macro => AnsiColor::BrightWhite,
        NodeKind::Property => AnsiColor::Magenta,
        NodeKind::Annotation => AnsiColor::Red,
        NodeKind::EnumMember => AnsiColor::BrightGreen,
        // Structural nodes are dim — they're scaffolding, not the answer.
        NodeKind::File | NodeKind::Folder => AnsiColor::BrightBlack,
    }))
}

pub fn kind_style(k: &NodeKind) -> Style {
    Style::new().fg_color(kind_color(k))
}

/// Like [`kind_style`] but accepts the string form (`"struct"`, `"trait"`,
/// `"interface"`, …) used by `search::Hit` and the MCP JSON surface.
pub fn kind_style_from_str(k: &str) -> Style {
    let ansi = match k {
        "struct" => AnsiColor::Green,
        "enum" => AnsiColor::Cyan,
        "trait" => AnsiColor::Yellow,
        "interface" => AnsiColor::BrightCyan,
        "function" => AnsiColor::Blue,
        "method" => AnsiColor::BrightBlue,
        "constant" => AnsiColor::Yellow,
        "type_alias" => AnsiColor::Red,
        "module" => AnsiColor::Magenta,
        "macro" => AnsiColor::BrightWhite,
        "property" => AnsiColor::Magenta,
        "annotation" => AnsiColor::Red,
        "enum_member" => AnsiColor::BrightGreen,
        "file" | "folder" => AnsiColor::BrightBlack,
        _ => return Style::new(),
    };
    Style::new().fg_color(Some(Color::Ansi(ansi)))
}

// ─── Semantic styles ──────────────────────────────────────────────────────────

pub fn name_style() -> Style {
    Style::new().bold()
}

pub fn path_style() -> Style {
    Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)))
}

pub fn header_style() -> Style {
    Style::new().bold().underline()
}

pub fn arrow_style() -> Style {
    Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)))
}

pub fn hint_style() -> Style {
    Style::new()
        .fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)))
        .italic()
}

pub fn risk_style(level: &str) -> Style {
    let c = match level {
        "LOW" => AnsiColor::Green,
        "MEDIUM" => AnsiColor::Yellow,
        "HIGH" => AnsiColor::Red,
        "CRITICAL" => AnsiColor::BrightRed,
        _ => AnsiColor::BrightBlack,
    };
    Style::new().fg_color(Some(Color::Ansi(c))).bold()
}

pub fn score_style() -> Style {
    Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightYellow)))
}

// ─── Global enable/disable policy ─────────────────────────────────────────────

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ColorMode {
    /// Colour iff stdout is a TTY and no NO_COLOR / CLICOLOR=0 / TERM=dumb override.
    Auto,
    /// Always emit ANSI escapes, even when piped.
    Always,
    /// Never emit ANSI escapes.
    Never,
}

static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn init(mode: ColorMode) {
    let on = match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => detect_tty_color(),
    };
    // `init` may be called from tests more than once; ignore the second call.
    let _ = ENABLED.set(on);
}

fn detect_tty_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var("CLICOLOR").as_deref(), Ok("0")) {
        return false;
    }
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
        return false;
    }
    std::io::stdout().is_terminal()
}

#[inline]
pub fn enabled() -> bool {
    *ENABLED.get().unwrap_or(&false)
}

// ─── Paint helpers ────────────────────────────────────────────────────────────

/// Wrap `text` with the ANSI open + reset sequences of `style` if colouring
/// is enabled; otherwise return the text untouched. Uses anstyle's `Display`
/// impl: `{style}` writes the open sequence, `{style:#}` writes the reset.
pub fn paint(style: Style, text: &str) -> String {
    if enabled() {
        format!("{style}{text}{style:#}")
    } else {
        text.to_owned()
    }
}

// ─── High-level formatters used across query.rs ───────────────────────────────

use gitcortex_core::graph::Node;

/// Canonical one-line representation of a node:
///   `<bold name> <kind-colored (kind)>  <dim file>:<dim line>`
pub fn node_line(n: &Node) -> String {
    format!(
        "{} {}  {}{}{}",
        paint(name_style(), &n.name),
        paint(kind_style(&n.kind), &format!("({})", n.kind)),
        paint(path_style(), &n.file.display().to_string()),
        paint(path_style(), ":"),
        paint(path_style(), &n.span.start_line.to_string()),
    )
}

/// Like [`node_line`] with a leading indent string (preserves the prefix
/// untouched so callers can splice in tree-drawing chars later).
pub fn node_line_indented(n: &Node, prefix: &str) -> String {
    format!("{prefix}{}", node_line(n))
}

/// Format an arrow joining two nodes in a path (`trace-path`).
pub fn arrow() -> String {
    paint(arrow_style(), "→")
}
