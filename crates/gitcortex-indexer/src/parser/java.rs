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

pub struct JavaParser {
    language: tree_sitter::Language,
}

impl JavaParser {
    pub fn new() -> Self {
        Self {
            language: tree_sitter_java::LANGUAGE.into(),
        }
    }
}

impl Default for JavaParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for JavaParser {
    fn extensions(&self) -> &[&str] {
        &["java"]
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

        let mut visitor = FileVisitor::new(path, source);
        visitor.collect_names(tree.root_node());
        visitor.visit_program(tree.root_node());
        visitor.collect_imports(tree.root_node());

        Ok(ParseResult {
            nodes: visitor.nodes,
            edges: visitor.edges,
            deferred_calls: visitor.deferred_calls,
            deferred_uses: visitor.deferred_uses,
            deferred_implements: visitor.deferred_implements,
            deferred_imports: visitor.deferred_imports,
            deferred_inherits: visitor.deferred_inherits,
            deferred_throws: visitor.deferred_throws,
            deferred_annotated: visitor.deferred_annotated,
        })
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    /// NodeId of the file-level package node (anchor for Imports edges).
    package_id: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// class/interface/enum name → NodeId
    type_index: HashMap<String, NodeId>,
    /// method name → NodeId
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
    deferred_uses: Vec<(NodeId, String)>,
    deferred_implements: Vec<(NodeId, String)>,
    deferred_imports: Vec<(NodeId, String)>,
    deferred_inherits: Vec<(NodeId, String)>,
    deferred_throws: Vec<(NodeId, String)>,
    deferred_annotated: Vec<(NodeId, String)>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str) -> Self {
        let package_id = NodeId::new();
        // Use the file stem as the compilation unit name.
        let unit_name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_owned();
        let package_node = Node {
            id: package_id.clone(),
            qualified_name: unit_name.clone(),
            kind: NodeKind::Module,
            name: unit_name,
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
            deferred_throws: Vec::new(),
            deferred_annotated: Vec::new(),
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
            if child.kind() == "modifiers" {
                let text = child.utf8_text(source).unwrap_or("");
                if text.contains("public") {
                    return Visibility::Pub;
                }
                if text.contains("protected") {
                    return Visibility::PubCrate;
                }
                return Visibility::Private;
            }
        }
        // Package-private (no modifier) = effectively PubCrate within package
        Visibility::PubCrate
    }

    fn is_async(_node: TsNode<'_>) -> bool {
        // Java doesn't have async/await; treat `synchronized` as a proxy for async
        false
    }

