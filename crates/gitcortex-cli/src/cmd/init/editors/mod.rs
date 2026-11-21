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

pub(crate) fn editor_flag(editor: &EditorKind) -> &'static str {
    match editor {
        EditorKind::ClaudeCode => "claude",
        EditorKind::Cursor => "cursor",
        EditorKind::Windsurf => "windsurf",
        EditorKind::Copilot => "copilot",
        EditorKind::Antigravity => "antigravity",
        EditorKind::Codex => "codex",
    }
}

pub fn install_for_editor(
    editor: &EditorKind,
    repo_root: &Path,
    global_editor_config: bool,
) -> Result<()> {
    match editor {
        EditorKind::ClaudeCode => claude::install(repo_root, global_editor_config),
        EditorKind::Cursor => cursor::install(repo_root, global_editor_config),
        EditorKind::Windsurf => windsurf::install(repo_root, global_editor_config),
        EditorKind::Copilot => copilot::install(repo_root, global_editor_config),
        EditorKind::Antigravity => antigravity::install(repo_root, global_editor_config),
        EditorKind::Codex => codex::install(repo_root, global_editor_config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::init::guide::{start_marker, CORE_RULES};

    /// (editor, guidance file, marker) for every supported client.
    fn guide_files() -> Vec<(EditorKind, &'static str, &'static str)> {
        vec![
            (EditorKind::ClaudeCode, ".claude/CLAUDE.md", claude::MARKER),
            (EditorKind::Codex, "AGENTS.md", codex::MARKER),
            (
                EditorKind::Antigravity,
                ".agents/plugins/gitcortex/rules/AGENTS.md",
                antigravity::MARKER,
            ),
            (
                EditorKind::Cursor,
                ".cursor/rules/gitcortex.mdc",
                cursor::MARKER,
            ),
            (
                EditorKind::Copilot,
                ".github/copilot-instructions.md",
                copilot::MARKER,
            ),
            (EditorKind::Windsurf, ".windsurfrules", windsurf::MARKER),
        ]
    }

    #[test]
    fn editor_guides_share_core_rules() {
        let repo = tempfile::tempdir().expect("repo");
        let files = guide_files();
        assert_eq!(files.len(), EditorKind::all().len());
        // A stale block from an older release must be replaced, not duplicated.
        std::fs::write(
            repo.path().join("AGENTS.md"),
            format!(
                "# Team notes\n\n{}\nold guidance\n{}\n",
                start_marker(codex::MARKER),
                crate::cmd::init::guide::end_marker(codex::MARKER)
            ),
        )
        .expect("seed legacy block");

        for (editor, _, _) in &files {
            install_for_editor(editor, repo.path(), false).expect("install");
        }
        let first: Vec<String> = files
            .iter()
            .map(|(_, path, _)| std::fs::read_to_string(repo.path().join(path)).expect("guide"))
            .collect();
        for (editor, _, _) in &files {
            install_for_editor(editor, repo.path(), false).expect("reinstall");
        }

        for ((editor, path, marker), before) in files.iter().zip(&first) {
            let text = std::fs::read_to_string(repo.path().join(path)).expect("guide");
            assert!(
                text.contains(CORE_RULES),
                "{editor:?} {path} lacks core rules"
            );
            assert_eq!(
                text.matches(&start_marker(marker)).count(),
                1,
                "{editor:?} {path} block duplicated"
            );
            assert_eq!(&text, before, "{editor:?} {path} changed on re-init");
        }
        let agents = std::fs::read_to_string(repo.path().join("AGENTS.md")).expect("agents");
        assert!(agents.starts_with("# Team notes\n"));
        assert!(!agents.contains("old guidance"));
    }
}
