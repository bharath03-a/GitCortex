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

use super::LanguageParser;

// ── Public parser ─────────────────────────────────────────────────────────────

pub struct RustParser {
    language: tree_sitter::Language,
}

impl RustParser {
    pub fn new() -> Self {
        Self {
            language: tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for RustParser {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn parse(&self, path: &Path, source: &str) -> Result<(Vec<Node>, Vec<Edge>)> {
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
        // Pass 1 — pre-allocate NodeIds for all types so impl blocks can
        // reference them even when the struct is declared after the impl.
        visitor.collect_type_ids(tree.root_node(), &[]);
        // Pass 2 — full walk: create nodes, edges, call sites.
        visitor.visit_items(tree.root_node(), &[], None);

        Ok((visitor.nodes, visitor.edges))
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// Struct/enum/trait name → pre-allocated NodeId.
    /// Populated in pass 1 so impl blocks can reference types by name.
    type_index: HashMap<String, NodeId>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            file: file.to_owned(),
            nodes: Vec::new(),
            edges: Vec::new(),
            type_index: HashMap::new(),
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn text<'t>(&self, node: TsNode<'t>) -> &'src str {
        node.utf8_text(self.source).unwrap_or("")
    }

    fn field_text(&self, node: TsNode<'_>, field: &str) -> Option<String> {
        node.child_by_field_name(field)
            .and_then(|n| n.utf8_text(self.source).ok())
            .map(str::to_owned)
    }

    fn span(node: TsNode<'_>) -> Span {
        Span {
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
        }
    }

    fn visibility(&self, node: TsNode<'_>) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let t = self.text(child);
                return if t.contains("crate") {
                    Visibility::PubCrate
                } else {
                    Visibility::Pub
                };
            }
        }
        Visibility::Private
    }

    fn is_async(&self, node: TsNode<'_>) -> bool {
        let mut cursor = node.walk();
        let result = node.children(&mut cursor).any(|c| c.kind() == "async");
        result
    }

    fn is_unsafe(&self, node: TsNode<'_>) -> bool {
        let mut cursor = node.walk();
        let result = node.children(&mut cursor).any(|c| c.kind() == "unsafe");
        result
    }

