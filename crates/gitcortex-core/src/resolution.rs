/// Maximum number of same-language candidates that name-based resolution may
/// connect to. Larger candidate sets are too ambiguous to produce useful graph
/// edges and must be left unresolved.
pub const MAX_RESOLVE_FANOUT: usize = 8;

use crate::schema::{EdgeKind, NodeKind};

/// Whether a name-only candidate set is precise enough to resolve.
pub fn candidate_count_is_resolvable(count: usize) -> bool {
    count <= MAX_RESOLVE_FANOUT
}

/// Store-side variant for database aggregate counts. Negative or excessively
/// large values are never resolvable and require no lossy integer conversion.
pub fn database_candidate_count_is_resolvable(count: i64) -> bool {
    count >= 0 && count <= MAX_RESOLVE_FANOUT as i64
}

/// Whether `target` is a semantically valid destination for `edge` during
/// name-based resolution. `None`-filtered relationship kinds accept any node.
pub fn target_kind_is_valid(edge: &EdgeKind, target: &NodeKind) -> bool {
    match edge {
        EdgeKind::Calls | EdgeKind::HandledBy => {
            matches!(target, NodeKind::Function | NodeKind::Method)
        }
        EdgeKind::Uses => matches!(
            target,
            NodeKind::Struct
                | NodeKind::Enum
                | NodeKind::Trait
                | NodeKind::Interface
                | NodeKind::TypeAlias
        ),
        EdgeKind::Implements => {
            matches!(
                target,
                NodeKind::Struct | NodeKind::Trait | NodeKind::Interface
            )
        }
        EdgeKind::Inherits => matches!(
            target,
            NodeKind::Struct | NodeKind::Trait | NodeKind::Interface
        ),
        EdgeKind::Annotated => matches!(
            target,
            NodeKind::Annotation | NodeKind::Macro | NodeKind::Function
        ),
        EdgeKind::Contains | EdgeKind::Imports | EdgeKind::Throws | EdgeKind::References => true,
    }
}

/// On-disk labels accepted by [`target_kind_is_valid`]. `None` means no kind
/// predicate is required by the store query.
pub fn target_kind_labels(edge: &EdgeKind) -> Option<&'static [&'static str]> {
    match edge {
        EdgeKind::Calls | EdgeKind::HandledBy => Some(&["function", "method"]),
        EdgeKind::Uses => Some(&["struct", "enum", "trait", "interface", "type_alias"]),
        EdgeKind::Implements => Some(&["struct", "trait", "interface"]),
        EdgeKind::Inherits => Some(&["struct", "trait", "interface"]),
        EdgeKind::Annotated => Some(&["annotation", "macro", "function"]),
        EdgeKind::Contains | EdgeKind::Imports | EdgeKind::Throws | EdgeKind::References => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_count_at_limit_is_resolvable() {
        assert!(candidate_count_is_resolvable(MAX_RESOLVE_FANOUT));
    }

    #[test]
    fn candidate_count_above_limit_is_ambiguous() {
        assert!(!candidate_count_is_resolvable(MAX_RESOLVE_FANOUT + 1));
        assert!(!database_candidate_count_is_resolvable(-1));
        assert!(!database_candidate_count_is_resolvable(i64::MAX));
    }

    #[test]
    fn calls_accept_functions_and_methods_only() {
        assert!(target_kind_is_valid(&EdgeKind::Calls, &NodeKind::Function));
        assert!(target_kind_is_valid(&EdgeKind::Calls, &NodeKind::Method));
        assert!(!target_kind_is_valid(&EdgeKind::Calls, &NodeKind::Struct));
        assert_eq!(
            target_kind_labels(&EdgeKind::Calls),
            Some(&["function", "method"][..])
        );
    }

    #[test]
    fn target_kind_predicate_and_store_labels_agree() {
        let edges = [
            EdgeKind::Contains,
            EdgeKind::Calls,
            EdgeKind::Implements,
            EdgeKind::Inherits,
            EdgeKind::Uses,
            EdgeKind::Imports,
            EdgeKind::Annotated,
            EdgeKind::Throws,
            EdgeKind::References,
            EdgeKind::HandledBy,
        ];
        let nodes = [
            NodeKind::Folder,
            NodeKind::File,
            NodeKind::Module,
            NodeKind::Struct,
            NodeKind::Enum,
            NodeKind::Trait,
            NodeKind::Interface,
            NodeKind::TypeAlias,
            NodeKind::Function,
            NodeKind::Method,
            NodeKind::Property,
            NodeKind::Constant,
            NodeKind::Macro,
            NodeKind::Annotation,
            NodeKind::EnumMember,
            NodeKind::Section,
            NodeKind::Route,
        ];
        for edge in &edges {
            for node in &nodes {
                let label_allowed = target_kind_labels(edge)
                    .map(|labels| labels.contains(&node.to_string().as_str()))
                    .unwrap_or(true);
                assert_eq!(
                    target_kind_is_valid(edge, node),
                    label_allowed,
                    "policy differs for {edge} -> {node}"
                );
            }
        }
    }
}
