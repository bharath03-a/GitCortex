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
            deferred_annotated: Vec::new(),
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
        let kind = if container_id.is_some() {
            NodeKind::Method
        } else {
            NodeKind::Function
        };
        let graph_node = self.make_node(id.clone(), kind, name, scope, node, is_async);

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
        for dec in decorators {
            self.deferred_uses.push((id.clone(), dec.clone()));
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
        let graph_node = self.make_node(
            id.clone(),
            NodeKind::Struct,
            name.clone(),
            scope,
            node,
            false,
        );
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

        // Decorator names → Uses edges (e.g. @dataclass)
        for dec in decorators {
            self.deferred_uses.push((id.clone(), dec.clone()));
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
                            if def.kind() == "function_definition" {
                                self.visit_function(
                                    def,
                                    &class_scope,
                                    Some(id.clone()),
                                    is_async,
                                    &method_decorators,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn maybe_visit_constant(&mut self, node: TsNode<'_>, scope: &[String]) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "assignment" {
                if let Some(left) = child.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let name = self.text(left).to_owned();
                        if name
                            .chars()
                            .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
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
                    let all_children: Vec<TsNode<'_>> =
                        child.named_children(&mut c).collect();
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
            "call" => child.child_by_field_name("function").and_then(|f| {
                match f.kind() {
                    "identifier" => Some(self.text(f).to_owned()),
                    "attribute" => f
                        .child_by_field_name("attribute")
                        .map(|n| self.text(n).to_owned()),
                    _ => None,
                }
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
                "typed_parameter" | "typed_default_parameter" => {
                    param.child_by_field_name("type")
                }
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
        let fns: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Function).collect();
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
        let modules: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Module).collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "test");
    }

    #[test]
    fn async_function_flagged() {
        let src = "async def fetch():\n    pass\n";
        let (nodes, _) = parse(src);
        let fns: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Function).collect();
        assert_eq!(fns.len(), 1);
        assert!(fns[0].metadata.is_async);
    }
}
