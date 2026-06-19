use tree_sitter::Node as TsNode;

/// Count cyclomatic complexity of a function body subtree.
///
/// Complexity = 1 (baseline) + number of decision-point AST nodes within the
/// subtree. Decision points are language-specific: `is_decision` returns true
/// for node kinds that introduce a new execution path (if, while, for, match
/// arm, case, catch, etc.).
///
/// Uses an iterative DFS to avoid stack overflows on deeply nested bodies.
pub fn cyclomatic_complexity(root: TsNode<'_>, is_decision: &dyn Fn(&str) -> bool) -> u32 {
    let mut complexity = 1u32;
    let mut stack: Vec<TsNode<'_>> = vec![root];
    while let Some(node) = stack.pop() {
        if is_decision(node.kind()) {
            complexity += 1;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    complexity
}

// ── Per-language decision-point predicates ────────────────────────────────────

pub fn rust_decision(kind: &str) -> bool {
    matches!(
        kind,
        "if_expression"
            | "while_expression"
            | "while_let_expression"
            | "for_expression"
            | "loop_expression"
            | "match_arm"
    )
}

pub fn python_decision(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement" | "elif_clause" | "while_statement" | "for_statement" | "except_clause"
    )
}

pub fn typescript_decision(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "while_statement"
            | "do_statement"
            | "for_statement"
            | "for_in_statement"
            | "switch_case"
            | "catch_clause"
    )
}

pub fn go_decision(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "expression_case"
            | "type_case"
            | "select_statement"
            | "communication_case"
    )
}

pub fn java_decision(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "while_statement"
            | "do_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "switch_label"
            | "catch_clause"
    )
}
