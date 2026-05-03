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
    visitor.collect_imports(tree.root_node());

    Ok(ParseResult {
        nodes: visitor.nodes,
        edges: visitor.edges,
        deferred_calls: visitor.deferred_calls,
        deferred_uses: visitor.deferred_uses,
        deferred_implements: visitor.deferred_implements,
        deferred_inherits: visitor.deferred_inherits,
        deferred_throws: Vec::new(),
        deferred_annotated: visitor.deferred_annotated,
        deferred_imports: visitor.deferred_imports,
    })
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    /// NodeId of the file-level Module node (anchor for Imports edges).
    module_id: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// class/interface/type name → NodeId
    type_index: HashMap<String, NodeId>,
    /// function/method name → NodeId
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
    deferred_uses: Vec<(NodeId, String)>,
    deferred_implements: Vec<(NodeId, String)>,
    deferred_inherits: Vec<(NodeId, String)>,
    deferred_annotated: Vec<(NodeId, String)>,
    deferred_imports: Vec<(NodeId, String)>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str) -> Self {
        let module_id = NodeId::new();
        let module_name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("index")
            .to_owned();
        let module_node = Node {
            id: module_id.clone(),
            qualified_name: module_name.clone(),
            kind: NodeKind::Module,
            name: module_name,
            file: file.to_owned(),
            span: Span {
                start_line: 1,
                end_line: 1,
            },
            metadata: NodeMetadata {
                loc: source.lines().count() as u32,
                visibility: Visibility::Pub,
                is_async: false,
                is_unsafe: false,
                ..Default::default()
            },
        };
        let nodes = vec![module_node];
        Self {
            source: source.as_bytes(),
            file: file.to_owned(),
            module_id,
            nodes,
            edges: Vec::new(),
            type_index: HashMap::new(),
            fn_index: HashMap::new(),
            deferred_calls: Vec::new(),
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_inherits: Vec::new(),
            deferred_annotated: Vec::new(),
            deferred_imports: Vec::new(),
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
        Visibility::Pub
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
            "internal_module" | "module" => {
                self.visit_namespace(node, scope);
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
        let mut graph_node = self.make_node(id.clone(), kind, name, scope, node);
        if node.kind() == "generator_function_declaration" {
            graph_node.metadata.is_generator = true;
        }

        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);

        self.extract_param_types(node, &id);
        self.extract_return_type_annotation(node, &id);
        self.extract_generic_constraints(node, &id);
        self.extract_decorator_annotated(node, &id);

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
        let mut graph_node =
            self.make_node(id.clone(), NodeKind::Struct, name.clone(), scope, node);
        if node.kind() == "abstract_class_declaration" {
            graph_node.metadata.is_abstract = true;
        }
        self.nodes.push(graph_node);

        // extends → Inherits edges; implements → Implements edges
        self.extract_heritage(node, &id);
        // generic constraints → Uses edges
        self.extract_generic_constraints(node, &id);
        // class-level decorators (@Injectable, @Component, etc.) → Annotated edges
        self.extract_decorator_annotated(node, &id);

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

        self.extract_param_types(node, &id);
        self.extract_return_type_annotation(node, &id);
        self.extract_decorator_uses(node, &id);

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

        // Interface extends other interfaces → Implements edges
        self.extract_heritage(node, &id);

        // Capture method signatures defined inside the interface body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            for child in children {
                if matches!(
                    child.kind(),
                    "method_signature" | "call_signature" | "construct_signature"
                ) {
                    self.visit_method(child, scope, id.clone());
                }
            }
        }
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
        let enum_id = NodeId::new();
        let graph_node = self.make_node(enum_id.clone(), NodeKind::Enum, name.clone(), scope, node);
        self.nodes.push(graph_node);

        // Emit EnumMember nodes for each variant
        let member_scope: Vec<String> = scope.iter().cloned().chain(std::iter::once(name)).collect();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for member in body.named_children(&mut cursor) {
                if member.kind() != "enum_assignment" && member.kind() != "property_identifier" {
                    continue;
                }
                let member_name_node = if member.kind() == "enum_assignment" {
                    member.child_by_field_name("name").or_else(|| member.named_child(0))
                } else {
                    Some(member)
                };
                if let Some(mn) = member_name_node {
                    let mname = self.text(mn).to_owned();
                    let mid = NodeId::new();
                    let mnode = self.make_node(mid.clone(), NodeKind::EnumMember, mname, &member_scope, member);
                    self.nodes.push(mnode);
                    self.edges.push(Edge { src: enum_id.clone(), dst: mid, kind: EdgeKind::Contains });
                }
            }
        }
    }

    fn visit_namespace(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let ns_id = NodeId::new();
        let graph_node = self.make_node(ns_id.clone(), NodeKind::Module, name.clone(), scope, node);
        self.nodes.push(graph_node);
        self.edges.push(Edge { src: self.module_id.clone(), dst: ns_id.clone(), kind: EdgeKind::Contains });

        // Visit namespace body
        if let Some(body) = node.child_by_field_name("body") {
            let child_scope: Vec<String> = scope.iter().cloned().chain(std::iter::once(name)).collect();
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            for child in children {
                let actual = self.unwrap_export(child);
                self.visit_statement(actual, &child_scope, Some(ns_id.clone()));
            }
        }
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
                    self.extract_param_types(value, &id);
                    self.extract_return_type_annotation(value, &id);
                    if let Some(body) = value.child_by_field_name("body") {
                        self.collect_calls(body, &id);
                    }
                }
                _ => {
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

    // ── Pass 3: collect import statements ────────────────────────────────────

    fn collect_imports(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for stmt in children {
            if stmt.kind() != "import_statement" {
                continue;
            }
            // import_clause is a named child of import_statement (not a field)
            let mut c = stmt.walk();
            let clause_children: Vec<TsNode<'_>> = stmt.named_children(&mut c).collect();
            let Some(import_clause) = clause_children
                .iter()
                .find(|n| n.kind() == "import_clause")
                .copied()
            else {
                continue;
            };

            // import_clause contains: identifier (default), namespace_import, named_imports
            let mut cc = import_clause.walk();
            let clause_members: Vec<TsNode<'_>> =
                import_clause.named_children(&mut cc).collect();
            for member in clause_members {
                match member.kind() {
                    "identifier" => {
                        // import DefaultExport from 'module'
                        let name = self.text(member).to_owned();
                        self.deferred_imports.push((self.module_id.clone(), name));
                    }
                    "namespace_import" => {
                        // import * as x from 'module' — binding is the identifier
                        let mut c2 = member.walk();
                        let idents: Vec<TsNode<'_>> =
                            member.named_children(&mut c2).collect();
                        if let Some(ident) =
                            idents.iter().find(|n| n.kind() == "identifier").copied()
                        {
                            let name = self.text(ident).to_owned();
                            self.deferred_imports.push((self.module_id.clone(), name));
                        }
                    }
                    "named_imports" => {
                        // import { foo, bar as baz } from 'module'
                        let mut c2 = member.walk();
                        let specifiers: Vec<TsNode<'_>> =
                            member.named_children(&mut c2).collect();
                        for specifier in specifiers {
                            if specifier.kind() == "import_specifier" {
                                let orig =
                                    specifier.child_by_field_name("name").unwrap_or(specifier);
                                let name = self.text(orig).to_owned();
                                self.deferred_imports.push((self.module_id.clone(), name));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Type annotation helpers ───────────────────────────────────────────────

    /// Extract Uses edges from function/method parameter type annotations.
    /// Emit Uses edges for generic type constraints: `function foo<T extends Bar>()` → Uses Bar.
    fn extract_generic_constraints(&mut self, node: TsNode<'_>, id: &NodeId) {
        let Some(type_params) = node.child_by_field_name("type_parameters") else {
            return;
        };
        let mut cursor = type_params.walk();
        for param in type_params.named_children(&mut cursor) {
            if param.kind() != "type_parameter" {
                continue;
            }
            if let Some(constraint) = param.child_by_field_name("constraint") {
                for name in self.collect_type_names(constraint) {
                    self.deferred_uses.push((id.clone(), name));
                }
            }
        }
    }

    fn extract_param_types(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        let Some(params) = fn_node.child_by_field_name("parameters") else {
            return;
        };
        let mut c = params.walk();
        for param in params.named_children(&mut c) {
            // required_parameter, optional_parameter, rest_parameter all have a `type` field
            if let Some(type_ann) = param.child_by_field_name("type") {
                for name in self.collect_type_names(type_ann) {
                    self.deferred_uses.push((fn_id.clone(), name));
                }
            }
        }
    }

    /// Extract a Uses edge from the return type annotation.
    fn extract_return_type_annotation(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        if let Some(ret) = fn_node.child_by_field_name("return_type") {
            for name in self.collect_type_names(ret) {
                self.deferred_uses.push((fn_id.clone(), name));
            }
        }
    }

    /// Extract Implements edges from `extends_clause` and `implements_clause`.
    /// Handles both the class case (`class_heritage` wrapper) and the interface
    /// case (direct `extends_clause` child).
    fn extract_heritage(&mut self, node: TsNode<'_>, node_id: &NodeId) {
        let mut c = node.walk();
        let top: Vec<TsNode<'_>> = node.named_children(&mut c).collect();
        for child in top {
            match child.kind() {
                "extends_clause" => self.collect_extends_names(child, node_id),
                "implements_clause" => self.collect_implements_names(child, node_id),
                "class_heritage" => {
                    let mut cc = child.walk();
                    let heritage: Vec<TsNode<'_>> = child.named_children(&mut cc).collect();
                    for hc in heritage {
                        match hc.kind() {
                            "extends_clause" => self.collect_extends_names(hc, node_id),
                            "implements_clause" => self.collect_implements_names(hc, node_id),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_extends_names(&mut self, extends_clause: TsNode<'_>, node_id: &NodeId) {
        let mut c = extends_clause.walk();
        let children: Vec<TsNode<'_>> = extends_clause.named_children(&mut c).collect();
        for ext in children {
            let type_name = match ext.kind() {
                "identifier" | "type_identifier" => Some(self.text(ext).to_owned()),
                "generic_type" => ext
                    .child_by_field_name("name")
                    .map(|n| self.text(n).to_owned()),
                "member_expression" => ext
                    .child_by_field_name("property")
                    .map(|n| self.text(n).to_owned()),
                _ => None,
            };
            if let Some(name) = type_name {
                // `extends` is always inheritance (is-a): class→class or interface→interface.
                self.deferred_inherits.push((node_id.clone(), name));
            }
        }
    }

    fn collect_implements_names(&mut self, implements_clause: TsNode<'_>, node_id: &NodeId) {
        let mut c = implements_clause.walk();
        let children: Vec<TsNode<'_>> = implements_clause.named_children(&mut c).collect();
        for iface in children {
            let type_name = match iface.kind() {
                "type_identifier" | "identifier" => Some(self.text(iface).to_owned()),
                "generic_type" => iface
                    .child_by_field_name("name")
                    .map(|n| self.text(n).to_owned()),
                _ => None,
            };
            if let Some(name) = type_name {
                self.deferred_implements.push((node_id.clone(), name));
            }
        }
    }

    /// Emit `deferred_annotated` entries for each decorator on a node.
    fn extract_decorator_annotated(&mut self, node: TsNode<'_>, node_id: &NodeId) {
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            if child.kind() == "decorator" {
                if let Some(name) = self.decorator_name(child) {
                    self.deferred_annotated.push((node_id.clone(), name));
                }
            }
        }
    }

    /// Extract Uses edges from decorator nodes on a class, method, or function.
    fn extract_decorator_uses(&mut self, node: TsNode<'_>, node_id: &NodeId) {
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            if child.kind() == "decorator" {
                if let Some(name) = self.decorator_name(child) {
                    self.deferred_uses.push((node_id.clone(), name));
                }
            }
        }
    }

    /// Extract the callable name from a `decorator` node.
    fn decorator_name(&self, decorator: TsNode<'_>) -> Option<String> {
        let mut c = decorator.walk();
        let child = decorator.named_children(&mut c).next()?;
        match child.kind() {
            "identifier" => Some(self.text(child).to_owned()),
            "member_expression" => child
                .child_by_field_name("property")
                .map(|n| self.text(n).to_owned()),
            "call_expression" => child.child_by_field_name("function").and_then(|f| {
                match f.kind() {
                    "identifier" => Some(self.text(f).to_owned()),
                    "member_expression" => f
                        .child_by_field_name("property")
                        .map(|n| self.text(n).to_owned()),
                    _ => None,
                }
            }),
            _ => None,
        }
    }

    /// Walk a type expression and collect non-builtin type identifiers.
    fn collect_type_names(&self, node: TsNode<'_>) -> Vec<String> {
        let mut names = Vec::new();
        self.walk_type_names(node, &mut names);
        names
    }

    fn walk_type_names(&self, node: TsNode<'_>, out: &mut Vec<String>) {
        match node.kind() {
            "type_identifier" => {
                let name = self.text(node).to_owned();
                if !is_builtin_ts_type(&name) {
                    out.push(name);
                }
            }
            // Skip predefined types (string, number, boolean, void, etc.)
            "predefined_type" => {}
            _ => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    self.walk_type_names(child, out);
                }
            }
        }
    }

    // ── Call collection ───────────────────────────────────────────────────────

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

/// Returns true for TypeScript/JavaScript built-in type names.
fn is_builtin_ts_type(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "number"
            | "boolean"
            | "void"
            | "any"
            | "unknown"
            | "never"
            | "null"
            | "undefined"
            | "object"
            | "symbol"
            | "bigint"
            | "Array"
            | "Object"
            | "Function"
            | "Promise"
            | "ReadonlyArray"
            | "Readonly"
            | "Partial"
            | "Required"
            | "Record"
            | "Pick"
            | "Omit"
            | "Exclude"
            | "Extract"
            | "NonNullable"
            | "ReturnType"
            | "InstanceType"
            | "Parameters"
            | "ConstructorParameters"
            | "ThisType"
            | "Map"
            | "Set"
            | "WeakMap"
            | "WeakSet"
            | "Iterator"
            | "IterableIterator"
            | "Generator"
            | "AsyncGenerator"
    )
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

    fn parse_ts_full(src: &str) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
    ) {
        let r = TypeScriptParser::new_ts()
            .parse(Path::new("test.ts"), src)
            .unwrap();
        (r.nodes, r.edges, r.deferred_calls, r.deferred_uses, r.deferred_implements, r.deferred_imports, r.deferred_inherits)
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
        // Module node + Function node
        let fns: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Function).collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "greet");
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

    #[test]
    fn detects_ts_extends_implements() {
        let src = "interface Base {}\nclass Foo extends Base implements Base {}";
        let (_, _, _, _, implements, _, inherits) = parse_ts_full(src);
        let impl_bases: Vec<_> = implements.iter().filter(|(_, n)| n == "Base").collect();
        let inh_bases: Vec<_> = inherits.iter().filter(|(_, n)| n == "Base").collect();
        assert!(
            impl_bases.len() + inh_bases.len() >= 2,
            "expected extends (inherits) + implements edges to Base, implements={implements:?} inherits={inherits:?}"
        );
    }

    #[test]
    fn detects_ts_type_annotation_uses() {
        let src = "class MyService {}\nfunction process(svc: MyService): MyService { return svc; }";
        let (_, _, _, uses, ..) = parse_ts_full(src);
        let svc_uses: Vec<_> = uses.iter().filter(|(_, n)| n == "MyService").collect();
        assert!(
            svc_uses.len() >= 2,
            "expected Uses edges to MyService (param + return), got: {uses:?}"
        );
    }

    #[test]
    fn detects_ts_named_imports() {
        let src = "import { Component, Injectable } from '@angular/core';\nfunction foo() {}";
        let (_, _, _, _, _, imports, ..) = parse_ts_full(src);
        assert!(
            imports.iter().any(|(_, n)| n == "Component"),
            "expected import 'Component', got: {imports:?}"
        );
        assert!(
            imports.iter().any(|(_, n)| n == "Injectable"),
            "expected import 'Injectable', got: {imports:?}"
        );
    }

    #[test]
    fn module_node_is_emitted() {
        let (nodes, _) = parse_ts("const x = 1;");
        let modules: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Module).collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "test");
    }
}
