//! Cypher query helpers — row → `Node` conversion and column lists shared
//! across all node-returning queries.

use std::path::PathBuf;

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{DefinitionText, Node, NodeId, NodeMetadata, Span},
};
use kuzu::{Connection, Value};

use super::{
    conv::{kind_from_str, vis_from_str},
    values::{bool_val, i64_val, str_val},
};

/// Number of columns emitted by `NODE_COLS`. Extra columns appended after this
/// index (e.g. `e.confidence`) can be read as `row[NODE_COL_COUNT]`.
pub(super) const NODE_COL_COUNT: usize = 25;

/// Fixed column projection used in all node-returning queries.
/// Order must match `row_to_node()`.
pub(super) const NODE_COLS: &str = "n.id, n.kind, n.name, n.qualified_name, n.file, \
     n.start_line, n.end_line, n.loc, n.visibility, n.is_async, n.is_unsafe, \
     n.is_static, n.is_abstract, n.is_final, n.is_property, n.is_generator, n.is_const, \
     n.generic_bounds, n.def_signature, n.def_body, n.def_doc, n.def_start_byte, n.def_end_byte, \
     n.complexity, n.annotations";

/// Cypher `ORDER BY` fragment that ranks a node by "how likely the user meant
/// THIS one" when several share a name. Type declarations win over
/// functions/methods, which win over constants, which win over structural
/// (module/file/folder) nodes. Ties break by source line for determinism.
///
/// Without this, `wiki Echo` on a Go repo (where `Echo` is both a `type` and a
/// `Context.Echo()` method) — or `wiki Gson` on Java (class vs file `module`)
/// — would surface the wrong node, hiding the headline type.
pub(super) const SYMBOL_RANK: &str = "CASE n.kind \
     WHEN 'struct' THEN 0 WHEN 'enum' THEN 0 WHEN 'trait' THEN 0 \
     WHEN 'interface' THEN 0 WHEN 'type_alias' THEN 0 \
     WHEN 'function' THEN 1 WHEN 'method' THEN 1 \
     WHEN 'macro' THEN 2 WHEN 'constant' THEN 2 WHEN 'property' THEN 2 \
     WHEN 'annotation' THEN 2 WHEN 'enum_member' THEN 2 \
     ELSE 5 END, n.start_line";

pub(super) fn rows_to_nodes(result: &mut kuzu::QueryResult) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for row in result.by_ref() {
        match row_to_node(row) {
            Ok(n) => nodes.push(n),
            Err(e) => tracing::debug!("skipping malformed node row: {e}"),
        }
    }
    Ok(nodes)
}

pub(super) fn row_to_node(row: Vec<Value>) -> Result<Node> {
    if row.len() < 25 {
        return Err(GitCortexError::Store(format!(
            "expected 25 columns, got {}",
            row.len()
        )));
    }
    let id_str = str_val(&row[0])?;
    let kind = kind_from_str(&str_val(&row[1])?);
    let name = str_val(&row[2])?;
    let qualified_name = str_val(&row[3])?;
    let file = PathBuf::from(str_val(&row[4])?);
    let start_line = i64_val(&row[5])? as u32;
    let end_line = i64_val(&row[6])? as u32;
    let loc = i64_val(&row[7])? as u32;
    let visibility = vis_from_str(&str_val(&row[8])?);
    let is_async = bool_val(&row[9])?;
    let is_unsafe = bool_val(&row[10])?;
    let is_static = bool_val(&row[11])?;
    let is_abstract = bool_val(&row[12])?;
    let is_final = bool_val(&row[13])?;
    let is_property = bool_val(&row[14])?;
    let is_generator = bool_val(&row[15])?;
    let is_const = bool_val(&row[16])?;
    let generic_bounds_str = str_val(&row[17])?;
    let generic_bounds: Vec<String> = if generic_bounds_str.is_empty() {
        Vec::new()
    } else {
        generic_bounds_str.split('|').map(String::from).collect()
    };
    let def_signature = str_val(&row[18])?;
    let def_body = str_val(&row[19])?;
    let def_doc_raw = str_val(&row[20])?;
    let def_doc = if def_doc_raw.is_empty() {
        None
    } else {
        Some(def_doc_raw)
    };
    let def_start_byte = i64_val(&row[21]).unwrap_or(0) as u32;
    let def_end_byte = i64_val(&row[22]).unwrap_or(0) as u32;
    let complexity = i64_val(&row[23])
        .ok()
        .and_then(|c| if c >= 0 { Some(c as u32) } else { None });
    let annotations_str = str_val(&row[24])?;
    let annotations: Vec<String> = if annotations_str.is_empty() {
        Vec::new()
    } else {
        annotations_str.split('|').map(String::from).collect()
    };

    Ok(Node {
        id: NodeId::try_from(id_str.as_str())
            .map_err(|e| GitCortexError::Store(format!("bad node id: {e}")))?,
        kind,
        name,
        qualified_name,
        file,
        span: Span {
            start_line,
            end_line,
        },
        metadata: NodeMetadata {
            loc,
            visibility,
            is_async,
            is_unsafe,
            is_static,
            is_abstract,
            is_final,
            is_property,
            is_generator,
            is_const,
            generic_bounds,
            annotations,
            definition: DefinitionText {
                signature: def_signature,
                body: def_body,
                doc_comment: def_doc,
                start_byte: def_start_byte,
                end_byte: def_end_byte,
            },
            lld: gitcortex_core::graph::LldLabels {
                complexity,
                ..Default::default()
            },
        },
    })
}

pub(super) fn collect_ids(conn: &mut Connection, table: &str) -> Result<Vec<String>> {
    let result = conn
        .query(&format!("MATCH (n:{table}) RETURN n.id"))
        .map_err(|e| GitCortexError::Store(e.to_string()))?;

    let mut ids = Vec::new();
    for row in result {
        ids.push(str_val(&row[0])?);
    }
    Ok(ids)
}
