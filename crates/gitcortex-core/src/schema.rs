use serde::{Deserialize, Serialize};

/// Every named, referenceable syntactic entity becomes a node of one of these kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Module,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Function,
    Method,
    Constant,
    Macro,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NodeKind::File => "file",
            NodeKind::Module => "module",
            NodeKind::Struct => "struct",
            NodeKind::Enum => "enum",
            NodeKind::Trait => "trait",
            NodeKind::TypeAlias => "type_alias",
            NodeKind::Function => "function",
            NodeKind::Method => "method",
            NodeKind::Constant => "constant",
            NodeKind::Macro => "macro",
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
    /// `impl Trait for Struct` — Struct→Trait.
    Implements,
    /// A type appears as a parameter or return type: fn→Struct/Trait.
    Uses,
    /// `use path::to::Thing` import.
    Imports,
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EdgeKind::Contains => "contains",
            EdgeKind::Calls => "calls",
            EdgeKind::Implements => "implements",
            EdgeKind::Uses => "uses",
            EdgeKind::Imports => "imports",
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
