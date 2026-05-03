use serde::{Deserialize, Serialize};

/// Bumped whenever the on-disk graph schema changes.
/// Stores compare this against the persisted version and re-index on mismatch.
pub const SCHEMA_VERSION: u32 = 3;

/// Every named, referenceable syntactic entity becomes a node of one of these kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Folder,
    File,
    Module,
    Struct,
    Enum,
    /// Rust trait. Languages with a separate notion of interface use [`NodeKind::Interface`].
    Trait,
    /// Language interface (Java, TypeScript, Go) — semantically distinct from Rust traits.
    Interface,
    TypeAlias,
    Function,
    Method,
    /// Property (Python `@property`, TypeScript `readonly` field, getter/setter pair).
    Property,
    Constant,
    Macro,
    /// Decorator / annotation declaration (e.g. `@dataclass`, `@Override`, `#[derive(...)]`).
    Annotation,
    /// Member of an enum (`Color::Red`, `Direction.NORTH`).
    EnumMember,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NodeKind::Folder => "folder",
            NodeKind::File => "file",
            NodeKind::Module => "module",
            NodeKind::Struct => "struct",
            NodeKind::Enum => "enum",
            NodeKind::Trait => "trait",
            NodeKind::Interface => "interface",
            NodeKind::TypeAlias => "type_alias",
            NodeKind::Function => "function",
            NodeKind::Method => "method",
            NodeKind::Property => "property",
            NodeKind::Constant => "constant",
            NodeKind::Macro => "macro",
            NodeKind::Annotation => "annotation",
            NodeKind::EnumMember => "enum_member",
        };
        f.write_str(s)
    }
}

/// Directed relationship between two nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Parent–child containment: File→Module, Module→Struct, Struct→Method.
    Contains,
    /// Resolved call site: Function→Function or Method→Method.
    Calls,
    /// `impl Trait for Struct`, `class Foo implements Bar` — Struct→Trait/Interface.
    Implements,
    /// `class Foo extends Bar`, embedded struct in Go — subtype→supertype.
    /// Distinct from `Implements`: this is "is-a" inheritance vs "can-do" interface
    /// satisfaction.
    Inherits,
    /// A type appears as a parameter or return type: fn→Struct/Trait.
    Uses,
    /// `use path::to::Thing` import.
    Imports,
    /// A symbol is decorated/annotated by another (`@Override`, `@dataclass`,
    /// `#[derive(Debug)]`).
    Annotated,
    /// Java `throws ExceptionType` — method→exception class.
    Throws,
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EdgeKind::Contains => "contains",
            EdgeKind::Calls => "calls",
            EdgeKind::Implements => "implements",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Uses => "uses",
            EdgeKind::Imports => "imports",
            EdgeKind::Annotated => "annotated",
            EdgeKind::Throws => "throws",
        };
        f.write_str(s)
    }
}

/// Symbol visibility in the source language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Private,
    PubCrate,
    Pub,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Pub => f.write_str("pub"),
            Visibility::PubCrate => f.write_str("pub_crate"),
            Visibility::Private => f.write_str("private"),
        }
    }
}

// ── LLD labels ──────────────────────────────────────────────────────────────

/// Which SOLID principle a node may be violating (populated in pass 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SolidHint {
    /// Too many responsibilities in one type.
    Srp,
    /// Logic closed for extension but open for modification.
    Ocp,
    /// Subtype breaks contract of supertype.
    Lsp,
    /// Interface has too many unrelated methods.
    Isp,
    /// Depends on concrete type instead of abstraction.
    Dip,
}

/// Common design patterns detectable syntactically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignPattern {
    Builder,
    Factory,
    Observer,
    Strategy,
    Decorator,
    Singleton,
    Repository,
}

/// Code quality smells detectable without full type resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSmell {
    /// Struct with too many methods or dependencies.
    GodStruct,
    /// Function body too long.
    LongMethod,
    /// Nesting depth exceeds threshold.
    DeepNesting,
    /// Trait with too many methods.
    FatInterface,
}
