use std::path::Path;

use anyhow::Result;

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod windsurf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorKind {
    ClaudeCode,
    Cursor,
    Windsurf,
    Copilot,
    Antigravity,
    Codex,
}

impl EditorKind {
    pub fn all() -> Vec<Self> {
        vec![
            Self::ClaudeCode,
            Self::Cursor,
            Self::Windsurf,
            Self::Copilot,
            Self::Antigravity,
            Self::Codex,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
            Self::Copilot => "Copilot",
            Self::Antigravity => "Antigravity",
            Self::Codex => "Codex",
        }
    }
}

pub fn install_for_editor(editor: &EditorKind, repo_root: &Path) -> Result<()> {
    match editor {
        EditorKind::ClaudeCode => claude::install(repo_root),
        EditorKind::Cursor => cursor::install(repo_root),
        EditorKind::Windsurf => windsurf::install(repo_root),
        EditorKind::Copilot => copilot::install(repo_root),
        EditorKind::Antigravity => antigravity::install(repo_root),
        EditorKind::Codex => codex::install(repo_root),
    }
}
