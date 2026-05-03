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
        // Pass 1 — pre-allocate NodeIds for all named items so forward
        // references (impl blocks, call sites) can resolve correctly.
        visitor.collect_names(tree.root_node());
        // Pass 2 — full walk: create nodes and edges.
        visitor.visit_items(tree.root_node(), &[], None);
        // Pass 3 — collect use declarations for Imports edges.
        visitor.collect_imports(tree.root_node());

        Ok(ParseResult {
            nodes: visitor.nodes,
            edges: visitor.edges,
            deferred_calls: visitor.deferred_calls,
            deferred_uses: visitor.deferred_uses,
            deferred_implements: visitor.deferred_implements,
            deferred_imports: visitor.deferred_imports,
            deferred_inherits: Vec::new(),
            deferred_throws: Vec::new(),
            deferred_annotated: Vec::new(),
        })
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    type_index: HashMap<String, NodeId>,
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
    deferred_uses: Vec<(NodeId, String)>,
    deferred_implements: Vec<(NodeId, String)>,
    deferred_imports: Vec<(NodeId, String)>,
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
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_imports: Vec::new(),
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

    fn type_name(&self, node: TsNode<'_>) -> Option<String> {
        match node.kind() {
            "type_identifier" => Some(self.text(node).to_owned()),
            "generic_type" => node
                .child_by_field_name("type")
                .map(|n| self.text(n).to_owned()),
            "scoped_type_identifier" => node
                .child_by_field_name("name")
                .map(|n| self.text(n).to_owned()),
            "reference_type" => node
                .child_by_field_name("type")
                .and_then(|n| self.type_name(n)),
            "mutable_specifier" => None,
            _ => Some(self.text(node).to_owned()),
        }
    }

    // ── Pass 1: pre-allocate NodeIds for all named items ─────────────────────

    fn collect_names(&mut self, node: TsNode<'_>) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            match child.kind() {
                "struct_item" | "enum_item" | "trait_item" => {
                    if let Some(name) = self.field_text(child, "name") {
                        self.type_index.entry(name).or_default();
                    }
                }
                "function_item" => {
                    if let Some(name) = self.field_text(child, "name") {
                        self.fn_index.entry(name).or_default();
                    }
                }
                "impl_item" => {
                    // Methods are not pre-allocated — they can share names across
                    // multiple impl blocks (e.g. fmt in Display and Debug).
                    // Methods are never targets of bare call_expression resolution.
                }
                "mod_item" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        self.collect_names(body);
                    }
                }
                _ => {}
            }
        }
    }

    // ── Pass 2: full AST walk ─────────────────────────────────────────────────

    fn visit_items(&mut self, parent: TsNode<'_>, scope: &[String], container_id: Option<NodeId>) {
        let mut cursor = parent.walk();
        let children: Vec<TsNode<'_>> = parent.named_children(&mut cursor).collect();
        for child in children {
            self.visit_item(child, scope, container_id.clone());
        }
    }

    fn visit_item(&mut self, node: TsNode<'_>, scope: &[String], container_id: Option<NodeId>) {
        match node.kind() {
            "function_item" => self.visit_function(node, scope, container_id, NodeKind::Function),
            "struct_item" => self.visit_type_item(node, scope, container_id, NodeKind::Struct),
            "enum_item" => self.visit_type_item(node, scope, container_id, NodeKind::Enum),
            "trait_item" => self.visit_trait(node, scope, container_id),
            "impl_item" => self.visit_impl(node, scope),
            "mod_item" => self.visit_mod(node, scope, container_id),
            "const_item" | "static_item" => self.visit_const(node, scope, container_id),
            "type_item" => self.visit_type_alias(node, scope, container_id),
            "macro_definition" => self.visit_macro_def(node, scope, container_id),
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
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        // Methods always get a fresh ID — same method name can appear in multiple
        // impl blocks (e.g. `fmt` in Display and Debug) and bare-name call resolution
        // doesn't apply to methods. Free functions use the fn_index for deferred calls.
        let id = if kind == NodeKind::Method {
            NodeId::new()
        } else {
            self.fn_index
                .get(&name)
                .cloned()
                .unwrap_or_else(NodeId::new)
        };
        let graph_node = self.make_node(id.clone(), kind, name, scope, node);

        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }

        // Uses edges: parameter types and return type referencing same-file types.
        self.collect_uses_edges(node, &id);

        self.nodes.push(graph_node);

        // Walk the function body for call sites.
        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    /// Create `Uses` edges for each parameter/return type. Intra-file types
    /// resolve immediately; cross-file types go into `deferred_uses`.
    fn collect_uses_edges(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        let mut type_names: Vec<String> = Vec::new();

        if let Some(params) = fn_node.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            for param in params.named_children(&mut cursor) {
                if param.kind() == "parameter" {
                    if let Some(type_node) = param.child_by_field_name("type") {
                        if let Some(tname) = self.type_name(type_node) {
                            type_names.push(tname);
                        }
                    }
                }
            }
        }

        if let Some(ret_type) = fn_node.child_by_field_name("return_type") {
            if let Some(tname) = self.type_name(ret_type) {
                type_names.push(tname);
            }
        }

        for tname in type_names {
            if let Some(tid) = self.type_index.get(&tname).cloned() {
                self.edges.push(Edge {
                    src: fn_id.clone(),
                    dst: tid,
                    kind: EdgeKind::Uses,
                });
            } else if !tname.is_empty()
                && !is_primitive(&tname)
                && !self
                    .deferred_uses
                    .iter()
                    .any(|(id, n)| id == fn_id && n == &tname)
            {
                self.deferred_uses.push((fn_id.clone(), tname));
            }
        }
    }

    /// Recursively walk an expression node collecting call sites.
    ///
    /// Called both on container nodes (blocks, statements) and directly on
    /// call/method-call nodes when recursing into chained receivers. We match
    /// on `node.kind()` first so the node itself is never silently skipped.
    fn collect_calls(&mut self, node: TsNode<'_>, caller_id: &NodeId) {
        match node.kind() {
            "call_expression" => {
                if let Some(callee) = self.callee_name(node) {
                    self.record_call(caller_id.clone(), callee);
                }
                if let Some(args) = node.child_by_field_name("arguments") {
                    self.collect_calls(args, caller_id);
                }
                // For chained calls like `store.apply_diff(...).context("x")`,
                // tree-sitter represents .context as a call_expression whose
                // `function` is a field_expression whose `value` is the inner
                // call_expression for .apply_diff. Recurse into that value.
                if let Some(func) = node.child_by_field_name("function") {
                    if let Some(value) = func.child_by_field_name("value") {
                        self.collect_calls(value, caller_id);
                    }
                }
            }
            "method_call_expression" => {
                // tree-sitter-rust uses call_expression for dot-call syntax,
                // but method_call_expression may appear for other constructs.
                if let Some(name_node) = node.child_by_field_name("name") {
                    let method = self.text(name_node).to_owned();
                    self.record_call(caller_id.clone(), method);
                }
                if let Some(args) = node.child_by_field_name("arguments") {
                    self.collect_calls(args, caller_id);
                }
                if let Some(recv) = node.child_by_field_name("receiver") {
                    self.collect_calls(recv, caller_id);
                }
            }
            _ => {
                let mut cursor = node.walk();
                let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
                for child in children {
                    self.collect_calls(child, caller_id);
                }
            }
        }
    }

    /// Extract the simple callee name from a `call_expression` function field.
    fn callee_name(&self, call_expr: TsNode<'_>) -> Option<String> {
        let func = call_expr.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => Some(self.text(func).to_owned()),
            "scoped_identifier" => func
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(self.source).ok())
                .map(str::to_owned),
            "field_expression" => func
                .child_by_field_name("field")
                .and_then(|n| n.utf8_text(self.source).ok())
                .map(str::to_owned),
            _ => None,
        }
    }

    /// Resolve a call: create an intra-file edge or push to deferred list.
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

    fn visit_type_item(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
        kind: NodeKind,
    ) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = self
            .type_index
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
    }

    fn visit_trait(&mut self, node: TsNode<'_>, scope: &[String], container_id: Option<NodeId>) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = self
            .type_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);
        let graph_node = self.make_node(id.clone(), NodeKind::Trait, name.clone(), scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            let mut new_scope = scope.to_vec();
            new_scope.push(name);
            self.visit_items(body, &new_scope, Some(id));
        }
    }

    fn visit_impl(&mut self, node: TsNode<'_>, scope: &[String]) {
        let type_node = node.child_by_field_name("type");
        let type_name = type_node.and_then(|n| self.type_name(n));
        let Some(type_name) = type_name else { return };
        let type_id = self.type_index.get(&type_name).cloned();

        if let Some(trait_node) = node.child_by_field_name("trait") {
            if let Some(trait_name) = self.type_name(trait_node) {
                let trait_id = self.type_index.get(&trait_name).cloned();
                match (type_id.clone(), trait_id) {
                    (Some(tid), Some(trid)) => {
                        self.edges.push(Edge {
                            src: tid,
                            dst: trid,
                            kind: EdgeKind::Implements,
                        });
                    }
                    (Some(tid), None)
                        if !is_primitive(&trait_name)
                            && !self
                                .deferred_implements
                                .iter()
                                .any(|(id, n)| id == &tid && n == &trait_name) =>
                    {
                        self.deferred_implements.push((tid, trait_name));
                    }
                    _ => {}
                }
            }
        }

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

    fn visit_mod(&mut self, node: TsNode<'_>, scope: &[String], container_id: Option<NodeId>) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Module, name.clone(), scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);

        if let Some(body) = node.child_by_field_name("body") {
            let mut new_scope = scope.to_vec();
            new_scope.push(name);
            self.visit_items(body, &new_scope, Some(id));
        }
    }

    fn visit_const(&mut self, node: TsNode<'_>, scope: &[String], container_id: Option<NodeId>) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Constant, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);
    }

    fn visit_type_alias(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::TypeAlias, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);
    }

    fn visit_macro_def(
        &mut self,
        node: TsNode<'_>,
        scope: &[String],
        container_id: Option<NodeId>,
    ) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let id = NodeId::new();
        let graph_node = self.make_node(id.clone(), NodeKind::Macro, name, scope, node);
        if let Some(cid) = container_id {
            self.edges.push(Edge {
                src: cid,
                dst: id.clone(),
                kind: EdgeKind::Contains,
            });
        }
        self.nodes.push(graph_node);
    }

    /// Walk `use_declaration` nodes in the AST root and record deferred imports.
    /// We emit one Imports edge per leaf identifier that is not a primitive.
    fn collect_imports(&mut self, root: TsNode<'_>) {
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "use_declaration" {
                // Find the use_as_clause or use_list or scoped_identifier etc.
                if let Some(arg) = child.child_by_field_name("argument") {
                    self.collect_import_leaves(arg);
                }
            } else if child.kind() == "mod_item" {
                if let Some(body) = child.child_by_field_name("body") {
                    self.collect_imports(body);
                }
            }
        }
    }

    fn collect_import_leaves(&mut self, node: TsNode<'_>) {
        match node.kind() {
            "identifier" | "type_identifier" => {
                let name = self.text(node).to_owned();
                if !name.is_empty()
                    && !is_primitive(&name)
                    && name != "self"
                    && name != "super"
                    && name != "crate"
                {
                    // Use file-level placeholder NodeId — resolved to real nodes in indexer.
                    let placeholder = NodeId::new();
                    self.deferred_imports.push((placeholder, name));
                }
            }
            "use_list" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.collect_import_leaves(child);
                }
            }
            "scoped_identifier" | "scoped_use_list" => {
                // Recurse to get the leaf name.
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.collect_import_leaves(child);
                }
            }
            "use_as_clause" => {
                // `use foo as bar` — record bar (the alias).
                if let Some(alias) = node.child_by_field_name("alias") {
                    self.collect_import_leaves(alias);
                }
            }
            _ => {}
        }
    }
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "char"
            | "str"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Rc"
            | "Arc"
            | "Cell"
            | "RefCell"
            | "Cow"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "PathBuf"
            | "Path"
            | "OsString"
            | "OsStr"
            | "Send"
            | "Sync"
            | "Sized"
            | "Clone"
            | "Copy"
            | "Debug"
            | "Display"
            | "Default"
            | "PartialEq"
            | "Eq"
            | "PartialOrd"
            | "Ord"
            | "Hash"
            | "Iterator"
            | "Into"
            | "From"
            | "AsRef"
            | "AsMut"
            | "Deref"
            | "DerefMut"
            | "Error"
            | "Write"
            | "Read"
            | "Seek"
            | "Self"
            | "()"
            | "_"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gitcortex_core::{
        graph::{Edge, Node},
        schema::{EdgeKind, NodeKind},
    };

    use super::RustParser;
    use crate::parser::LanguageParser;

    fn parse(src: &str) -> (Vec<Node>, Vec<Edge>) {
        let r = RustParser::new().parse(Path::new("test.rs"), src).unwrap();
        (r.nodes, r.edges)
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
        let structs: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
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
        let structs: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        let impl_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Implements)
            .collect();

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

        let mods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .collect();
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();

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

    #[test]
    fn detects_intra_file_calls() {
        let src = r#"
pub fn caller() { callee(); }
pub fn callee() {}
"#;
        let (_, edges) = parse(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1, "expected one Calls edge");
    }

    #[test]
    fn detects_uses_edges_for_param_types() {
        let src = r#"
pub struct Config {}
pub fn run(cfg: Config) {}
"#;
        let (_, edges) = parse(src);
        let uses: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Uses).collect();
        assert_eq!(uses.len(), 1, "expected one Uses edge from run to Config");
    }

    #[test]
    fn deferred_calls_capture_unknown_callees() {
        let src = r#"
pub fn caller() { external_fn(); }
"#;
        let result = RustParser::new().parse(Path::new("test.rs"), src).unwrap();
        assert_eq!(result.deferred_calls.len(), 1);
        assert_eq!(result.deferred_calls[0].1, "external_fn");
    }
}
