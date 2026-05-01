/// Parser for TypeScript (.ts, .tsx) and JavaScript (.js, .jsx).
///
/// Both grammars share nearly identical AST node types. The TypeScript grammar
/// (a superset of JS) is used for all four extensions.
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

// ── Public parser ─────────────────────────────────────────────────────────────

pub struct TypeScriptParser {
    language: tree_sitter::Language,
}

impl TypeScriptParser {
    pub fn new_ts() -> Self {
        Self {
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        }
    }

    pub fn new_tsx() -> Self {
        Self {
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }
}

pub struct JavaScriptParser {
    language: tree_sitter::Language,
}

impl JavaScriptParser {
    pub fn new() -> Self {
        Self {
            language: tree_sitter_javascript::LANGUAGE.into(),
        }
    }
}

impl Default for JavaScriptParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for TypeScriptParser {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult> {
        parse_source(&self.language, path, source)
    }
}

impl LanguageParser for JavaScriptParser {
    fn extensions(&self) -> &[&str] {
        &["js", "jsx", "mjs", "cjs"]
    }

    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult> {
        parse_source(&self.language, path, source)
    }
}

fn parse_source(
    language: &tree_sitter::Language,
    path: &Path,
    source: &str,
) -> Result<ParseResult> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|e| GitCortexError::Parse {
            file: path.to_owned(),
            message: e.to_string(),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| GitCortexError::Parse {
            file: path.to_owned(),
            message: "tree-sitter returned no parse tree".into(),
        })?;

    let mut visitor = FileVisitor::new(path, source);
    visitor.collect_names(tree.root_node());
    visitor.visit_program(tree.root_node(), &[]);

    Ok(ParseResult {
        nodes: visitor.nodes,
        edges: visitor.edges,
        deferred_calls: visitor.deferred_calls,
        deferred_uses: Vec::new(),
        deferred_implements: Vec::new(),
        deferred_imports: Vec::new(),
    })
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// class/interface/type name → NodeId
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

