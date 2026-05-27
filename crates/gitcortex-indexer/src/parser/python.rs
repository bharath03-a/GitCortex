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

use super::{capture_definition, LanguageParser, ParseResult};

pub struct PythonParser {
    language: tree_sitter::Language,
}

impl PythonParser {
    pub fn new() -> Self {
        Self {
            language: tree_sitter_python::LANGUAGE.into(),
        }
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for PythonParser {
    fn extensions(&self) -> &[&str] {
        &["py"]
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
        visitor.visit_module(tree.root_node());
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
            deferred_annotated: visitor.deferred_annotated,
        })
    }
}

// ── Internal visitor ──────────────────────────────────────────────────────────

struct FileVisitor<'src> {
    source: &'src [u8],
    file: PathBuf,
    /// NodeId of the file-level Module node (anchor for Imports edges).
    module_id: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// class name → NodeId (pass 1)
    class_index: HashMap<String, NodeId>,
    /// function/method name → NodeId (pass 1)
    fn_index: HashMap<String, NodeId>,
    deferred_calls: Vec<(NodeId, String)>,
    deferred_uses: Vec<(NodeId, String)>,
    deferred_implements: Vec<(NodeId, String)>,
    deferred_imports: Vec<(NodeId, String)>,
    deferred_annotated: Vec<(NodeId, String)>,
}

