use serde::{Deserialize, Serialize};

/// Bumped whenever the on-disk graph schema changes.
/// Stores compare this against the persisted version and re-index on mismatch.
pub const SCHEMA_VERSION: u32 = 12;

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
    /// A Markdown heading section (`## Installation`). File-level prose lives
    /// directly on the synthesized `File` node; this kind only covers headings.
    Section,
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
            NodeKind::Section => "section",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for NodeKind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "folder" => Ok(NodeKind::Folder),
            "file" => Ok(NodeKind::File),
            "module" => Ok(NodeKind::Module),
            "struct" => Ok(NodeKind::Struct),
            "enum" => Ok(NodeKind::Enum),
            "trait" => Ok(NodeKind::Trait),
            "interface" => Ok(NodeKind::Interface),
            "type_alias" => Ok(NodeKind::TypeAlias),
            "function" => Ok(NodeKind::Function),
            "method" => Ok(NodeKind::Method),
            "property" => Ok(NodeKind::Property),
            "constant" => Ok(NodeKind::Constant),
            "macro" => Ok(NodeKind::Macro),
            "annotation" => Ok(NodeKind::Annotation),
            "enum_member" | "enum-member" => Ok(NodeKind::EnumMember),
            "section" => Ok(NodeKind::Section),
            _ => Err(()),
        }
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
    /// A Markdown section (or file-level prose) mentions a code symbol, via
    /// an inline code-span or link text matching a known identifier.
    /// Source can be cross-language by design (docs reference any language).
    References,
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
            EdgeKind::References => "references",
        };
        f.write_str(s)
    }
}

/// How confident the indexer is that an edge is real. Direct edges resolved
/// within a single file are `Extracted`; cross-file edges resolved by matching
/// an unqualified name against the symbol table are `Inferred` (a same-named
/// symbol in another module could in principle be the true target).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EdgeConfidence {
    /// Directly observed in the source (same-file resolution). High confidence.
    #[default]
    Extracted,
    /// Resolved cross-file by name match. Lower confidence.
    Inferred,
}

impl std::fmt::Display for EdgeConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EdgeConfidence::Extracted => "extracted",
            EdgeConfidence::Inferred => "inferred",
        })
    }
}

impl EdgeConfidence {
    /// Parse from the stored string form; unknown/empty defaults to `Extracted`.
    pub fn from_label(s: &str) -> Self {
        match s {
            "inferred" => EdgeConfidence::Inferred,
            _ => EdgeConfidence::Extracted,
        }
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

impl std::str::FromStr for Visibility {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pub" => Ok(Visibility::Pub),
            "pub_crate" => Ok(Visibility::PubCrate),
            "private" => Ok(Visibility::Private),
            _ => Err(()),
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
