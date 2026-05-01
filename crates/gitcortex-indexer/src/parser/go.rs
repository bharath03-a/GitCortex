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

pub struct GoParser {
    language: tree_sitter::Language,
}

impl GoParser {
    pub fn new() -> Self {
        Self { language: tree_sitter_go::LANGUAGE.into() }
    }
}

impl Default for GoParser {
    fn default() -> Self { Self::new() }
}

impl LanguageParser for GoParser {
    fn extensions(&self) -> &[&str] { &["go"] }

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
        visitor.visit_source_file(tree.root_node());

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
    /// type name → NodeId (struct/interface)
    type_index: HashMap<String, NodeId>,
    /// function/method name → NodeId
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
            type_index: HashMap::new(),
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

    /// In Go, exported = first letter is uppercase.
    fn visibility(name: &str) -> Visibility {
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            Visibility::Pub
        } else {
            Visibility::Private
        }
    }

    fn qualified(scope: &[String], name: &str) -> String {
        if scope.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{name}", scope.join("."))
        }
    }

    fn make_node(&self, id: NodeId, kind: NodeKind, name: String, scope: &[String], ts_node: TsNode<'_>) -> Node {
        Node {
            id,
            qualified_name: Self::qualified(scope, &name),
            kind,
            name: name.clone(),
            file: self.file.clone(),
            span: Self::span(ts_node),
            metadata: NodeMetadata {
                loc: (ts_node.end_position().row - ts_node.start_position().row + 1) as u32,
                visibility: Self::visibility(&name),
                is_async: false, // Go does not have async/await syntax
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
                "function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_insert_with(NodeId::new);
                    }
                }
                "method_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_insert_with(NodeId::new);
                    }
                }
                "type_declaration" => {
                    self.collect_type_names(child);
                }
                _ => {}
            }
        }
    }

    fn collect_type_names(&mut self, decl: TsNode<'_>) {
        let mut cursor = decl.walk();
        for spec in decl.named_children(&mut cursor) {
            if spec.kind() != "type_spec" { continue; }
            if let Some(name_node) = spec.child_by_field_name("name") {
                let name = self.text(name_node).to_owned();
                if let Some(type_node) = spec.child_by_field_name("type") {
                    if matches!(type_node.kind(), "struct_type" | "interface_type") {
                        self.type_index.entry(name).or_insert_with(NodeId::new);
                    }
                }
            }
        }
    }

    // ── Pass 2 ────────────────────────────────────────────────────────────────

    fn visit_source_file(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            self.visit_top_level(child);
        }
    }

    fn visit_top_level(&mut self, node: TsNode<'_>) {
        match node.kind() {
            "function_declaration" => self.visit_function(node, &[]),
            "method_declaration" => self.visit_method(node),
            "type_declaration" => self.visit_type_decl(node),
            "const_declaration" => self.visit_const_decl(node),
            _ => {}
        }
    }

    fn visit_function(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let name = self.text(name_node).to_owned();
        let id = self.fn_index.get(&name).cloned().unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Function, name, scope, node);
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_method(&mut self, node: TsNode<'_>) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let name = self.text(name_node).to_owned();

        // Determine the receiver type name for the scope.
        let receiver_type = self.receiver_type(node);
        let scope: Vec<String> = receiver_type.into_iter().collect();

        let container_id = scope.first().and_then(|t| self.type_index.get(t).cloned());
        let id = self.fn_index.get(&name).cloned().unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Method, name, &scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    /// Extract the receiver type name from `func (r *ReceiverType) MethodName()`.
    fn receiver_type(&self, method_node: TsNode<'_>) -> Option<String> {
        let recv = method_node.child_by_field_name("receiver")?;
        // The receiver is a parameter_list containing a parameter_declaration.
        let mut cursor = recv.walk();
        for param in recv.named_children(&mut cursor) {
            if param.kind() != "parameter_declaration" { continue; }
            if let Some(type_node) = param.child_by_field_name("type") {
                return match type_node.kind() {
                    "type_identifier" => Some(self.text(type_node).to_owned()),
                    "pointer_type" => {
                        // *Type → dereference to get the type identifier
                        let mut c = type_node.walk();
                        let result = type_node.named_children(&mut c)
                            .find(|n| n.kind() == "type_identifier")
                            .map(|n| self.text(n).to_owned());
                        result
                    }
                    _ => None,
                };
            }
        }
        None
    }

    fn visit_type_decl(&mut self, decl: TsNode<'_>) {
        let mut cursor = decl.walk();
        let specs: Vec<TsNode<'_>> = decl.named_children(&mut cursor).collect();
        for spec in specs {
            if spec.kind() != "type_spec" { continue; }
            let Some(name_node) = spec.child_by_field_name("name") else { continue };
            let name = self.text(name_node).to_owned();
            let Some(type_node) = spec.child_by_field_name("type") else { continue };

            let kind = match type_node.kind() {
                "struct_type" => NodeKind::Struct,
                "interface_type" => NodeKind::Trait,
                _ => {
                    // Simple type alias.
                    let id = NodeId::new();
                    let graph_node = self.make_node(id, NodeKind::TypeAlias, name, &[], spec);
                    self.nodes.push(graph_node);
                    continue;
                }
            };

            let id = self.type_index.get(&name).cloned().unwrap_or_else(NodeId::new);
            let graph_node = self.make_node(id, kind, name, &[], spec);
            self.nodes.push(graph_node);
        }
    }

    fn visit_const_decl(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        for spec in node.named_children(&mut cursor) {
            if spec.kind() != "const_spec" { continue; }
            let Some(name_node) = spec.child_by_field_name("name") else { continue };
            let name = self.text(name_node).to_owned();
            let id = NodeId::new();
            let graph_node = self.make_node(id, NodeKind::Constant, name, &[], spec);
            self.nodes.push(graph_node);
        }
    }

    fn collect_calls(&mut self, node: TsNode<'_>, caller_id: &NodeId) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() == "call_expression" {
                if let Some(callee) = self.callee_name(child) {
                    self.record_call(caller_id.clone(), callee);
                }
                if let Some(args) = child.child_by_field_name("arguments") {
                    self.collect_calls(args, caller_id);
                }
            } else {
                self.collect_calls(child, caller_id);
            }
        }
    }

    fn callee_name(&self, call_expr: TsNode<'_>) -> Option<String> {
        let func = call_expr.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => Some(self.text(func).to_owned()),
            "selector_expression" => func
                .child_by_field_name("field")
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
    use super::GoParser;
    use crate::parser::LanguageParser;

    fn parse(src: &str) -> (Vec<gitcortex_core::graph::Node>, Vec<gitcortex_core::graph::Edge>) {
        let r = GoParser::new().parse(Path::new("test.go"), src).unwrap();
        (r.nodes, r.edges)
    }

    #[test]
    fn parses_function() {
        let src = "package main\nfunc Greet(name string) string { return name }";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Function).collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "Greet");
    }

    #[test]
    fn parses_struct_and_method() {
        let src = "package main\ntype Person struct { Name string }\nfunc (p *Person) Greet() string { return p.Name }";
        let (nodes, edges) = parse(src);
        let structs: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Struct).collect();
        let methods: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Method).collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(methods.len(), 1);
        let contains: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Contains).collect();
        assert!(!contains.is_empty());
    }

    #[test]
    fn parses_interface() {
        let src = "package main\ntype Greeter interface { Greet() string }";
        let (nodes, _) = parse(src);
        let traits: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Trait).collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].name, "Greeter");
    }

    #[test]
    fn go_visibility_is_uppercase() {
        let src = "package main\nfunc Exported() {}\nfunc unexported() {}";
        let (nodes, _) = parse(src);
        use gitcortex_core::schema::Visibility;
        let exp = nodes.iter().find(|n| n.name == "Exported").unwrap();
        let unexp = nodes.iter().find(|n| n.name == "unexported").unwrap();
        assert_eq!(exp.metadata.visibility, Visibility::Pub);
        assert_eq!(unexp.metadata.visibility, Visibility::Private);
    }

    #[test]
    fn detects_call_edges() {
        let src = "package main\nfunc Caller() { Callee() }\nfunc Callee() {}";
        let (_, edges) = parse(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1);
    }
}
