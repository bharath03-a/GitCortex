use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
};
use tree_sitter::{Node as TsNode, Parser};

use super::{LanguageParser, ParseResult};

pub struct PythonParser {
    language: tree_sitter::Language,
}

impl PythonParser {
    pub fn new() -> Self {
        Self { language: tree_sitter_python::LANGUAGE.into() }
    }
}

impl Default for PythonParser {
    fn default() -> Self { Self::new() }
}

impl LanguageParser for PythonParser {
    fn extensions(&self) -> &[&str] { &["py"] }

    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult> {
        let mut parser = Parser::new();
        parser.set_language(&self.language).map_err(|e| GitCortexError::Parse {
            file: path.to_owned(),
            message: e.to_string(),
        })?;

        let tree = parser.parse(source, None).ok_or_else(|| GitCortexError::Parse {
            file: path.to_owned(),
            message: "tree-sitter returned no parse tree".into(),
        })?;

        let mut visitor = FileVisitor::new(path, source);
        visitor.collect_names(tree.root_node());
        visitor.visit_module(tree.root_node());

        Ok(ParseResult {
            nodes: visitor.nodes,
            edges: visitor.edges,
            deferred_calls: visitor.deferred_calls,
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_imports: Vec::new(),
        })
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// class name → NodeId (pass 1)
    class_index: HashMap<String, NodeId>,
    /// function/method name → NodeId (pass 1)
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            file: file.to_owned(),
            nodes: Vec::new(),
            edges: Vec::new(),
            class_index: HashMap::new(),
            fn_index: HashMap::new(),
            deferred_calls: Vec::new(),
        }
    }

    fn text<'t>(&self, node: TsNode<'t>) -> &'src str {
        node.utf8_text(self.source).unwrap_or("")
    }

    fn span(node: TsNode<'_>) -> Span {
        Span {
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
        }
    }

    /// In Python, public = not starting with `_`.
    fn visibility(name: &str) -> Visibility {
        if name.starts_with('_') { Visibility::Private } else { Visibility::Pub }
    }

    fn qualified(scope: &[String], name: &str) -> String {
        if scope.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{name}", scope.join("."))
        }
    }

    fn make_node(
        &self,
        id: NodeId,
        kind: NodeKind,
        name: String,
        scope: &[String],
        ts_node: TsNode<'_>,
        is_async: bool,
    ) -> Node {
        let vis = Self::visibility(&name);
        Node {
            id,
            qualified_name: Self::qualified(scope, &name),
            kind,
            name,
            file: self.file.clone(),
            span: Self::span(ts_node),
            metadata: NodeMetadata {
                loc: (ts_node.end_position().row - ts_node.start_position().row + 1) as u32,
                visibility: vis,
                is_async,
                is_unsafe: false,
                ..Default::default()
            },
        }
    }

    // ── Pass 1 ────────────────────────────────────────────────────────────────

    fn collect_names(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            match child.kind() {
                "class_definition" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.class_index.entry(name).or_insert_with(NodeId::new);
                    }
                }
                "function_definition" | "decorated_definition" => {
                    let fn_node = if child.kind() == "decorated_definition" {
                        child.child_by_field_name("definition")
                    } else {
                        Some(child)
                    };
                    if let Some(fn_node) = fn_node {
                        if let Some(name_node) = fn_node.child_by_field_name("name") {
                            let name = self.text(name_node).to_owned();
                            self.fn_index.entry(name).or_insert_with(NodeId::new);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Pass 2 ────────────────────────────────────────────────────────────────

    fn visit_module(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            self.visit_top_level(child, &[]);
        }
    }

    fn visit_top_level(&mut self, node: TsNode<'_>, scope: &[String]) {
        match node.kind() {
            "function_definition" => {
                self.visit_function(node, scope, None, false);
            }
            "decorated_definition" => {
                let is_async = {
                    let mut c = node.walk();
                    let result = node.named_children(&mut c).any(|n| n.kind() == "async");
                    result
                };
                if let Some(def) = node.child_by_field_name("definition") {
                    match def.kind() {
                        "function_definition" => self.visit_function(def, scope, None, is_async),
                        "class_definition" => self.visit_class(def, scope),
                        _ => {}
                    }
                }
            }
            "class_definition" => self.visit_class(node, scope),
            "expression_statement" => self.maybe_visit_constant(node, scope),
            _ => {}
        }
    }

    fn visit_function(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
        is_async: bool,
    ) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let name = self.text(name_node).to_owned();
        let id = self.fn_index.get(&name).cloned().unwrap_or_else(NodeId::new);
        let kind = if container_id.is_some() { NodeKind::Method } else { NodeKind::Function };
        let graph_node = self.make_node(id.clone(), kind, name, scope, node, is_async);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_class(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let name = self.text(name_node).to_owned();
        let id = self.class_index.get(&name).cloned().unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Struct, name.clone(), scope, node, false);
        self.nodes.push(graph_node);

        let mut class_scope = scope.to_vec();
        class_scope.push(name.clone());

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            for child in children {
                match child.kind() {
                    "function_definition" => {
                        self.visit_function(child, &class_scope, Some(id.clone()), false);
                    }
                    "decorated_definition" => {
                        if let Some(def) = child.child_by_field_name("definition") {
                            if def.kind() == "function_definition" {
                                self.visit_function(def, &class_scope, Some(id.clone()), false);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn maybe_visit_constant(&mut self, node: TsNode<'_>, scope: &[String]) {
        // Only capture module-level UPPER_CASE = ... assignments.
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "assignment" {
                if let Some(left) = child.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let name = self.text(left).to_owned();
                        if name.chars().all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
                            && name.len() > 1
                            && !name.starts_with('_')
                        {
                            let id = NodeId::new();
                            let graph_node =
                                self.make_node(id, NodeKind::Constant, name, scope, node, false);
                            self.nodes.push(graph_node);
                        }
                    }
                }
            }
        }
    }

    fn collect_calls(&mut self, node: TsNode<'_>, caller_id: &NodeId) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() == "call" {
                if let Some(callee) = self.callee_name(child) {
                    self.record_call(caller_id.clone(), callee);
                }
                // Recurse into arguments.
                if let Some(args) = child.child_by_field_name("arguments") {
                    self.collect_calls(args, caller_id);
                }
            } else {
                self.collect_calls(child, caller_id);
            }
        }
    }

    fn callee_name(&self, call_node: TsNode<'_>) -> Option<String> {
        let func = call_node.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => Some(self.text(func).to_owned()),
            "attribute" => func
                .child_by_field_name("attribute")
                .map(|n| self.text(n).to_owned()),
            _ => None,
        }
    }

    fn record_call(&mut self, caller_id: NodeId, callee_name: String) {
        if callee_name.is_empty() { return; }
        if let Some(callee_id) = self.fn_index.get(&callee_name).cloned() {
            let edge = Edge { src: caller_id, dst: callee_id, kind: EdgeKind::Calls };
            if !self.edges.contains(&edge) {
                self.edges.push(edge);
            }
        } else if !self.deferred_calls.iter().any(|(c, n)| c == &caller_id && n == &callee_name) {
            self.deferred_calls.push((caller_id, callee_name));
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::Path;
    use gitcortex_core::schema::{NodeKind, EdgeKind};
    use super::PythonParser;
    use crate::parser::LanguageParser;

    fn parse(src: &str) -> (Vec<gitcortex_core::graph::Node>, Vec<gitcortex_core::graph::Edge>) {
        let r = PythonParser::new().parse(Path::new("test.py"), src).unwrap();
        (r.nodes, r.edges)
    }

    #[test]
    fn parses_free_function() {
        let (nodes, _) = parse("def greet(name):\n    return name\n");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, NodeKind::Function);
        assert_eq!(nodes[0].name, "greet");
    }

    #[test]
    fn parses_class_and_method() {
        let src = "class Person:\n    def greet(self):\n        pass\n";
        let (nodes, edges) = parse(src);
        let classes: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Struct).collect();
        let methods: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Method).collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(methods.len(), 1);
        let contains: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Contains).collect();
        assert!(!contains.is_empty());
    }

    #[test]
    fn detects_call_edges() {
        let src = "def caller():\n    callee()\ndef callee():\n    pass\n";
        let (_, edges) = parse(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1);
    }
}
