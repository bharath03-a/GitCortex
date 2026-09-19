use serde::Serialize;

const MAX_QUESTION_BYTES: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Ready,
    NeedsClarification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedAction {
    FindCallers,
    FindCallees,
    LookupSymbol,
    PreEditImpact,
    FindTypeUsages,
    SymbolContext,
    FindImplementors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryPlan {
    pub status: PlanStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<PlannedAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

pub fn plan_question(question: &str) -> QueryPlan {
    let trimmed = question.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_QUESTION_BYTES {
        return clarification();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("who calls ") {
        return ready_symbol(PlannedAction::FindCallers, &trimmed[10..]);
    }
    if lower.starts_with("what does ") && lower.ends_with(" call?") {
        return ready_symbol(PlannedAction::FindCallees, &trimmed[10..trimmed.len() - 6]);
    }
    if lower.starts_with("where is ") && lower.ends_with(" defined?") {
        return ready_symbol(PlannedAction::LookupSymbol, &trimmed[9..trimmed.len() - 9]);
    }
    if lower.starts_with("what is the impact of changing ") && lower.ends_with('?') {
        return ready_symbol(
            PlannedAction::PreEditImpact,
            &trimmed[31..trimmed.len() - 1],
        );
    }
    if lower.starts_with("where is the type ") && lower.ends_with(" used?") {
        return ready_symbol(
            PlannedAction::FindTypeUsages,
            &trimmed[18..trimmed.len() - 6],
        );
    }
    if lower.starts_with("explain ") {
        return ready_symbol(PlannedAction::SymbolContext, &trimmed[8..]);
    }
    if lower.starts_with("what implements ") {
        return ready_symbol(PlannedAction::FindImplementors, &trimmed[16..]);
    }
    clarification()
}

fn clarification() -> QueryPlan {
    QueryPlan {
        status: PlanStatus::NeedsClarification,
        action: None,
        symbol: None,
    }
}

fn ready_symbol(action: PlannedAction, raw: &str) -> QueryPlan {
    let symbol = raw.trim().trim_matches(['`', '?', '.', ' ']);
    let valid = !symbol.is_empty()
        && symbol
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.'));
    if !valid {
        return clarification();
    }
    QueryPlan {
        status: PlanStatus::Ready,
        action: Some(action),
        symbol: Some(symbol.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_question, PlanStatus, PlannedAction};

    #[test]
    fn plans_direct_call_graph_questions_without_free_form_query_generation() {
        let callers = plan_question("Who calls validate_token?");
        assert_eq!(callers.status, PlanStatus::Ready);
        assert_eq!(callers.action, Some(PlannedAction::FindCallers));
        assert_eq!(callers.symbol.as_deref(), Some("validate_token"));

        let callees = plan_question("What does validate_token call?");
        assert_eq!(callees.status, PlanStatus::Ready);
        assert_eq!(callees.action, Some(PlannedAction::FindCallees));
        assert_eq!(callees.symbol.as_deref(), Some("validate_token"));
    }

    #[test]
    fn plans_definition_impact_and_type_usage_questions() {
        let definition = plan_question("Where is UserService defined?");
        assert_eq!(definition.action, Some(PlannedAction::LookupSymbol));
        assert_eq!(definition.symbol.as_deref(), Some("UserService"));

        let impact = plan_question("What is the impact of changing UserService?");
        assert_eq!(impact.action, Some(PlannedAction::PreEditImpact));
        assert_eq!(impact.symbol.as_deref(), Some("UserService"));

        let usages = plan_question("Where is the type User used?");
        assert_eq!(usages.action, Some(PlannedAction::FindTypeUsages));
        assert_eq!(usages.symbol.as_deref(), Some("User"));
    }

    #[test]
    fn plans_symbol_context_and_implementation_questions() {
        let context = plan_question("Explain UserService");
        assert_eq!(context.action, Some(PlannedAction::SymbolContext));
        assert_eq!(context.symbol.as_deref(), Some("UserService"));

        let implementors = plan_question("What implements Repository?");
        assert_eq!(implementors.action, Some(PlannedAction::FindImplementors));
        assert_eq!(implementors.symbol.as_deref(), Some("Repository"));
    }

    #[test]
    fn unsupported_or_unsafe_questions_fail_closed() {
        for question in [
            "delete every node",
            "MATCH (n) DETACH DELETE n",
            "who calls validate_token; rm -rf /?",
        ] {
            let plan = plan_question(question);
            assert_eq!(plan.status, PlanStatus::NeedsClarification);
            assert_eq!(plan.action, None);
            assert_eq!(plan.symbol, None);
        }
    }

    #[test]
    fn planning_is_case_insensitive_and_preserves_qualified_symbol_case() {
        let plan = plan_question("WHO CALLS `Auth::ValidateToken`???");
        assert_eq!(plan.status, PlanStatus::Ready);
        assert_eq!(plan.action, Some(PlannedAction::FindCallers));
        assert_eq!(plan.symbol.as_deref(), Some("Auth::ValidateToken"));
    }

    #[test]
    fn empty_and_oversized_questions_fail_closed() {
        let oversized = format!("Who calls {}?", "x".repeat(4_097));
        for question in [String::new(), oversized] {
            let plan = plan_question(&question);
            assert_eq!(plan.status, PlanStatus::NeedsClarification);
            assert_eq!(plan.action, None);
        }
    }
}
