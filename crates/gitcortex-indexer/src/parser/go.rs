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
        Self {
            language: tree_sitter_go::LANGUAGE.into(),
        }
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for GoParser {
    fn extensions(&self) -> &[&str] {
        &["go"]
    }

    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
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

        let mut visitor = FileVisitor::new(path, source, tree.root_node());
        visitor.collect_names(tree.root_node());
        visitor.visit_source_file(tree.root_node());
        visitor.collect_imports(tree.root_node());
        visitor.collect_interface_assertions(tree.root_node());

        Ok(ParseResult {
            nodes: visitor.nodes,
            edges: visitor.edges,
            deferred_calls: visitor.deferred_calls,
            deferred_uses: visitor.deferred_uses,
            deferred_implements: visitor.deferred_implements,
            deferred_imports: visitor.deferred_imports,
            deferred_inherits: visitor.deferred_inherits,
            deferred_throws: Vec::new(),
            deferred_annotated: Vec::new(),
        })
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    /// NodeId of the package node (anchor for Imports edges).
    package_id: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// type name → NodeId (struct/interface)
    type_index: HashMap<String, NodeId>,
    /// function/method name → NodeId
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
    deferred_uses: Vec<(NodeId, String)>,
    deferred_implements: Vec<(NodeId, String)>,
    deferred_imports: Vec<(NodeId, String)>,
    deferred_inherits: Vec<(NodeId, String)>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str, root: TsNode<'_>) -> Self {
        let package_id = NodeId::new();
        // Extract the package name from the source (first package_clause in the tree).
        let package_name = {
            let mut c = root.walk();
            let pkg_clause: Vec<TsNode<'_>> = root.named_children(&mut c).collect();
            let name = pkg_clause
                .iter()
                .find(|n| n.kind() == "package_clause")
                .and_then(|pc| {
                    let mut cc = pc.walk();
                    let ids: Vec<TsNode<'_>> = pc.named_children(&mut cc).collect();
                    ids.into_iter()
                        .find(|n| n.kind() == "package_identifier")
                        .map(|n| n.utf8_text(source.as_bytes()).unwrap_or("main").to_owned())
                })
                .unwrap_or_else(|| {
                    file.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("main")
                        .to_owned()
                });
            name
        };
        let package_node = Node {
            id: package_id.clone(),
            qualified_name: package_name.clone(),
            kind: NodeKind::Module,
            name: package_name,
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
        let nodes = vec![package_node];
        Self {
            source: source.as_bytes(),
            file: file.to_owned(),
            package_id,
            nodes,
            edges: Vec::new(),
            type_index: HashMap::new(),
            fn_index: HashMap::new(),
            deferred_calls: Vec::new(),
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_imports: Vec::new(),
            deferred_inherits: Vec::new(),
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
        if name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
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
            name: name.clone(),
            file: self.file.clone(),
            span: Self::span(ts_node),
            metadata: NodeMetadata {
                loc: (ts_node.end_position().row - ts_node.start_position().row + 1) as u32,
                visibility: Self::visibility(&name),
                is_async: false,
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
            match child.kind() {
                "function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_default();
                    }
                }
                "method_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_default();
                    }
                }
                "type_declaration" => {
                    self.collect_type_decl_names(child);
                }
                _ => {}
            }
        }
    }

    fn collect_type_decl_names(&mut self, decl: TsNode<'_>) {
        let mut cursor = decl.walk();
        for spec in decl.named_children(&mut cursor) {
            if spec.kind() != "type_spec" {
                continue;
            }
            if let Some(name_node) = spec.child_by_field_name("name") {
                let name = self.text(name_node).to_owned();
                if let Some(type_node) = spec.child_by_field_name("type") {
                    if matches!(type_node.kind(), "struct_type" | "interface_type") {
                        self.type_index.entry(name).or_default();
                    }
                }
            }
        }
    }

    // ── Pass 2: emit nodes + edges ────────────────────────────────────────────

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
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .fn_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let mut graph_node =
            self.make_node(id.clone(), NodeKind::Function, name.clone(), scope, node);

        // init and main are package-level entry points — mark them as static.
        if name == "init" || name == "main" {
            graph_node.metadata.is_static = true;
        }

        // Capture generic type parameter constraints (Go 1.18+).
        // tree-sitter-go uses "type_parameters" for `func Foo[T any, U comparable]()`.
        graph_node.metadata.generic_bounds = self.collect_generic_bounds(node);

        self.nodes.push(graph_node);

        self.extract_fn_type_uses(node, &id);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_method(&mut self, node: TsNode<'_>) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();

        let receiver_type = self.receiver_type(node);
        let scope: Vec<String> = receiver_type.into_iter().collect();

        let container_id = scope.first().and_then(|t| self.type_index.get(t).cloned());
        let id = self
            .fn_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Method, name, &scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);

        self.extract_fn_type_uses(node, &id);

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    /// Extract the receiver type name from `func (r *ReceiverType) MethodName()`.
    fn receiver_type(&self, method_node: TsNode<'_>) -> Option<String> {
        let recv = method_node.child_by_field_name("receiver")?;
        let mut cursor = recv.walk();
        for param in recv.named_children(&mut cursor) {
            if param.kind() != "parameter_declaration" {
                continue;
            }
            if let Some(type_node) = param.child_by_field_name("type") {
                return match type_node.kind() {
                    "type_identifier" => Some(self.text(type_node).to_owned()),
                    "pointer_type" => {
                        let mut c = type_node.walk();
                        let result = type_node
                            .named_children(&mut c)
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
            if spec.kind() != "type_spec" {
                continue;
            }
            let Some(name_node) = spec.child_by_field_name("name") else {
                continue;
            };
            let name = self.text(name_node).to_owned();
            let Some(type_node) = spec.child_by_field_name("type") else {
                continue;
            };

            match type_node.kind() {
                "struct_type" => {
                    let id = self
                        .type_index
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(NodeId::new);
                    let mut graph_node =
                        self.make_node(id.clone(), NodeKind::Struct, name, &[], spec);
                    // Capture generic type parameter constraints (Go 1.18+).
                    graph_node.metadata.generic_bounds = self.collect_generic_bounds(spec);
                    self.nodes.push(graph_node);
                    // Struct field types → Uses edges; embedded fields → Inherits
                    self.extract_struct_field_uses(type_node, &id);
                }
                "interface_type" => {
                    let id = self
                        .type_index
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(NodeId::new);
                    let mut graph_node =
                        self.make_node(id.clone(), NodeKind::Trait, name, &[], spec);
                    // Capture generic type parameter constraints (Go 1.18+).
                    graph_node.metadata.generic_bounds = self.collect_generic_bounds(spec);
                    self.nodes.push(graph_node);
                    // Interface method signatures → Method nodes
                    self.extract_interface_methods(type_node, &id);
                }
                _ => {
                    let id = NodeId::new();
                    let graph_node = self.make_node(id, NodeKind::TypeAlias, name, &[], spec);
                    self.nodes.push(graph_node);
                }
            }
        }
    }

    fn visit_const_decl(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        for spec in node.named_children(&mut cursor) {
            if spec.kind() != "const_spec" {
                continue;
            }
            let Some(name_node) = spec.child_by_field_name("name") else {
                continue;
            };
            let name = self.text(name_node).to_owned();
            let id = NodeId::new();
            let mut graph_node = self.make_node(id, NodeKind::Constant, name, &[], spec);
            graph_node.metadata.is_const = true;
            self.nodes.push(graph_node);
        }
    }

    // ── Pass 3: collect import declarations ───────────────────────────────────

    fn collect_imports(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() != "import_declaration" {
                continue;
            }
            // import_declaration contains import_spec or import_spec_list
            let mut c = child.walk();
            let decl_children: Vec<TsNode<'_>> = child.named_children(&mut c).collect();
            for dc in decl_children {
                match dc.kind() {
                    "import_spec" => self.record_import_spec(dc),
                    "import_spec_list" => {
                        let mut cc = dc.walk();
                        let specs: Vec<TsNode<'_>> = dc.named_children(&mut cc).collect();
                        for spec in specs {
                            if spec.kind() == "import_spec" {
                                self.record_import_spec(spec);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn record_import_spec(&mut self, spec: TsNode<'_>) {
        // If there's an explicit alias (name field), use it. Otherwise derive from path.
        let alias = spec
            .child_by_field_name("name")
            .map(|n| self.text(n).to_owned());

        // Skip blank imports (`import _ "pkg"`)
        if alias.as_deref() == Some("_") {
            return;
        }

        let pkg_name = if let Some(alias) = alias {
            alias
        } else if let Some(path_node) = spec.child_by_field_name("path") {
            // Derive package name from the last path segment, stripping quotes.
            let raw = self.text(path_node).trim_matches('"').trim_matches('\'');
            raw.split('/').next_back().unwrap_or(raw).to_owned()
        } else {
            return;
        };

        self.deferred_imports
            .push((self.package_id.clone(), pkg_name));
    }

    // ── Pass 4: detect explicit interface assertions ──────────────────────────

    /// Detect `var _ MyInterface = (*MyStruct)(nil)` patterns.
    fn collect_interface_assertions(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() != "var_declaration" {
                continue;
            }
            let mut c = child.walk();
            let specs: Vec<TsNode<'_>> = child.named_children(&mut c).collect();
            for spec in specs {
                if spec.kind() != "var_spec" {
                    continue;
                }
                let mut cc = spec.walk();
                let spec_children: Vec<TsNode<'_>> = spec.named_children(&mut cc).collect();
                // Expect: identifier "_", type_identifier (interface), expression_list (value)
                if spec_children.len() < 3 {
                    continue;
                }
                if spec_children[0].kind() != "identifier" || self.text(spec_children[0]) != "_" {
                    continue;
                }
                if spec_children[1].kind() != "type_identifier" {
                    continue;
                }
                let interface_name = self.text(spec_children[1]).to_owned();
                // Walk the value expression to find identifiers matching known types
                let value = spec_children[2];
                let mut candidates = Vec::new();
                self.collect_candidate_type_names(value, &mut candidates);
                for struct_name in candidates {
                    if let Some(struct_id) = self.type_index.get(&struct_name).cloned() {
                        self.deferred_implements
                            .push((struct_id, interface_name.clone()));
                    }
                }
            }
        }
    }

    /// Recursively collect identifier/type_identifier names that could be type names.
    fn collect_candidate_type_names(&self, node: TsNode<'_>, out: &mut Vec<String>) {
        match node.kind() {
            "identifier" | "type_identifier" => {
                let name = self.text(node).to_owned();
                if name != "nil" && !is_builtin_go_type(&name) {
                    out.push(name);
                }
            }
            _ => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    self.collect_candidate_type_names(child, out);
                }
            }
        }
    }

    // ── Type extraction helpers ───────────────────────────────────────────────

    /// Extract Uses edges from a function/method's parameter list and result type.
    fn extract_fn_type_uses(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        // Parameters
        if let Some(params) = fn_node.child_by_field_name("parameters") {
            let mut c = params.walk();
            let param_list: Vec<TsNode<'_>> = params.named_children(&mut c).collect();
            for param in param_list {
                if param.kind() == "parameter_declaration"
                    || param.kind() == "variadic_parameter_declaration"
                {
                    if let Some(type_node) = param.child_by_field_name("type") {
                        for name in self.collect_type_idents(type_node) {
                            self.deferred_uses.push((fn_id.clone(), name));
                        }
                    }
                }
            }
        }
        // Result (return types)
        if let Some(result) = fn_node.child_by_field_name("result") {
            match result.kind() {
                "parameter_list" => {
                    let mut c = result.walk();
                    let ret_params: Vec<TsNode<'_>> = result.named_children(&mut c).collect();
                    for rp in ret_params {
                        if rp.kind() == "parameter_declaration" {
                            if let Some(type_node) = rp.child_by_field_name("type") {
                                for name in self.collect_type_idents(type_node) {
                                    self.deferred_uses.push((fn_id.clone(), name));
                                }
                            }
                        }
                    }
                }
                // Single return type (no parens)
                _ => {
                    for name in self.collect_type_idents(result) {
                        self.deferred_uses.push((fn_id.clone(), name));
                    }
                }
            }
        }
    }

    /// Extract Uses edges from struct field types.
    /// Embedded (anonymous) fields also produce Inherits edges.
    fn extract_struct_field_uses(&mut self, struct_type: TsNode<'_>, struct_id: &NodeId) {
        let mut tw = struct_type.walk();
        let top: Vec<TsNode<'_>> = struct_type.named_children(&mut tw).collect();
        let Some(field_list) = top
            .iter()
            .find(|n| n.kind() == "field_declaration_list")
            .copied()
        else {
            return;
        };
        let mut c = field_list.walk();
        let fields: Vec<TsNode<'_>> = field_list.named_children(&mut c).collect();
        for field in fields {
            if field.kind() == "field_declaration" {
                // An embedded (anonymous) field has no "name" field in tree-sitter-go —
                // only a "type" field. Detect this by checking that there are no named
                // children with field-name "name".
                let has_name = field.child_by_field_name("name").is_some();
                if let Some(type_node) = field.child_by_field_name("type") {
                    let type_names = self.collect_type_idents(type_node);
                    for name in &type_names {
                        self.deferred_uses.push((struct_id.clone(), name.clone()));
                    }
                    // Embedded field (no explicit name) → structural inheritance
                    if !has_name {
                        for name in type_names {
                            self.deferred_inherits.push((struct_id.clone(), name));
                        }
                    }
                }
            }
        }
    }

    /// Capture method signatures from an interface body as Method nodes.
    fn extract_interface_methods(&mut self, interface_type: TsNode<'_>, iface_id: &NodeId) {
        let mut c = interface_type.walk();
        let children: Vec<TsNode<'_>> = interface_type.named_children(&mut c).collect();
        for child in children {
            // tree-sitter-go uses `method_elem` for interface method signatures
            if child.kind() == "method_elem" {
                let mut cc = child.walk();
                let method_children: Vec<TsNode<'_>> = child.named_children(&mut cc).collect();
                // Name is the first `field_identifier` child
                let Some(name_node) = method_children
                    .iter()
                    .find(|n| n.kind() == "field_identifier")
                else {
                    continue;
                };
                let name = self.text(*name_node).to_owned();
                let id = NodeId::new();
                let graph_node = self.make_node(id.clone(), NodeKind::Method, name, &[], child);
                self.edges.push(Edge {
                    src: iface_id.clone(),
                    dst: id.clone(),
                    kind: EdgeKind::Contains,
                });
                self.nodes.push(graph_node);
            }
        }
    }

    /// Parse `type_parameters` of a generic function or type declaration (Go 1.18+).
    ///
    /// For `func Map[T any, U comparable]()` this returns `["T any", "U comparable"]`.
    /// For `type Set[E comparable] struct {}` this returns `["E comparable"]`.
    fn collect_generic_bounds(&self, node: TsNode<'_>) -> Vec<String> {
        let Some(type_params) = node.child_by_field_name("type_parameters") else {
            return Vec::new();
        };
        let mut bounds = Vec::new();
        let mut cursor = type_params.walk();
        for child in type_params.named_children(&mut cursor) {
            // tree-sitter-go models each type parameter as a `type_parameter_declaration`
            // with a "name" field (the type variable) and a "type" field (the constraint).
            if child.kind() == "type_parameter_declaration" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| self.text(n))
                    .unwrap_or("");
                let constraint = child
                    .child_by_field_name("type")
                    .map(|n| self.text(n))
                    .unwrap_or("");
                if !name.is_empty() {
                    let bound = if constraint.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{name} {constraint}")
                    };
                    bounds.push(bound);
                }
            }
        }
        bounds
    }

    /// Walk a Go type expression and collect non-builtin type_identifier names.
    fn collect_type_idents(&self, node: TsNode<'_>) -> Vec<String> {
        let mut names = Vec::new();
        self.walk_type_idents(node, &mut names);
        names
    }

    fn walk_type_idents(&self, node: TsNode<'_>, out: &mut Vec<String>) {
        match node.kind() {
            "type_identifier" => {
                let name = self.text(node).to_owned();
                if !is_builtin_go_type(&name) {
                    out.push(name);
                }
            }
            _ => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    self.walk_type_idents(child, out);
                }
            }
        }
    }

    // ── Call collection ───────────────────────────────────────────────────────

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
            } else if child.kind() == "go_statement" {
                // `go fn()` — record the call and mark it as async via deferred_calls
                if let Some(call) = child.named_child(0) {
                    if call.kind() == "call_expression" {
                        if let Some(callee) = self.callee_name(call) {
                            // Record as a regular deferred call; the goroutine is conceptually async
                            self.deferred_calls.push((caller_id.clone(), callee));
                        }
                    }
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

/// Returns true for Go built-in types that don't correspond to user-defined symbols.
fn is_builtin_go_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "byte"
            | "complex64"
            | "complex128"
            | "error"
            | "float32"
            | "float64"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "rune"
            | "string"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
            | "any"
            | "comparable"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::GoParser;
    use crate::parser::LanguageParser;
    use gitcortex_core::schema::{EdgeKind, NodeKind};
    use std::path::Path;

    fn parse(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
    ) {
        let r = GoParser::new().parse(Path::new("test.go"), src).unwrap();
        (r.nodes, r.edges)
    }

    fn parse_full(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
    ) {
        let r = GoParser::new().parse(Path::new("test.go"), src).unwrap();
        (
            r.nodes,
            r.edges,
            r.deferred_calls,
            r.deferred_uses,
            r.deferred_implements,
            r.deferred_imports,
        )
    }

    #[test]
    fn parses_function() {
        let src = "package main\nfunc Greet(name string) string { return name }";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "Greet");
    }

    #[test]
    fn parses_struct_and_method() {
        let src = "package main\ntype Person struct { Name string }\nfunc (p *Person) Greet() string { return p.Name }";
        let (nodes, edges) = parse(src);
        let structs: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(methods.len(), 1);
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
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

    #[test]
    fn package_node_is_emitted() {
        let src = "package mypackage\nfunc Foo() {}";
        let (nodes, _) = parse(src);
        let modules: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "mypackage");
    }

    #[test]
    fn detects_import_declaration() {
        let src = "package main\nimport (\n\t\"fmt\"\n\t\"os/exec\"\n)\nfunc main() {}";
        let (_, _, _, _, _, imports) = parse_full(src);
        assert!(
            imports.iter().any(|(_, n)| n == "fmt"),
            "expected import 'fmt', got: {imports:?}"
        );
        assert!(
            imports.iter().any(|(_, n)| n == "exec"),
            "expected import 'exec' (last segment of os/exec), got: {imports:?}"
        );
    }

    #[test]
    fn detects_fn_type_uses() {
        let src = "package main\ntype Request struct{}\ntype Response struct{}\nfunc Handle(req *Request) *Response { return nil }";
        let (_, _, _, uses, _, _) = parse_full(src);
        assert!(
            uses.iter().any(|(_, n)| n == "Request"),
            "expected Uses edge to Request, got: {uses:?}"
        );
        assert!(
            uses.iter().any(|(_, n)| n == "Response"),
            "expected Uses edge to Response, got: {uses:?}"
        );
    }

    #[test]
    fn detects_interface_assertion() {
        let src = "package main\ntype Greeter interface { Greet() string }\ntype Person struct{}\nvar _ Greeter = (*Person)(nil)";
        let (_, _, _, _, implements, _) = parse_full(src);
        assert!(
            implements.iter().any(|(_, n)| n == "Greeter"),
            "expected Implements edge to Greeter, got: {implements:?}"
        );
    }

    #[test]
    fn captures_interface_methods() {
        let src = "package main\ntype Greeter interface { Greet() string\nGetName() string }";
        let (nodes, edges) = parse(src);
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 2, "expected 2 interface method specs");
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
        assert_eq!(
            contains.len(),
            2,
            "expected 2 Contains edges from interface to methods"
        );
    }
}