    /// Returns the text of the `modifiers` child node, or `""` if absent.
    fn modifiers_text<'t>(node: TsNode<'t>, source: &'t [u8]) -> &'t str {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                return child.utf8_text(source).unwrap_or("");
            }
        }
        ""
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
        let mods = Self::modifiers_text(ts_node, self.source);
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
                is_abstract: mods.contains("abstract"),
                is_final: mods.contains("final"),
                is_static: mods.contains("static"),
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
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration"
                | "record_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.type_index.entry(name).or_default();
                    }
                }
                _ => {}
            }
        }
    }

    // ── Pass 2: emit nodes + edges ────────────────────────────────────────────

    fn visit_program(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            self.visit_top_level(child, &[]);
        }
    }

    fn visit_top_level(&mut self, node: TsNode<'_>, scope: &[String]) {
        match node.kind() {
            "class_declaration" => self.visit_class(node, scope),
            "interface_declaration" => self.visit_interface(node, scope),
            "enum_declaration" => self.visit_enum(node, scope),
            "record_declaration" => self.visit_record(node, scope),
            _ => {}
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

        // extends (super class) → Inherits edge
        if let Some(superclass) = node.child_by_field_name("superclass") {
            let type_name = self.extract_simple_type(superclass);
            if let Some(t) = type_name {
                self.deferred_inherits.push((id.clone(), t));
            }
        }

        // implements (interfaces) → Implements edges
        if let Some(interfaces) = node.child_by_field_name("interfaces") {
            let mut c = interfaces.walk();
            let iface_children: Vec<TsNode<'_>> = interfaces.named_children(&mut c).collect();
            for iface in iface_children {
                if let Some(t) = self.extract_simple_type(iface) {
                    self.deferred_implements.push((id.clone(), t));
                }
            }
        }

        // Annotations on the class → Annotated edges
        self.extract_annotation_uses(node, &id);

        let mut class_scope = scope.to_vec();
        class_scope.push(name.clone());

        // Visit class body
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            let body_children: Vec<TsNode<'_>> = body.named_children(&mut c).collect();
            for child in body_children {
                match child.kind() {
                    "method_declaration" | "constructor_declaration" => {
                        self.visit_method(child, &class_scope, id.clone());
                    }
                    "class_declaration" => {
                        let nested_id = self.visit_class_nested(child, &class_scope);
                        if let Some(nid) = nested_id {
                            self.edges.push(Edge {
                                src: id.clone(),
                                dst: nid,
                                kind: EdgeKind::Contains,
                            });
                        }
                    }
                    "interface_declaration" => {
                        let nested_id = self.visit_interface_nested(child, &class_scope);
                        if let Some(nid) = nested_id {
                            self.edges.push(Edge {
                                src: id.clone(),
                                dst: nid,
                                kind: EdgeKind::Contains,
                            });
                        }
                    }
                    "field_declaration" => {
                        self.extract_field_uses(child, &id);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Visit a nested class declaration, returning the new node's id.
    fn visit_class_nested(&mut self, node: TsNode<'_>, scope: &[String]) -> Option<NodeId> {
        let name_node = node.child_by_field_name("name")?;
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let mut graph_node =
            self.make_node(id.clone(), NodeKind::Struct, name.clone(), scope, node);
        // make_node reads `static` from modifiers — already handled there
        let mods = Self::modifiers_text(node, self.source);
        graph_node.metadata.is_static = mods.contains("static");
        self.nodes.push(graph_node);

        if let Some(superclass) = node.child_by_field_name("superclass") {
            if let Some(t) = self.extract_simple_type(superclass) {
                self.deferred_inherits.push((id.clone(), t));
            }
        }
        if let Some(interfaces) = node.child_by_field_name("interfaces") {
            let mut c = interfaces.walk();
            for iface in interfaces.named_children(&mut c).collect::<Vec<_>>() {
                if let Some(t) = self.extract_simple_type(iface) {
                    self.deferred_implements.push((id.clone(), t));
                }
            }
        }
        self.extract_annotation_uses(node, &id);

        let mut nested_scope = scope.to_vec();
        nested_scope.push(name);
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            for child in body.named_children(&mut c).collect::<Vec<_>>() {
                if matches!(
                    child.kind(),
                    "method_declaration" | "constructor_declaration"
                ) {
                    self.visit_method(child, &nested_scope, id.clone());
                } else if child.kind() == "field_declaration" {
                    self.extract_field_uses(child, &id);
                }
            }
        }
        Some(id)
    }

    /// Visit a nested interface declaration, returning the new node's id.
    fn visit_interface_nested(&mut self, node: TsNode<'_>, scope: &[String]) -> Option<NodeId> {
        let name_node = node.child_by_field_name("name")?;
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let is_functional = self.has_functional_interface_annotation(node);
        let mut graph_node =
            self.make_node(id.clone(), NodeKind::Interface, name.clone(), scope, node);
        if is_functional {
            graph_node.metadata.is_abstract = true;
        }
        self.nodes.push(graph_node);

        if let Some(extends) = node.child_by_field_name("extends") {
            let mut c = extends.walk();
            for ext in extends.named_children(&mut c).collect::<Vec<_>>() {
                if let Some(t) = self.extract_simple_type(ext) {
                    self.deferred_implements.push((id.clone(), t));
                }
            }
        }
        self.extract_annotation_uses(node, &id);

        let mut nested_scope = scope.to_vec();
        nested_scope.push(name);
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            for child in body.named_children(&mut c).collect::<Vec<_>>() {
                if matches!(child.kind(), "method_declaration" | "constant_declaration") {
                    self.visit_method(child, &nested_scope, id.clone());
                }
            }
        }
        Some(id)
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
        let is_functional = self.has_functional_interface_annotation(node);
        let mut graph_node =
            self.make_node(id.clone(), NodeKind::Interface, name.clone(), scope, node);
        if is_functional {
            graph_node.metadata.is_abstract = true;
        }
        self.nodes.push(graph_node);

        // Annotations on the interface → Annotated edges
        self.extract_annotation_uses(node, &id);

        // extends (parent interfaces) → Implements edges
        if let Some(extends) = node.child_by_field_name("extends") {
            let mut c = extends.walk();
            let ext_children: Vec<TsNode<'_>> = extends.named_children(&mut c).collect();
            for ext in ext_children {
                if let Some(t) = self.extract_simple_type(ext) {
                    self.deferred_implements.push((id.clone(), t));
                }
            }
        }

        let mut iface_scope = scope.to_vec();
        iface_scope.push(name.clone());

        // Interface body methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            let body_children: Vec<TsNode<'_>> = body.named_children(&mut c).collect();
            for child in body_children {
                if matches!(child.kind(), "method_declaration" | "constant_declaration") {
                    self.visit_method(child, &iface_scope, id.clone());
                }
            }
        }
    }

    fn visit_enum(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Enum, name.clone(), scope, node);
        self.nodes.push(graph_node);

        // implements (interfaces) → Implements edges
        if let Some(interfaces) = node.child_by_field_name("interfaces") {
            let mut c = interfaces.walk();
            let iface_children: Vec<TsNode<'_>> = interfaces.named_children(&mut c).collect();
            for iface in iface_children {
                if let Some(t) = self.extract_simple_type(iface) {
                    self.deferred_implements.push((id.clone(), t));
                }
            }
        }

        let mut enum_scope = scope.to_vec();
        enum_scope.push(name.clone());

        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            let body_children: Vec<TsNode<'_>> = body.named_children(&mut c).collect();
            for child in body_children {
                if child.kind() == "method_declaration" {
                    self.visit_method(child, &enum_scope, id.clone());
                }
            }
        }
    }

    fn visit_record(&mut self, node: TsNode<'_>, scope: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        // Records are treated as Struct (they are essentially final data classes)
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Struct, name.clone(), scope, node);
        self.nodes.push(graph_node);

        let mut record_scope = scope.to_vec();
        record_scope.push(name);

        if let Some(body) = node.child_by_field_name("body") {
            let mut c = body.walk();
            let body_children: Vec<TsNode<'_>> = body.named_children(&mut c).collect();
            for child in body_children {
                if child.kind() == "method_declaration" {
                    self.visit_method(child, &record_scope, id.clone());
                }
            }
        }
    }

    fn visit_method(&mut self, node: TsNode<'_>, scope: &[String], container_id: NodeId) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .fn_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let kind = if node.kind() == "constructor_declaration" {
            NodeKind::Function
        } else {
            NodeKind::Method
        };
        let graph_node = self.make_node(id.clone(), kind, name, scope, node);
        self.edges.push(Edge {
            src: container_id,
            dst: id.clone(),
            kind: EdgeKind::Contains,
        });
        self.nodes.push(graph_node);

        // Parameter types → Uses edges
        if let Some(params) = node.child_by_field_name("parameters") {
            let mut c = params.walk();
            let param_list: Vec<TsNode<'_>> = params.named_children(&mut c).collect();
            for param in param_list {
                if param.kind() == "formal_parameter" || param.kind() == "spread_parameter" {
                    if let Some(type_node) = param.child_by_field_name("type") {
                        for tname in self.collect_type_names(type_node) {
                            self.deferred_uses.push((id.clone(), tname));
                        }
                    }
                }
            }
        }

        // Return type → Uses edges
        if let Some(ret) = node.child_by_field_name("type") {
            for tname in self.collect_type_names(ret) {
                self.deferred_uses.push((id.clone(), tname));
            }
        }

        // Annotations on the method → Annotated edges
        self.extract_annotation_uses(node, &id);

        // throws clause → Throws edges
        if let Some(throws) = node.child_by_field_name("throws") {
            let mut c = throws.walk();
            for exc in throws.named_children(&mut c).collect::<Vec<_>>() {
                if let Some(t) = self.extract_simple_type(exc) {
                    self.deferred_throws.push((id.clone(), t));
                }
            }
        }

        // Calls in the method body
        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
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
            // import_declaration text: `import com.example.Foo;` or `import static ...`
            // The last identifier in the import path is the leaf name.
            let raw = self.text(child);
            let clean = raw
                .trim_start_matches("import")
                .trim_start_matches(" static")
                .trim()
                .trim_end_matches(';')
                .trim();
            // Get the last segment after the last '.'
            let leaf = clean.split('.').next_back().unwrap_or(clean);
            // Skip wildcard imports (*)
            if leaf == "*" {
                continue;
            }
            self.deferred_imports
                .push((self.package_id.clone(), leaf.to_owned()));
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Extract annotations on a node as Annotated edges.
    fn extract_annotation_uses(&mut self, node: TsNode<'_>, node_id: &NodeId) {
        let mut c = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut c).collect();
        for child in children {
            if child.kind() == "modifiers" {
                let mut cc = child.walk();
                let mod_children: Vec<TsNode<'_>> = child.named_children(&mut cc).collect();
                for mc in mod_children {
                    if mc.kind() == "annotation" || mc.kind() == "marker_annotation" {
                        // Annotation name is the first named child (type_identifier or identifier)
                        let mut ccc = mc.walk();
                        let ann_children: Vec<TsNode<'_>> = mc.named_children(&mut ccc).collect();
                        if let Some(ann_name_node) = ann_children.first() {
                            let ann_name = self.text(*ann_name_node).to_owned();
                            if !ann_name.is_empty() {
                                self.deferred_annotated.push((node_id.clone(), ann_name));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns true when a node has a `@FunctionalInterface` annotation.
    fn has_functional_interface_annotation(&self, node: TsNode<'_>) -> bool {
        let mut c = node.walk();
        for child in node.named_children(&mut c).collect::<Vec<_>>() {
            if child.kind() == "modifiers" {
                let mut cc = child.walk();
                for mc in child.named_children(&mut cc).collect::<Vec<_>>() {
                    if mc.kind() == "annotation" || mc.kind() == "marker_annotation" {
                        let mut ccc = mc.walk();
                        if let Some(ann) = mc.named_children(&mut ccc).collect::<Vec<_>>().first() {
                            if self.text(*ann) == "FunctionalInterface" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Extract Uses edges from a field declaration's type.
    fn extract_field_uses(&mut self, field_decl: TsNode<'_>, container_id: &NodeId) {
        if let Some(type_node) = field_decl.child_by_field_name("type") {
            for tname in self.collect_type_names(type_node) {
                self.deferred_uses.push((container_id.clone(), tname));
            }
        }
    }

    /// Extract the simple class/interface name from a type node.
    fn extract_simple_type(&self, node: TsNode<'_>) -> Option<String> {
        match node.kind() {
            "type_identifier" | "identifier" => Some(self.text(node).to_owned()),
            "generic_type" => node
                .child_by_field_name("name")
                .map(|n| self.text(n).to_owned()),
            _ => {
                // Try first named child
                let mut c = node.walk();
                let children: Vec<TsNode<'_>> = node.named_children(&mut c).collect();
                children
                    .into_iter()
                    .find_map(|ch| self.extract_simple_type(ch))
            }
        }
    }

    /// Walk a type expression and collect non-builtin type names.
    fn collect_type_names(&self, node: TsNode<'_>) -> Vec<String> {
        let mut names = Vec::new();
        self.walk_type_names(node, &mut names);
        names
    }

    fn walk_type_names(&self, node: TsNode<'_>, out: &mut Vec<String>) {
        match node.kind() {
            "type_identifier" => {
                let name = self.text(node).to_owned();
                if !is_builtin_java_type(&name) {
                    out.push(name);
                }
            }
            // Skip integral_type (int, long, etc.) and floating_point_type
            "integral_type" | "floating_point_type" | "boolean_type" | "void_type" => {}
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
                "method_invocation" | "object_creation_expression" => {
                    if let Some(callee) = self.callee_name(child) {
                        self.record_call(caller_id.clone(), callee);
                    }
                    // Recurse into arguments
                    if let Some(args) = child.child_by_field_name("arguments") {
                        self.collect_calls(args, caller_id);
                    }
                }
                _ => self.collect_calls(child, caller_id),
            }
        }
    }

    fn callee_name(&self, call_expr: TsNode<'_>) -> Option<String> {
        // method_invocation: object?.method(args) → name field is the method name
        // object_creation_expression: new Type(args) → type field
        match call_expr.kind() {
            "method_invocation" => call_expr
                .child_by_field_name("name")
                .map(|n| self.text(n).to_owned()),
            "object_creation_expression" => {
                call_expr
                    .child_by_field_name("type")
                    .and_then(|t| match t.kind() {
                        "type_identifier" => Some(self.text(t).to_owned()),
                        "generic_type" => t
                            .child_by_field_name("name")
                            .map(|n| self.text(n).to_owned()),
                        _ => None,
                    })
            }
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

/// Returns true for Java primitive types and common standard library types.
fn is_builtin_java_type(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Object"
            | "Integer"
            | "Long"
            | "Double"
            | "Float"
            | "Boolean"
            | "Byte"
            | "Short"
            | "Character"
            | "Number"
            | "Math"
            | "System"
            | "StringBuilder"
            | "StringBuffer"
            | "Comparable"
            | "Serializable"
            | "Cloneable"
            | "Iterable"
            | "Iterator"
            | "Collection"
            | "List"
            | "Set"
            | "Map"
            | "Queue"
            | "Deque"
            | "Stack"
            | "ArrayList"
            | "HashMap"
            | "HashSet"
            | "LinkedList"
            | "Optional"
            | "Stream"
            | "Collectors"
            | "Arrays"
            | "Collections"
            | "Enum"
            | "Throwable"
            | "Exception"
            | "RuntimeException"
            | "Error"
            | "Override"
            | "Deprecated"
            | "SuppressWarnings"
            | "FunctionalInterface"
            | "void"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::JavaParser;
    use crate::parser::LanguageParser;
    use gitcortex_core::schema::{EdgeKind, NodeKind};
    use std::path::Path;

    fn parse(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
    ) {
        let r = JavaParser::new()
            .parse(Path::new("Test.java"), src)
            .unwrap();
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
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
        Vec<(gitcortex_core::graph::NodeId, String)>,
    ) {
        let r = JavaParser::new()
            .parse(Path::new("Test.java"), src)
            .unwrap();
        (
            r.nodes,
            r.edges,
            r.deferred_calls,
            r.deferred_uses,
            r.deferred_implements,
            r.deferred_imports,
            r.deferred_inherits,
            r.deferred_throws,
            r.deferred_annotated,
        )
    }

    #[test]
    fn parses_class_and_method() {
        let src = "public class Greeter { public String greet(String name) { return name; } }";
        let (nodes, edges) = parse(src);
        let classes: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(classes.len(), 1, "expected 1 class");
        assert_eq!(classes[0].name, "Greeter");
        assert_eq!(methods.len(), 1, "expected 1 method");
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
        assert!(!contains.is_empty(), "expected Contains edge");
    }

    #[test]
    fn parses_interface() {
        let src = "public interface Greeter { String greet(String name); }";
        let (nodes, _) = parse(src);
        let interfaces: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Interface || n.kind == NodeKind::Trait)
            .collect();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, "Greeter");
    }

    #[test]
    fn parses_enum() {
        let src = "public enum Direction { NORTH, SOUTH, EAST, WEST }";
        let (nodes, _) = parse(src);
        let enums: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Enum).collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "Direction");
    }

    #[test]
    fn detects_extends_and_implements() {
        let src = "interface Base {}\nclass Child extends Base implements Base {}";
        let (_, _, _, _, implements, _, inherits, ..) = parse_full(src);
        let impl_edges: Vec<_> = implements.iter().filter(|(_, n)| n == "Base").collect();
        let inh_edges: Vec<_> = inherits.iter().filter(|(_, n)| n == "Base").collect();
        assert!(
            impl_edges.len() + inh_edges.len() >= 2,
            "expected extends+implements edges to Base, implements={implements:?} inherits={inherits:?}"
        );
    }

    #[test]
    fn detects_type_annotation_uses() {
        let src = "class Service {}\nclass Controller {\n    public Service handle(Service svc) { return svc; }\n}";
        let (_, _, _, uses, ..) = parse_full(src);
        let svc_uses: Vec<_> = uses.iter().filter(|(_, n)| n == "Service").collect();
        assert!(
            svc_uses.len() >= 2,
            "expected Uses edges to Service (param + return), got: {uses:?}"
        );
    }

    #[test]
    fn detects_import_declaration() {
        let src = "import com.example.MyService;\nimport java.util.List;\npublic class App {}";
        let (_, _, _, _, _, imports, ..) = parse_full(src);
        assert!(
            imports.iter().any(|(_, n)| n == "MyService"),
            "expected import 'MyService', got: {imports:?}"
        );
    }

    #[test]
    fn module_node_is_emitted() {
        let src = "public class App {}";
        let (nodes, _) = parse(src);
        let modules: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "Test"); // from "Test.java"
    }
}
