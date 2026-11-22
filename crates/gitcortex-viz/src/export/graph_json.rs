use serde_json::{json, Value};

use crate::export::{ExportScope, ScopedGraph};

/// Current schema version for `gitcortex.graph` JSON exports.
pub const GRAPH_JSON_SCHEMA_VERSION: u32 = 1;

/// Provenance and scoping metadata emitted alongside a [`ScopedGraph`].
#[derive(Debug, Clone)]
pub struct ExportMeta {
    pub gcx_version: String,
    pub repo_name: String,
    pub commit: Option<String>,
    pub readiness: String,
    /// RFC 3339 UTC timestamp (e.g. `2026-11-05T00:00:00Z`).
    pub generated_at: String,
    pub scope: ExportScope,
}

/// Pure serializer for a [`ScopedGraph`] into the versioned `gitcortex.graph`
/// v1 JSON document. Never touches a store, never re-resolves node IDs, and
/// never emits source text (only names, kinds, paths, lines, and flags).
pub fn to_graph_json(g: &ScopedGraph, meta: &ExportMeta) -> Value {
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id.as_str(),
                "name": n.name,
                "qualified_name": n.qualified_name,
                "kind": n.kind.to_string(),
                "file": n.file.display().to_string(),
                "start_line": n.span.start_line,
                "end_line": n.span.end_line,
                "visibility": n.metadata.visibility.to_string(),
                "is_async": n.metadata.is_async,
                "is_unsafe": n.metadata.is_unsafe,
            })
        })
        .collect();

    let edges: Vec<Value> = g
        .edges
        .iter()
        .map(|e| {
            json!({
                "source": e.src.as_str(),
                "target": e.dst.as_str(),
                "kind": e.kind.to_string(),
                "line": e.line,
                "confidence": e.confidence.to_string(),
            })
        })
        .collect();

    json!({
        "format": "gitcortex.graph",
        "schema_version": GRAPH_JSON_SCHEMA_VERSION,
        "meta": {
            "gcx_version": meta.gcx_version,
            "repo": meta.repo_name,
            "branch": g.branch,
            "commit": meta.commit,
            "readiness": meta.readiness,
            "generated_at": meta.generated_at,
            "scope": meta.scope,
            "truncated": g.truncated,
            "total_nodes": g.total_nodes,
            "total_edges": g.total_edges,
        },
        "nodes": nodes,
        "edges": edges,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use gitcortex_core::{
        graph::{DefinitionText, Edge, Node, NodeId, NodeMetadata, Span},
        schema::{EdgeConfidence, EdgeKind, NodeKind, Visibility},
    };
    use serde_json::Value;

    use super::*;
    use crate::export::{Direction, NodeOrder};

    fn schema_validator() -> jsonschema::Validator {
        let raw = include_str!("../../../../docs/schemas/gitcortex-graph-v1.schema.json");
        let schema: Value = serde_json::from_str(raw).expect("valid schema json");
        jsonschema::validator_for(&schema).expect("compile schema")
    }

    fn sample_node(name: &str, line: u32) -> Node {
        Node {
            id: NodeId::stable(name),
            name: name.into(),
            qualified_name: format!("sample::{name}"),
            kind: NodeKind::Function,
            file: "src/lib.rs".into(),
            span: Span {
                start_line: line,
                end_line: line + 3,
            },
            metadata: NodeMetadata {
                visibility: Visibility::Pub,
                is_async: true,
                is_unsafe: false,
                definition: DefinitionText {
                    signature: format!("pub async fn {name}()"),
                    body: "SECRET_BODY_TOKEN_MUST_NOT_LEAK".into(),
                    doc_comment: Some("SECRET_DOC_COMMENT".into()),
                    start_byte: 0,
                    end_byte: 42,
                },
                ..Default::default()
            },
        }
    }

    fn sample_meta(readiness: &str, scope: ExportScope) -> ExportMeta {
        ExportMeta {
            gcx_version: "0.8.0".into(),
            repo_name: "GitCortex".into(),
            commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
            readiness: readiness.into(),
            generated_at: "2026-11-05T00:00:00Z".into(),
            scope,
        }
    }

    #[test]
    fn graph_json_validates_against_schema() {
        let a = sample_node("alpha", 1);
        let b = sample_node("beta", 10);
        let mut edge = Edge::new(a.id.clone(), b.id.clone(), EdgeKind::Calls);
        edge.line = Some(3);
        edge.confidence = EdgeConfidence::Resolved;

        let g = ScopedGraph::from_explicit("main".into(), vec![a, b], vec![edge]).unwrap();
        let doc = to_graph_json(&g, &sample_meta("ready", ExportScope::default()));

        let validator = schema_validator();
        let errors: Vec<_> = validator.iter_errors(&doc).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "schema errors: {errors:?}");
        assert_eq!(doc["format"], "gitcortex.graph");
        assert_eq!(doc["schema_version"], GRAPH_JSON_SCHEMA_VERSION);
        assert_eq!(doc["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(doc["edges"].as_array().unwrap().len(), 1);
        assert_eq!(doc["edges"][0]["line"], 3);
        assert_eq!(doc["edges"][0]["confidence"], "resolved");
    }

    #[test]
    fn edges_reference_existing_nodes() {
        let a = sample_node("alpha", 1);
        let b = sample_node("beta", 10);
        let edge = Edge::new(a.id.clone(), b.id.clone(), EdgeKind::Imports);
        let g = ScopedGraph::from_explicit("main".into(), vec![a, b], vec![edge]).unwrap();
        let doc = to_graph_json(&g, &sample_meta("ready", ExportScope::default()));

        let node_ids: HashSet<&str> = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        for edge in doc["edges"].as_array().unwrap() {
            assert!(node_ids.contains(edge["source"].as_str().unwrap()));
            assert!(node_ids.contains(edge["target"].as_str().unwrap()));
        }
    }

    #[test]
    fn meta_records_readiness() {
        let g = ScopedGraph::from_explicit("main".into(), vec![], vec![]).unwrap();
        let doc = to_graph_json(&g, &sample_meta("stale", ExportScope::default()));
        assert_eq!(doc["meta"]["readiness"], "stale");
        let validator = schema_validator();
        assert!(validator.is_valid(&doc));
    }

    #[test]
    fn cap_sets_truncated_and_totals() {
        let a = sample_node("alpha", 1);
        let b = sample_node("beta", 10);
        let c = sample_node("gamma", 20);
        let e1 = Edge::new(a.id.clone(), b.id.clone(), EdgeKind::Calls);
        let e2 = Edge::new(b.id.clone(), c.id.clone(), EdgeKind::Calls);
        let g = ScopedGraph {
            branch: "main".into(),
            nodes: vec![a, b],
            edges: vec![e1],
            truncated: true,
            total_nodes: 3,
            total_edges: 2,
            seed: None,
        };
        let scope = ExportScope {
            max_nodes: Some(2),
            ..ExportScope::default()
        };
        let _ = e2;
        let doc = to_graph_json(&g, &sample_meta("ready", scope));
        assert_eq!(doc["meta"]["truncated"], true);
        assert_eq!(doc["meta"]["total_nodes"], 3);
        assert_eq!(doc["meta"]["total_edges"], 2);
        assert_eq!(doc["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(doc["edges"].as_array().unwrap().len(), 1);
        let validator = schema_validator();
        assert!(validator.is_valid(&doc));
    }

    #[test]
    fn explicit_union_preserves_base_only_nodes() {
        let head_sha = "abcdef0123456789abcdef0123456789abcdef01";
        let base_only = sample_node("deleted_in_head", 5);
        let caller = sample_node("caller_in_head", 20);
        let edge = Edge::new(caller.id.clone(), base_only.id.clone(), EdgeKind::Calls);
        let g = ScopedGraph::from_explicit(
            head_sha.into(),
            vec![base_only.clone(), caller.clone()],
            vec![edge],
        )
        .unwrap();
        let scope = ExportScope {
            seed: None,
            depth: 2,
            direction: Direction::Both,
            edge_kinds: None,
            max_nodes: None,
            order: NodeOrder::Deterministic,
        };
        let doc = to_graph_json(&g, &sample_meta("ready", scope));

        let validator = schema_validator();
        let errors: Vec<_> = validator.iter_errors(&doc).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "schema errors: {errors:?}");
        assert_eq!(doc["meta"]["branch"], head_sha);
        assert!(doc["meta"]["scope"]["seed"].is_null());
        assert!(doc["meta"]["scope"]["max_nodes"].is_null());
        assert_eq!(doc["meta"]["truncated"], false);
        assert_eq!(doc["meta"]["total_nodes"], 2);
        assert_eq!(doc["meta"]["total_edges"], 1);
        let ids: HashSet<&str> = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(base_only.id.as_str().as_str()));
        assert_eq!(doc["edges"][0]["target"], base_only.id.as_str());
    }

    #[test]
    fn graph_json_never_emits_source_text() {
        let n = sample_node("alpha", 1);
        let g = ScopedGraph::from_explicit("main".into(), vec![n], vec![]).unwrap();
        let serialized =
            to_graph_json(&g, &sample_meta("ready", ExportScope::default())).to_string();
        assert!(!serialized.contains("SECRET_BODY_TOKEN_MUST_NOT_LEAK"));
        assert!(!serialized.contains("SECRET_DOC_COMMENT"));
        assert!(!serialized.contains("pub async fn alpha"));
    }
}