impl<'src> FileVisitor<'src> {
    fn new(file: &Path, source: &'src str) -> Self {
        let module_id = NodeId::new();
        // Derive module name from the file stem (e.g. "auth" from "auth.py").
        let module_name = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("__init__")
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
            class_index: HashMap::new(),
            fn_index: HashMap::new(),
            deferred_calls: Vec::new(),
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_imports: Vec::new(),
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

    /// In Python, public = not starting with `_`.
    fn visibility(name: &str) -> Visibility {
        if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Pub
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
                definition: capture_definition(self.source, ts_node),
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
                "class_definition" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.class_index.entry(name).or_default();
                    }
                    // Recurse into class body to index nested classes and methods.
                    if let Some(body) = child.child_by_field_name("body") {
                        self.collect_names(body);
                    }
                }
                "function_definition" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.text(name_node).to_owned();
                        self.fn_index.entry(name).or_default();
                    }
                }
                "decorated_definition" => {
                    let def = child.child_by_field_name("definition");
                    if let Some(def) = def {
                        match def.kind() {
                            "function_definition" => {
                                if let Some(name_node) = def.child_by_field_name("name") {
                                    let name = self.text(name_node).to_owned();
                                    self.fn_index.entry(name).or_default();
                                }
                            }
                            "class_definition" => {
                                if let Some(name_node) = def.child_by_field_name("name") {
                                    let name = self.text(name_node).to_owned();
                                    self.class_index.entry(name).or_default();
                                }
                                // Recurse into decorated nested class body.
                                if let Some(body) = def.child_by_field_name("body") {
                                    self.collect_names(body);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Pass 2: emit nodes + edges ────────────────────────────────────────────

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
                let is_async = Self::fn_is_async(node);
                self.visit_function(node, scope, None, is_async, &[]);
            }
            "decorated_definition" => {
                let decorators = self.collect_decorators(node);
                let is_async = node
                    .child_by_field_name("definition")
                    .map(Self::fn_is_async)
                    .unwrap_or(false);
                if let Some(def) = node.child_by_field_name("definition") {
                    match def.kind() {
                        "function_definition" => {
                            self.visit_function(def, scope, None, is_async, &decorators)
                        }
                        "class_definition" => self.visit_class(def, scope, &decorators),
                        _ => {}
                    }
                }
            }
            "class_definition" => self.visit_class(node, scope, &[]),
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
        decorators: &[String],
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

        // Determine kind and metadata flags from decorators.
        let has_property = decorators.iter().any(|d| d == "property");
        let has_staticmethod = decorators.iter().any(|d| d == "staticmethod");
        let has_classmethod = decorators.iter().any(|d| d == "classmethod");

        let kind = if has_property {
            NodeKind::Property
        } else if container_id.is_some() {
            NodeKind::Method
        } else {
            NodeKind::Function
        };

        // Check if the body contains a `yield` or `yield_from` → generator.
        let is_generator = node
            .child_by_field_name("body")
            .map(|body| Self::body_has_yield(body))
            .unwrap_or(false);

        let mut graph_node = self.make_node(id.clone(), kind, name, scope, node, is_async);
        if has_property {
            graph_node.metadata.is_property = true;
        }
        if has_staticmethod || has_classmethod {
            graph_node.metadata.is_static = true;
        }
        if is_generator {
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

        // Type annotations → Uses edges
        self.extract_param_types(node, &id);
        self.extract_return_type(node, &id);

        // Decorator names → Uses edges (e.g. @property, @staticmethod, @dataclass)
        // and → deferred_annotated edges.
        for dec in decorators {
            self.deferred_uses.push((id.clone(), dec.clone()));
            self.deferred_annotated.push((id.clone(), dec.clone()));
        }

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(body, &id);
        }
    }

    fn visit_class(&mut self, node: TsNode<'_>, scope: &[String], decorators: &[String]) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_owned();
        let id = self
            .class_index
            .get(&name)
            .cloned()
            .unwrap_or_else(NodeId::new);

        // Determine whether this class inherits from Protocol → Interface.
        let mut is_protocol = false;
        if let Some(bases) = node.child_by_field_name("superclasses") {
            let mut c = bases.walk();
            for base in bases.named_children(&mut c) {
                let base_name = match base.kind() {
                    "identifier" => Some(self.text(base).to_owned()),
                    "attribute" => base
                        .child_by_field_name("attribute")
                        .map(|n| self.text(n).to_owned()),
                    _ => None,
                };
                if base_name.as_deref() == Some("Protocol") {
                    is_protocol = true;
                }
            }
        }

        let class_kind = if is_protocol {
            NodeKind::Interface
        } else {
            NodeKind::Struct
        };

        let mut graph_node =
            self.make_node(id.clone(), class_kind, name.clone(), scope, node, false);
        if is_protocol {
            graph_node.metadata.is_abstract = true;
        }
        self.nodes.push(graph_node);

        // Base classes → Implements edges
        if let Some(bases) = node.child_by_field_name("superclasses") {
            let mut c = bases.walk();
            for base in bases.named_children(&mut c) {
                let base_name = match base.kind() {
                    "identifier" => Some(self.text(base).to_owned()),
                    "attribute" => base
                        .child_by_field_name("attribute")
                        .map(|n| self.text(n).to_owned()),
                    _ => None,
                };
                if let Some(b) = base_name {
                    self.deferred_implements.push((id.clone(), b));
                }
            }
        }

        // Decorator names → Uses edges (e.g. @dataclass) and → deferred_annotated.
        for dec in decorators {
            self.deferred_uses.push((id.clone(), dec.clone()));
            self.deferred_annotated.push((id.clone(), dec.clone()));
        }

        let mut class_scope = scope.to_vec();
        class_scope.push(name.clone());

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let children: Vec<TsNode<'_>> = body.named_children(&mut cursor).collect();
            for child in children {
                match child.kind() {
                    "function_definition" => {
                        let is_async = Self::fn_is_async(child);
                        self.visit_function(child, &class_scope, Some(id.clone()), is_async, &[]);
                    }
                    "decorated_definition" => {
                        let method_decorators = self.collect_decorators(child);
                        let is_async = child
                            .child_by_field_name("definition")
                            .map(Self::fn_is_async)
                            .unwrap_or(false);
                        if let Some(def) = child.child_by_field_name("definition") {
                            match def.kind() {
                                "function_definition" => {
                                    self.visit_function(
                                        def,
                                        &class_scope,
                                        Some(id.clone()),
                                        is_async,
                                        &method_decorators,
                                    );
                                }
                                "class_definition" => {
                                    self.visit_class(def, &class_scope, &method_decorators);
                                    // Add Contains edge from parent class to nested class.
                                    if let Some(nested_name_node) = def.child_by_field_name("name")
                                    {
                                        let nested_name = self.text(nested_name_node).to_owned();
                                        if let Some(nested_id) =
                                            self.class_index.get(&nested_name).cloned()
                                        {
                                            self.edges.push(Edge {
                                                src: id.clone(),
                                                dst: nested_id,
                                                kind: EdgeKind::Contains,
                                            });
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "class_definition" => {
                        self.visit_class(child, &class_scope, &[]);
                        // Add Contains edge from parent class to nested class.
                        if let Some(nested_name_node) = child.child_by_field_name("name") {
                            let nested_name = self.text(nested_name_node).to_owned();
                            if let Some(nested_id) = self.class_index.get(&nested_name).cloned() {
                                self.edges.push(Edge {
                                    src: id.clone(),
                                    dst: nested_id,
                                    kind: EdgeKind::Contains,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn maybe_visit_constant(&mut self, node: TsNode<'_>, scope: &[String]) {
        // Capture module-level bindings as `Constant` nodes. Any
        // simple-identifier assignment at module scope is an importable symbol
        // (`from pkg import default_config`, `__version__`, `T = TypeVar(...)`),
        // not just SCREAMING_SNAKE_CASE — restricting to all-caps dropped the
        // bulk of a module's public surface. Visibility follows Python's
        // leading-underscore convention via `make_node`.
        //
        // Only plain `identifier = …` / `identifier: T = …` targets are taken;
        // tuple/attribute/subscript targets (`a, b = …`, `self.x = …`) are
        // skipped — they aren't module-level named symbols.
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "assignment" {
                if let Some(left) = child.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let name = self.text(left).to_owned();
                        if name.is_empty() {
                            continue;
                        }
                        let id = NodeId::new();
                        let graph_node =
                            self.make_node(id, NodeKind::Constant, name, scope, node, false);
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
        for child in children {
            match child.kind() {
                "import_statement" => {
                    // `import foo`, `import foo.bar`, `import foo as f`
                    let mut c = child.walk();
                    for name_node in child.named_children(&mut c) {
                        let leaf = match name_node.kind() {
                            "dotted_name" => {
                                let text = self.text(name_node);
                                text.split('.').next_back().map(|s| s.to_owned())
                            }
                            "aliased_import" => name_node
                                .child_by_field_name("name")
                                .map(|n| self.text(n))
                                .map(|t| t.split('.').next_back().unwrap_or(t).to_owned()),
                            _ => None,
                        };
                        if let Some(name) = leaf {
                            self.deferred_imports.push((self.module_id.clone(), name));
                        }
                    }
                }
                "import_from_statement" => {
                    // `from foo import bar, baz`
                    // Named children: first is the source module (dotted_name or
                    // relative_import), the rest are the imported names.
                    let mut c = child.walk();
                    let all_children: Vec<TsNode<'_>> = child.named_children(&mut c).collect();
                    for name_node in all_children.iter().skip(1) {
                        let leaf = match name_node.kind() {
                            "dotted_name" => {
                                let text = self.text(*name_node);
                                Some(text.split('.').next_back().unwrap_or(text).to_owned())
                            }
                            "aliased_import" => name_node
                                .child_by_field_name("name")
                                .map(|n| self.text(n))
                                .map(|t| t.split('.').next_back().unwrap_or(t).to_owned()),
                            "wildcard_import" => None,
                            _ => None,
                        };
                        if let Some(name) = leaf {
                            self.deferred_imports.push((self.module_id.clone(), name));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Call collection ───────────────────────────────────────────────────────

    fn collect_calls(&mut self, node: TsNode<'_>, caller_id: &NodeId) {
        let mut cursor = node.walk();
        let children: Vec<TsNode<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            if child.kind() == "call" {
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

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Returns true if this `function_definition` node is `async def`.
    fn fn_is_async(node: TsNode<'_>) -> bool {
        let mut c = node.walk();
        // `async` appears as an anonymous child (keyword) before `def`
        let result = node.children(&mut c).any(|n| n.kind() == "async");
        result
    }

    /// Returns true if the body subtree contains a `yield` or `yield_from` expression.
    fn body_has_yield(node: TsNode<'_>) -> bool {
        if node.kind() == "yield" || node.kind() == "yield_from" {
            return true;
        }
        // Don't descend into nested function definitions — their yields are not
        // generators of the outer function.
        if node.kind() == "function_definition" {
            return false;
        }
        let mut c = node.walk();
        let found = node.named_children(&mut c).any(Self::body_has_yield);
        found
    }

    /// Extract decorator names from a `decorated_definition` node.
    fn collect_decorators(&self, node: TsNode<'_>) -> Vec<String> {
        let mut c = node.walk();
        node.named_children(&mut c)
            .filter(|n| n.kind() == "decorator")
            .filter_map(|d| self.decorator_name(d))
            .collect()
    }

    /// Get the callable name from a `decorator` node.
    fn decorator_name(&self, decorator: TsNode<'_>) -> Option<String> {
        let mut c = decorator.walk();
        let child = decorator.named_children(&mut c).next()?;
        match child.kind() {
            "identifier" => Some(self.text(child).to_owned()),
            "attribute" => child
                .child_by_field_name("attribute")
                .map(|n| self.text(n).to_owned()),
            "call" => child
                .child_by_field_name("function")
                .and_then(|f| match f.kind() {
                    "identifier" => Some(self.text(f).to_owned()),
                    "attribute" => f
                        .child_by_field_name("attribute")
                        .map(|n| self.text(n).to_owned()),
                    _ => None,
                }),
            _ => None,
        }
    }

    /// Extract type identifiers from a type-annotation node.
    /// Records all identifiers found (e.g. `List[MyType]` → ["List", "MyType"]).
    fn extract_param_types(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        let Some(params) = fn_node.child_by_field_name("parameters") else {
            return;
        };
        let mut c = params.walk();
        for param in params.named_children(&mut c) {
            let type_node = match param.kind() {
                "typed_parameter" | "typed_default_parameter" => param.child_by_field_name("type"),
                _ => None,
            };
            if let Some(t) = type_node {
                for name in self.collect_type_names(t) {
                    self.deferred_uses.push((fn_id.clone(), name));
                }
            }
        }
    }

    fn extract_return_type(&mut self, fn_node: TsNode<'_>, fn_id: &NodeId) {
        if let Some(ret) = fn_node.child_by_field_name("return_type") {
            for name in self.collect_type_names(ret) {
                self.deferred_uses.push((fn_id.clone(), name));
            }
        }
    }

    /// Walk a type-annotation subtree and collect all identifier names.
    fn collect_type_names(&self, node: TsNode<'_>) -> Vec<String> {
        let mut names = Vec::new();
        self.walk_type_names(node, &mut names);
        names
    }

    fn walk_type_names(&self, node: TsNode<'_>, out: &mut Vec<String>) {
        match node.kind() {
            "identifier" => {
                let name = self.text(node).to_owned();
                if !is_builtin_type(&name) {
                    out.push(name);
                }
            }
            _ => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    self.walk_type_names(child, out);
                }
            }
        }
    }
}

/// Returns true for built-in Python types and typing-module generics that do
/// not correspond to user-defined symbols.
fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "str"
            | "bool"
            | "float"
            | "complex"
            | "bytes"
            | "bytearray"
            | "None"
            | "list"
            | "dict"
            | "set"
            | "frozenset"
            | "tuple"
            | "type"
            | "object"
            | "Any"
            | "Optional"
            | "Union"
            | "List"
            | "Dict"
            | "Set"
            | "FrozenSet"
            | "Tuple"
            | "Callable"
            | "Type"
            | "ClassVar"
            | "Final"
            | "Literal"
            | "TypeVar"
            | "Generic"
            | "Protocol"
            | "Sequence"
            | "Iterable"
            | "Iterator"
            | "Generator"
            | "Coroutine"
            | "Awaitable"
            | "AsyncIterator"
            | "AsyncGenerator"
            | "NoReturn"
            | "Never"
            | "Self"
            | "Annotated"
            | "TypeAlias"
            | "ParamSpec"
            | "TypeVarTuple"
            | "overload"
            | "abstractmethod"
            | "staticmethod"
            | "classmethod"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::PythonParser;
    use crate::parser::LanguageParser;
    use gitcortex_core::schema::{EdgeKind, NodeKind};
    use std::path::Path;

    fn parse(
        src: &str,
    ) -> (
        Vec<gitcortex_core::graph::Node>,
        Vec<gitcortex_core::graph::Edge>,
    ) {
        let r = PythonParser::new()
            .parse(Path::new("test.py"), src)
            .unwrap();
        (r.nodes, r.edges)
    }

    #[allow(clippy::type_complexity)]
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
        let r = PythonParser::new()
            .parse(Path::new("test.py"), src)
            .unwrap();
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
    fn parses_free_function() {
        let (nodes, _) = parse("def greet(name):\n    return name\n");
        // Module node + Function node
        assert_eq!(nodes.len(), 2);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "greet");
    }

    #[test]
    fn parses_class_and_method() {
        let src = "class Person:\n    def greet(self):\n        pass\n";
        let (nodes, edges) = parse(src);
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
    fn detects_call_edges() {
        let src = "def caller():\n    callee()\ndef callee():\n    pass\n";
        let (_, edges) = parse(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn detects_base_class_implements() {
        let src = "class Base:\n    pass\nclass Child(Base):\n    pass\n";
        let (_, _, _, _, implements, _) = parse_full(src);
        assert!(
            implements.iter().any(|(_, name)| name == "Base"),
            "expected Implements edge to Base, got: {implements:?}"
        );
    }

    #[test]
    fn detects_type_annotation_uses() {
        let src = "class Foo:\n    pass\ndef bar(x: Foo) -> Foo:\n    pass\n";
        let (_, _, _, uses, _, _) = parse_full(src);
        let uses_foo: Vec<_> = uses.iter().filter(|(_, n)| n == "Foo").collect();
        assert!(
            uses_foo.len() >= 2,
            "expected at least 2 Uses edges to Foo, got: {uses_foo:?}"
        );
    }

    #[test]
    fn detects_decorator_uses() {
        let src = "class Foo:\n    @property\n    def name(self):\n        return self._name\n";
        let (_, _, _, uses, _, _) = parse_full(src);
        assert!(
            uses.iter().any(|(_, n)| n == "property"),
            "expected Uses edge to 'property' decorator, got: {uses:?}"
        );
    }

    #[test]
    fn detects_import_statement() {
        let src = "import os\nimport sys\n\ndef main():\n    pass\n";
        let (_, _, _, _, _, imports) = parse_full(src);
        assert!(
            imports.iter().any(|(_, n)| n == "os"),
            "expected import 'os', got: {imports:?}"
        );
        assert!(
            imports.iter().any(|(_, n)| n == "sys"),
            "expected import 'sys', got: {imports:?}"
        );
    }

    #[test]
    fn detects_from_import_statement() {
        let src = "from os.path import join, exists\n\ndef main():\n    pass\n";
        let (_, _, _, _, _, imports) = parse_full(src);
        assert!(
            imports.iter().any(|(_, n)| n == "join"),
            "expected import 'join', got: {imports:?}"
        );
        assert!(
            imports.iter().any(|(_, n)| n == "exists"),
            "expected import 'exists', got: {imports:?}"
        );
    }

    #[test]
    fn module_node_is_emitted() {
        let (nodes, _) = parse("x = 1\n");
        let modules: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "test");
    }

    #[test]
    fn async_function_flagged() {
        let src = "async def fetch():\n    pass\n";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert!(fns[0].metadata.is_async);
    }

    // ── Regression: Protocol / Interface ─────────────────────────────────────

    #[test]
    fn protocol_class_becomes_interface() {
        let src = "from typing import Protocol\n\nclass MyProto(Protocol):\n    def do_it(self) -> None:\n        ...\n";
        let (nodes, _) = parse(src);
        let ifaces: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Interface)
            .collect();
        assert_eq!(ifaces.len(), 1, "expected 1 Interface, got: {ifaces:?}");
        assert_eq!(ifaces[0].name, "MyProto");
        assert!(
            ifaces[0].metadata.is_abstract,
            "Protocol should be is_abstract"
        );
    }

    #[test]
    fn non_protocol_class_is_struct() {
        let src = "class Plain:\n    def work(self):\n        pass\n";
        let (nodes, _) = parse(src);
        let structs: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Plain");
        assert!(!structs[0].metadata.is_abstract);
    }

    // ── Regression: decorators ────────────────────────────────────────────────

    #[test]
    fn property_decorator_yields_property_kind() {
        let src =
            "class Foo:\n    @property\n    def bar(self) -> str:\n        return self._bar\n";
        let (nodes, _) = parse(src);
        let props: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Property)
            .collect();
        assert_eq!(props.len(), 1, "expected 1 Property node, got: {props:?}");
        assert_eq!(props[0].name, "bar");
        assert!(props[0].metadata.is_property);
    }

    #[test]
    fn staticmethod_decorator_sets_is_static() {
        let src = "class Foo:\n    @staticmethod\n    def create(x: int) -> 'Foo':\n        return Foo()\n";
        let (nodes, _) = parse(src);
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "create");
        assert!(
            methods[0].metadata.is_static,
            "staticmethod should set is_static"
        );
    }

    #[test]
    fn classmethod_decorator_sets_is_static() {
        let src = "class Foo:\n    @classmethod\n    def from_str(cls, s: str) -> 'Foo':\n        return cls()\n";
        let (nodes, _) = parse(src);
        let methods: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "from_str");
        assert!(
            methods[0].metadata.is_static,
            "classmethod should set is_static"
        );
    }

    #[test]
    fn dataclass_decorator_class_is_struct() {
        let src = "from dataclasses import dataclass\n\n@dataclass\nclass Point:\n    x: float\n    y: float\n";
        let (nodes, _) = parse(src);
        let structs: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1, "expected Struct for @dataclass Point");
        assert_eq!(structs[0].name, "Point");
    }

    // ── Regression: generator / async-generator ──────────────────────────────

    #[test]
    fn generator_function_sets_is_generator() {
        let src = "def numbers():\n    yield 1\n    yield 2\n";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert!(
            fns[0].metadata.is_generator,
            "yield fn should be is_generator"
        );
        assert!(!fns[0].metadata.is_async);
    }

    #[test]
    fn async_generator_is_both_async_and_generator() {
        let src = "async def stream():\n    yield 1\n    yield 2\n";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert!(
            fns[0].metadata.is_async,
            "async generator should be is_async"
        );
        assert!(
            fns[0].metadata.is_generator,
            "async generator should be is_generator"
        );
    }

    #[test]
    fn nested_yield_does_not_pollute_outer_function() {
        // The outer function is NOT a generator; only the inner lambda/nested fn yields.
        let src = "def outer():\n    def inner():\n        yield 1\n    return inner()\n";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        let outer = fns
            .iter()
            .find(|n| n.name == "outer")
            .expect("outer not found");
        assert!(
            !outer.metadata.is_generator,
            "outer should NOT be generator — yield is in nested fn"
        );
    }

    // ── Regression: constants ─────────────────────────────────────────────────

    #[test]
    fn module_level_bindings_detected() {
        // Module-level assignments are importable symbols regardless of case:
        // ALL_CAPS constants, lowercase config objects, dunders, and TypeVars
        // all count. Locals inside functions must NOT be captured here.
        let src = "MAX_SIZE = 100\nDEFAULT_NAME = 'anon'\ndefault_config = {}\n__version__ = '1.0'\nT = 1\n\ndef f():\n    local = 1\n    return local\n";
        let (nodes, _) = parse(src);
        let names: Vec<&str> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Constant)
            .map(|n| n.name.as_str())
            .collect();
        for expected in [
            "MAX_SIZE",
            "DEFAULT_NAME",
            "default_config",
            "__version__",
            "T",
        ] {
            assert!(
                names.contains(&expected),
                "expected module-level binding `{expected}` as Constant; got {names:?}"
            );
        }
        assert!(
            !names.contains(&"local"),
            "function-local assignment must not be captured as a module Constant"
        );
    }

    // ── Regression: nested classes ────────────────────────────────────────────

    #[test]
    fn nested_class_emits_contains_edge_from_parent() {
        let src = "class Outer:\n    class Inner:\n        pass\n";
        let (nodes, edges) = parse(src);
        let outer = nodes
            .iter()
            .find(|n| n.name == "Outer")
            .expect("Outer not found");
        let inner = nodes
            .iter()
            .find(|n| n.name == "Inner")
            .expect("Inner not found");
        let contains: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
        assert!(
            contains
                .iter()
                .any(|e| e.src == outer.id && e.dst == inner.id),
            "expected Contains Outer → Inner"
        );
    }

    // ── Regression: type annotation Uses deferred entries ────────────────────

    #[test]
    fn multiple_type_annotations_produce_uses_entries() {
        let src =
            "class Req:\n    pass\nclass Resp:\n    pass\ndef handler(r: Req) -> Resp:\n    pass\n";
        let (_, _, _, uses, _, _) = parse_full(src);
        let uses_req: Vec<_> = uses.iter().filter(|(_, n)| n == "Req").collect();
        let uses_resp: Vec<_> = uses.iter().filter(|(_, n)| n == "Resp").collect();
        assert!(!uses_req.is_empty(), "expected Uses edge to Req");
        assert!(!uses_resp.is_empty(), "expected Uses edge to Resp");
    }

    // ── Regression: visibility ────────────────────────────────────────────────

    #[test]
    fn private_method_has_private_visibility() {
        use gitcortex_core::schema::Visibility;
        let src = "class Foo:\n    def _internal(self):\n        pass\n    def public(self):\n        pass\n";
        let (nodes, _) = parse(src);
        let internal = nodes
            .iter()
            .find(|n| n.name == "_internal")
            .expect("_internal not found");
        let public = nodes
            .iter()
            .find(|n| n.name == "public")
            .expect("public not found");
        assert_eq!(internal.metadata.visibility, Visibility::Private);
        assert_eq!(public.metadata.visibility, Visibility::Pub);
    }

    // ── Regression: call detection ────────────────────────────────────────────

    #[test]
    fn calls_edge_between_two_functions() {
        let src = "def helper():\n    pass\n\ndef main():\n    helper()\n";
        let (_, edges) = parse(src);
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        assert_eq!(calls.len(), 1, "expected exactly 1 Calls edge");
    }

    #[test]
    fn method_call_via_self_creates_calls_edge() {
        let src = "class Svc:\n    def run(self):\n        self.process()\n    def process(self):\n        pass\n";
        let (nodes, edges) = parse(src);
        let run = nodes
            .iter()
            .find(|n| n.name == "run")
            .expect("run not found");
        let process = nodes
            .iter()
            .find(|n| n.name == "process")
            .expect("process not found");
        let calls: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();
        // self.process() resolves immediately because "process" is pre-indexed in pass 1
        assert!(
            calls.iter().any(|e| e.src == run.id && e.dst == process.id),
            "expected Calls edge run → process, got: {calls:?}"
        );
    }

    // ── Regression: import edge collection ───────────────────────────────────

    #[test]
    fn aliased_import_uses_alias_name() {
        let src = "import numpy as np\nimport pandas as pd\n";
        let (_, _, _, _, _, imports) = parse_full(src);
        // For aliased imports, the leaf of the original module name is recorded
        let names: Vec<&str> = imports.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            names.contains(&"numpy"),
            "expected import 'numpy', got: {names:?}"
        );
        assert!(
            names.contains(&"pandas"),
            "expected import 'pandas', got: {names:?}"
        );
    }

    #[test]
    fn dotted_import_records_leaf_module() {
        let src = "import os.path\n";
        let (_, _, _, _, _, imports) = parse_full(src);
        let names: Vec<&str> = imports.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            names.contains(&"path"),
            "expected leaf 'path' from 'import os.path', got: {names:?}"
        );
    }
}