    fn visibility(node: TsNode<'_>, source: &[u8]) -> Visibility {
        // Check for `export` keyword as a sibling or parent.
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "accessibility_modifier" {
                let t = child.utf8_text(source).unwrap_or("");
                return match t {
                    "public" => Visibility::Pub,
                    "protected" => Visibility::PubCrate,
                    _ => Visibility::Private,
                };
            }
        }
        Visibility::Pub // JS/TS default is effectively public
    }

    fn is_async(node: TsNode<'_>) -> bool {
        let mut cursor = node.walk();
        let result = node.children(&mut cursor).any(|c| c.kind() == "async");
        result
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
    ) -> Node {
        Node {
            id,
            qualified_name: Self::qualified(scope, &name),
            kind,
            name,
            file: self.file.clone(),
            span: Self::span(ts_node),
            metadata: NodeMetadata {
                loc: (ts_node.end_position().row - ts_node.start_position().row + 1) as u32,
                visibility: Self::visibility(ts_node, self.source),
                is_async: Self::is_async(ts_node),
                is_unsafe: false,
                ..Default::default()
            },
        }
    }

    // ── Pass 1: pre-allocate NodeIds ──────────────────────────────────────────

    fn collect_names(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            let actual = self.unwrap_export(child);
            match actual.kind() {
                "class_declaration" | "abstract_class_declaration" => {
                    if let Some(name_node) = actual.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.type_index.entry(name).or_default();
                    }
                }
                "interface_declaration" | "type_alias_declaration" => {
                    if let Some(name_node) = actual.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.type_index.entry(name).or_default();
                    }
                }
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(name_node) = actual.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_default();
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    self.collect_names_from_var_decl(actual);
                }
                _ => {}
            }
        }
    }

    fn collect_names_from_var_decl(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = declarator.child_by_field_name("name") else {
                continue;
            };
            let Some(value) = declarator.child_by_field_name("value") else {
                continue;
            };
            if matches!(
                value.kind(),
                "arrow_function" | "function" | "function_expression"
            ) {
                let name = self.text(name_node).to_owned();
                self.fn_index.entry(name).or_default();
            }
        }
    }

    /// Strip export wrapper if present, returning the inner declaration.
    fn unwrap_export<'a>(&self, node: TsNode<'a>) -> TsNode<'a> {
        if node.kind() == "export_statement" {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "class_declaration"
                    | "abstract_class_declaration"
                    | "function_declaration"
                    | "generator_function_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "lexical_declaration"
                    | "variable_declaration"
                    | "enum_declaration" => return child,
                    _ => {}
                }
            }
        }
        node
    }

    // ── Pass 2: full walk ─────────────────────────────────────────────────────

    fn visit_program(&mut self, node: TsNode<'_>, scope: &[String]) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            let actual = self.unwrap_export(child);
            self.visit_statement(actual, scope, None);
        }
    }

    fn visit_statement(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        match node.kind() {
            "function_declaration" | "generator_function_declaration" => {
                self.visit_function(node, scope, container_id, NodeKind::Function);
            }
            "class_declaration" | "abstract_class_declaration" => {
                self.visit_class(node, scope);
            }
            "interface_declaration" => {
                self.visit_interface(node, scope);
            }
            "type_alias_declaration" => {
                self.visit_type_alias(node, scope);
            }
            "enum_declaration" => {
                self.visit_enum(node, scope);
            }
            "lexical_declaration" | "variable_declaration" => {
                self.visit_var_decl(node, scope);
            }
            _ => {}
        }
    }

    fn visit_function(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
        kind: NodeKind,
    ) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .fn_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), kind, name, scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_class(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Struct, name.clone(), scope, node);
        self.nodes.push(graph_node);

        let mut class_scope = scope.to_vec();
        class_scope.push(name);

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            for child in children {
                match child.kind() {
                    "method_definition" | "abstract_method_signature" => {
                        self.visit_method(child, &class_scope, id.clone());
                    }
                    "public_field_definition" => {}
                    _ => {}
                }
            }
        }
    }

    fn visit_method(&mut self, node: TsNode<'_>, scope: &[String], class_id: NodeId) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .fn_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Method, name, scope, node);
        self.edges.push(Edge {
            src: class_id,
            dst: id.clone(),
            kind: EdgeKind::Contains,
        });
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_interface(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Trait, name, scope, node);
        self.nodes.push(graph_node);
    }

    fn visit_type_alias(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id, NodeKind::TypeAlias, name, scope, node);
        self.nodes.push(graph_node);
    }

    fn visit_enum(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = NodeId::new();
        let graph_node = self.make_node(id, NodeKind::Enum, name, scope, node);
        self.nodes.push(graph_node);
    }

    fn visit_var_decl(&mut self, node: TsNode<'_>, scope: &[String]) {
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = declarator.child_by_field_name("name") else {
                continue;
            };
            let Some(value) = declarator.child_by_field_name("value") else {
                continue;
            };
            let name = self.text(name_node).to_owned();

            match value.kind() {
                "arrow_function" | "function" | "function_expression" => {
                    let id = self
                        .fn_index
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(NodeId::new);
                    let graph_node =
                        self.make_node(id.clone(), NodeKind::Function, name, scope, value);
                    self.nodes.push(graph_node);
                    if let Some(body) = value.child_by_field_name("body") {
                        self.collect_calls(body, &id);
                    }
                }
                _ => {
                    // const MY_CONST = value — only if name looks like a constant
                    let is_const_style = node.kind() == "lexical_declaration" && {
                        let mut c = node.walk();
                        let r = node.children(&mut c).any(|ch| ch.kind() == "const");
                        r
                    };
                    if is_const_style
                        && name
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false)
                    {
                        let id = NodeId::new();
                        let graph_node =
                            self.make_node(id, NodeKind::Constant, name, scope, declarator);
                        self.nodes.push(graph_node);
                    }
                }
            }
        }
    }

    fn collect_calls(&mut self, node: TsNode<'_>, caller_id: &NodeId) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            match child.kind() {
                "call_expression" | "new_expression" => {
                    if let Some(callee) = self.callee_name(child) {
                        self.record_call(caller_id.clone(), callee);
                    }
                    if let Some(args) = child.child_by_field_name("arguments") {
                        self.collect_calls(args, caller_id);
                    }
                }
                _ => self.collect_calls(child, caller_id),
            }
        }
    }

    fn callee_name(&self, call_expr: TsNode<'_>) -> Option<String> {
        let func = call_expr.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => Some(self.text(func).to_owned()),
            "member_expression" => func
                .child_by_field_name("property")
                .map(|n| self.text(n).to_owned()),
            _ => None,
        }
    }

    fn record_call(&mut self, caller_id: NodeId, callee_name: String) {
        if callee_name.is_empty() {
            return;
        }
        if let Some(callee_id) = self.fn_index.get(&callee_name).cloned() {
            let edge = Edge {
                src: caller_id,
                dst: callee_id,
                kind: EdgeKind::Calls,
            };
            if !self.edges.contains(&edge) {
                self.edges.push(edge);
            }
        } else if !self
            .deferred_calls
            .iter()
            .any(|(c, n)| c == &caller_id && n == &callee_name)
        {
            self.deferred_calls.push((caller_id, callee_name));
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{JavaScriptParser, TypeScriptParser};
    use crate::parser::LanguageParser;
    use gitcortex_core::schema::{EdgeKind, NodeKind};
    use std::path::Path;

    fn parse_ts(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
    ) {
        let r = TypeScriptParser::new_ts()
            .parse(Path::new("test.ts"), src)
            .unwrap();
        (r.nodes, r.edges)
    }

    fn parse_js(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
    ) {
        let r = JavaScriptParser::new()
            .parse(Path::new("test.js"), src)
            .unwrap();
        (r.nodes, r.edges)
    }

    #[test]
    fn parses_ts_function() {
        let (nodes, _) = parse_ts("function greet(name: string): string { return name; }");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, NodeKind::Function);
        assert_eq!(nodes[0].name, "greet");
    }

    #[test]
    fn parses_ts_class_and_method() {
        let src = "class Person { greet(): string { return 'hi'; } }";
        let (nodes, edges) = parse_ts(src);
        let classes: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(methods.len(), 1);
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
        assert!(!contains.is_empty());
    }

    #[test]
    fn parses_ts_interface() {
        let (nodes, _) = parse_ts("interface Greeter { greet(): void; }");
        let traits: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Trait).collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].name, "Greeter");
    }

    #[test]
    fn parses_js_arrow_function() {
        let (nodes, _) = parse_js("const greet = (name) => name;");
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "greet");
    }

    #[test]
    fn detects_ts_call_edges() {
        let src = "function caller() { callee(); }\nfunction callee() {}";
        let (_, edges) = parse_ts(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1);
    }
}
