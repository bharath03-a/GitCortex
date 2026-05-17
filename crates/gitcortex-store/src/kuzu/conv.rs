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
