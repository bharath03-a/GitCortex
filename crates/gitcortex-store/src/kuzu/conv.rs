//! NodeKind / EdgeKind / Visibility ↔ string conversions used by Cypher
//! parameter binding and result decoding. Kept separate so changing the
//! on-disk string form doesn't require editing 1k-line files.

use gitcortex_core::schema::{EdgeKind, NodeKind, Visibility};

pub(super) fn kind_from_str(s: &str) -> NodeKind {
    match s {
        "folder" => NodeKind::Folder,
        "file" => NodeKind::File,
        "module" => NodeKind::Module,
        "struct" => NodeKind::Struct,
        "enum" => NodeKind::Enum,
        "trait" => NodeKind::Trait,
        "interface" => NodeKind::Interface,
        "type_alias" => NodeKind::TypeAlias,
        "function" => NodeKind::Function,
        "method" => NodeKind::Method,
        "property" => NodeKind::Property,
        "constant" => NodeKind::Constant,
        "macro" => NodeKind::Macro,
        "annotation" => NodeKind::Annotation,
        "enum_member" => NodeKind::EnumMember,
        "section" => NodeKind::Section,
        _ => NodeKind::Function,
    }
}

pub(super) fn edge_kind_from_str(s: &str) -> EdgeKind {
    match s {
        "calls" => EdgeKind::Calls,
        "implements" => EdgeKind::Implements,
        "inherits" => EdgeKind::Inherits,
        "uses" => EdgeKind::Uses,
        "imports" => EdgeKind::Imports,
        "annotated" => EdgeKind::Annotated,
        "throws" => EdgeKind::Throws,
        "references" => EdgeKind::References,
        _ => EdgeKind::Contains,
    }
}

pub(super) fn vis_str(v: &Visibility) -> String {
    match v {
        Visibility::Pub => "pub".into(),
        Visibility::PubCrate => "pub_crate".into(),
        Visibility::Private => "private".into(),
    }
}

pub(super) fn vis_from_str(s: &str) -> Visibility {
    match s {
        "pub" => Visibility::Pub,
        "pub_crate" => Visibility::PubCrate,
        _ => Visibility::Private,
    }
}

/// Return the list of file extensions (with leading dot) that belong to the
/// same language family as `file`. Used to scope deferred-edge resolution so
/// a Rust caller never resolves to a Python callee that shares a name.
///
/// Returns `None` for files we cannot classify — the caller treats that as
/// "no scoping" and falls back to the existing name-only match.
pub(super) fn language_extensions(file: &str) -> Option<&'static [&'static str]> {
    let ext = file.rsplit('.').next()?;
    match ext {
        "rs" => Some(&[".rs"]),
        "py" => Some(&[".py"]),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            Some(&[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"])
        }
        "go" => Some(&[".go"]),
        "java" => Some(&[".java"]),
        _ => None,
    }
}

/// Build a Cypher predicate that constrains `var.file` to any of the
/// language extensions matching `caller_file`. Returns an empty string when
/// the language is unknown — caller can splice it directly into a WHERE
/// clause.
pub(super) fn lang_scope_clause(caller_file: &str, var: &str) -> String {
    let Some(exts) = language_extensions(caller_file) else {
        return String::new();
    };
    let parts: Vec<String> = exts
        .iter()
        .map(|e| format!("ends_with({var}.file, '{e}')"))
        .collect();
    format!(" AND ({})", parts.join(" OR "))
}