    fn qualified(scope: &[String], name: &str) -> String {
        if scope.is_empty() {
            format!("crate::{name}")
        } else {
            format!("crate::{}::{name}", scope.join("::"))
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
                visibility: self.visibility(ts_node),
                is_async: self.is_async(ts_node),
                is_unsafe: self.is_unsafe(ts_node),
                ..Default::default()
            },
        }
    }

    /// Extract the simple type name from potentially generic types.
    /// `Foo<T>` → `"Foo"`, `type_identifier` → as-is.
    fn type_name(&self, node: TsNode<'_>) -> Option<String> {
        match node.kind() {
            "type_identifier" => Some(self.text(node).to_owned()),
            "generic_type" => node
                .child_by_field_name("type")
                .map(|n| self.text(n).to_owned()),
            "scoped_type_identifier" => node
                .child_by_field_name("name")
                .map(|n| self.text(n).to_owned()),
            _ => Some(self.text(node).to_owned()),
        }
    }

    // ── Pass 1: pre-allocate NodeIds for named types ──────────────────────────

    fn collect_type_ids(&mut self, node: TsNode<'_>, scope: &[String]) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            match child.kind() {
                "struct_item" | "enum_item" | "trait_item" => {
                    if let Some(name) = self.field_text(child, "name") {
                        self.type_index.entry(name).or_insert_with(NodeId::new);
                    }
                }
                "mod_item" => {
                    if let Some(name) = self.field_text(child, "name") {
                        if let Some(body) = child.child_by_field_name("body") {
                            let mut new_scope = scope.to_vec();
                            new_scope.push(name);
                            self.collect_type_ids(body, &new_scope);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Pass 2: full AST walk ─────────────────────────────────────────────────

    fn visit_items(
        &mut self,
        parent: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let mut cursor = parent.walk();
        let children: Vec<TsNode<'_>> = parent.named_children(&mut cursor).collect();
        for child in children {
            self.visit_item(child, scope, container_id.clone());
        }
    }

    fn visit_item(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        match node.kind() {
            "function_item" => self.visit_function(node, scope, container_id, NodeKind::Function),
            "struct_item" => self.visit_type_item(node, scope, container_id, NodeKind::Struct),
            "enum_item" => self.visit_type_item(node, scope, container_id, NodeKind::Enum),
            "trait_item" => self.visit_trait(node, scope, container_id),
            "impl_item" => self.visit_impl(node, scope),
            "mod_item" => self.visit_mod(node, scope, container_id),
            "const_item" | "static_item" => {
                self.visit_const(node, scope, container_id)
            }
            "type_item" => self.visit_type_alias(node, scope, container_id),
            "macro_definition" => self.visit_macro_def(node, scope, container_id),
            "call_expression" => self.visit_call(node),
            // Recurse into containers not handled above (e.g. block expressions
            // inside function bodies are skipped in v0.1 for performance).
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
        let Some(name) = self.field_text(node, "name") else { return };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), kind, name, scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);
        // v0.1: do not recurse into function bodies (keep pass 1 fast).
        // Call edges within bodies are captured separately by visit_call.
    }

    fn visit_type_item(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
        kind: NodeKind,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        // Use the pre-allocated ID from pass 1 for stable referencing.
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), kind, name, scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);
    }

    fn visit_trait(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Trait, name.clone(), scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);

        // Recurse into trait body to capture default method signatures.
        if let Some(body) = node.child_by_field_name("body") {
            let mut new_scope = scope.to_vec();
            new_scope.push(name);
            self.visit_items(body, &new_scope, Some(id));
        }
    }

    fn visit_impl(&mut self, node: TsNode<'_>, scope: &[String]) {
        // Determine the implementing type name.
        let type_node = node.child_by_field_name("type");
        let type_name = type_node.and_then(|n| self.type_name(n));
        let Some(type_name) = type_name else { return };

        // Resolve the type's NodeId (it must exist in this file or be unknown).
        let type_id = self.type_index.get(&type_name).cloned();

        // If this is a trait impl, create an Implements edge.
        if let Some(trait_node) = node.child_by_field_name("trait") {
            if let Some(trait_name) = self.type_name(trait_node) {
                let trait_id = self.type_index.get(&trait_name).cloned();
                if let (Some(tid), Some(trid)) = (type_id.clone(), trait_id) {
                    self.edges.push(Edge {
                        src: tid,
                        dst: trid,
                        kind: EdgeKind::Implements,
                    });
                }
            }
        }

        // Recurse into impl body — functions here become Methods.
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            let mut impl_scope = scope.to_vec();
            impl_scope.push(type_name);

            for child in children {
                if child.kind() == "function_item" {
                    self.visit_function(child, &impl_scope, type_id.clone(), NodeKind::Method);
                }
            }
        }
    }

    fn visit_mod(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Module, name.clone(), scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            let mut new_scope = scope.to_vec();
            new_scope.push(name);
            self.visit_items(body, &new_scope, Some(id));
        }
    }

    fn visit_const(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Constant, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);
    }

    fn visit_type_alias(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::TypeAlias, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);
    }

    fn visit_macro_def(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else { return };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Macro, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge { src: cid, dst: id.clone(), kind: EdgeKind::Contains });
        }
        self.nodes.push(graph_node);
    }

    /// Best-effort call detection for simple identifier calls: `foo(args)`.
    /// Method calls and fully-qualified paths are not captured in v0.1.
    fn visit_call(&mut self, _node: TsNode<'_>) {
        // v0.1: Call edges require knowing the callee's NodeId, which means
        // resolving the name across the whole project — not possible in a
        // single-file parse pass. Deferred to a future cross-file resolution
        // phase (post store population).
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gitcortex_core::schema::{EdgeKind, NodeKind};

    use super::RustParser;
    use crate::parser::LanguageParser;

    fn parse(src: &str) -> (Vec<gitcortex_core::graph::Node>, Vec<gitcortex_core::graph::Edge>) {
        RustParser::new().parse(Path::new("test.rs"), src).unwrap()
    }

    #[test]
    fn parses_free_function() {
        let (nodes, _) = parse("pub fn greet(name: &str) -> String { name.into() }");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, NodeKind::Function);
        assert_eq!(nodes[0].name, "greet");
    }

    #[test]
    fn parses_struct() {
        let (nodes, _) = parse("pub struct Person { pub name: String }");
        let structs: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Struct).collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Person");
    }

    #[test]
    fn parses_trait_impl_and_method() {
        let src = r#"
pub trait Greet { fn greet(&self) -> String; }
pub struct Person { pub name: String }
impl Greet for Person {
    fn greet(&self) -> String { self.name.clone() }
}
"#;
        let (nodes, edges) = parse(src);

        let traits: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Trait).collect();
        let structs: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Struct).collect();
        let methods: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Method).collect();
        let impl_edges: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Implements).collect();

        assert_eq!(traits.len(), 1, "expected Greet trait");
        assert_eq!(structs.len(), 1, "expected Person struct");
        assert_eq!(methods.len(), 1, "expected greet method");
        assert_eq!(impl_edges.len(), 1, "expected Implements edge");
    }

    #[test]
    fn parses_module_with_items() {
        let src = r#"
pub mod utils {
    pub fn helper() {}
    pub struct Config {}
}
"#;
        let (nodes, edges) = parse(src);

        let mods: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Module).collect();
        let fns: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Function).collect();
        let contains: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Contains).collect();

        assert_eq!(mods.len(), 1, "expected utils module");
        assert_eq!(fns.len(), 1, "expected helper function");
        assert!(!contains.is_empty(), "expected Contains edges");
    }

    #[test]
    fn qualified_name_includes_module_path() {
        let src = r#"
pub mod inner {
    pub fn foo() {}
}
"#;
        let (nodes, _) = parse(src);
        let foo = nodes.iter().find(|n| n.name == "foo").unwrap();
        assert_eq!(foo.qualified_name, "crate::inner::foo");
    }
}
