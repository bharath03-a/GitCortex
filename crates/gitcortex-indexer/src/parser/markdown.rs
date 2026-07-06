//! Structural Markdown parser — headings become `Section` nodes, inline
//! code-spans and link text that look like identifiers become unresolved
//! `References` edges to code symbols. No LLM calls, no HTML rendering: pure
//! CommonMark structural extraction via `pulldown-cmark`'s event stream.
//!
//! Doc→code references are intentionally cross-language (a README can
//! reference a Python function from a Rust repo's Python bindings, etc.) —
//! see `EdgeKind::References` and `resolve_deferred` in `indexer.rs`, which
//! only scopes resolution by language when the source file's extension is
//! recognised. Markdown isn't, so resolution is a no-op pass-through there.

use std::path::Path;

use gitcortex_core::{
    error::Result,
    graph::{Edge, Node, NodeId, NodeMetadata, Span},
    schema::{EdgeKind, NodeKind, Visibility},
};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser as CmarkParser, Tag, TagEnd};

use super::{LanguageParser, ParseResult};

pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for MarkdownParser {
    fn extensions(&self) -> &[&str] {
        &["md", "markdown"]
    }

    fn parse(&self, path: &Path, source: &str) -> Result<ParseResult> {
        let line_index = LineIndex::new(source);

        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut deferred_doc_refs: Vec<(NodeId, String)> = Vec::new();

        // Stack of (heading_level, section_node_id) — open ancestors, deepest last.
        let mut stack: Vec<(u8, NodeId)> = Vec::new();
        // The innermost section currently in scope, used as the source of any
        // doc-ref found in its body. `None` before the first heading — refs
        // found there are dropped (see module doc / known limitation).
        let mut current_section: Option<NodeId> = None;

        let mut in_heading = false;
        let mut heading_text = String::new();
        let mut heading_level: u8 = 1;
        let mut heading_start_line: u32 = 1;

        let mut in_link = false;
        let mut link_text = String::new();

        let cmark = CmarkParser::new_ext(source, Options::empty()).into_offset_iter();

        for (event, range) in cmark {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    heading_level = heading_level_to_u8(level);
                    heading_text.clear();
                    heading_start_line = line_index.line_at(range.start);
                }
                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;
                    let name = heading_text.trim().to_owned();
                    if name.is_empty() {
                        continue;
                    }

                    // Pop ancestors at this level or deeper — a new H2 closes
                    // a previous H2 or H3, but stays nested under an H1.
                    while matches!(stack.last(), Some((lvl, _)) if *lvl >= heading_level) {
                        stack.pop();
                    }
                    let parent_id = stack.last().map(|(_, id)| id.clone());

                    let id = NodeId::new();
                    let qualified_name = format!("{}#{}", path.display(), slugify(&name));
                    nodes.push(Node {
                        id: id.clone(),
                        kind: NodeKind::Section,
                        name,
                        qualified_name,
                        file: path.to_owned(),
                        span: Span {
                            start_line: heading_start_line,
                            end_line: heading_start_line,
                        },
                        metadata: NodeMetadata {
                            visibility: Visibility::Pub,
                            ..Default::default()
                        },
                    });
                    if let Some(parent_id) = parent_id {
                        edges.push(Edge::new(parent_id, id.clone(), EdgeKind::Contains));
                    }
                    stack.push((heading_level, id.clone()));
                    current_section = Some(id);
                }
                Event::Text(text) => {
                    if in_heading {
                        heading_text.push_str(&text);
                    } else if in_link {
                        link_text.push_str(&text);
                    }
                }
                Event::Start(Tag::Link { .. }) => {
                    in_link = true;
                    link_text.clear();
                }
                Event::End(TagEnd::Link) => {
                    in_link = false;
                    if let (Some(name), Some(src)) =
                        (candidate_symbol_name(&link_text), current_section.clone())
                    {
                        deferred_doc_refs.push((src, name));
                    }
                }
                Event::Code(code) => {
                    if let (Some(name), Some(src)) =
                        (candidate_symbol_name(&code), current_section.clone())
                    {
                        deferred_doc_refs.push((src, name));
                    }
                }
                _ => {}
            }
        }

        Ok(ParseResult {
            nodes,
            edges,
            deferred_calls: Vec::new(),
            deferred_uses: Vec::new(),
            deferred_implements: Vec::new(),
            deferred_imports: Vec::new(),
            deferred_inherits: Vec::new(),
            deferred_throws: Vec::new(),
            deferred_annotated: Vec::new(),
            deferred_doc_refs,
        })
    }
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Slugify heading text into a stable, URL-safe fragment for `qualified_name`
/// (e.g. "Quick Start" → "quick-start"). Not GitHub-anchor-perfect, just
/// stable and collision-resistant within a file.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut last_was_dash = true; // avoid leading dash
    for c in text.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Conservative heuristic: does `text` look like a code identifier worth
/// trying to resolve, rather than a shell command, file path, or prose
/// fragment? Returns the trailing identifier segment (after the last `::`
/// or `.`, with a trailing `()` stripped) to match against `Node::name`.
///
/// This is deliberately precision-first, not a fuzzy NLP matcher — known
/// limitation: hyphenated or otherwise non-identifier-shaped names never
/// match, by design.
fn candidate_symbol_name(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let without_call = trimmed.strip_suffix("()").unwrap_or(trimmed);

    // Reject anything containing whitespace, slashes, or other punctuation
    // that signals a shell command / file path / prose fragment rather than
    // a single qualified identifier.
    if without_call.is_empty()
        || !without_call
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':' || c == '.')
    {
        return None;
    }

    let last_segment = without_call
        .rsplit("::")
        .next()
        .unwrap_or(without_call)
        .rsplit('.')
        .next()
        .unwrap_or(without_call);

    let mut chars = last_segment.chars();
    let starts_ok = matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_');
    if !starts_ok || last_segment.len() < 3 {
        return None;
    }

    Some(last_segment.to_owned())
}

/// Precomputed newline byte offsets for O(log n) byte→line-number lookups,
/// since `pulldown-cmark`'s offset iterator yields byte ranges, not lines.
struct LineIndex {
    /// Byte offset of each `\n` in the source.
    newlines: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let newlines = source
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i)
            .collect();
        Self { newlines }
    }

    /// 1-indexed line number containing `byte_offset`.
    fn line_at(&self, byte_offset: usize) -> u32 {
        self.newlines.partition_point(|&nl| nl < byte_offset) as u32 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(md: &str) -> ParseResult {
        MarkdownParser::new()
            .parse(Path::new("README.md"), md)
            .unwrap()
    }

    #[test]
    fn headings_become_section_nodes() {
        let result = parse("# Title\n\n## Installation\n\nSome text.\n");
        let names: Vec<&str> = result.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["Title", "Installation"]);
        assert!(result.nodes.iter().all(|n| n.kind == NodeKind::Section));
    }

    #[test]
    fn nested_headings_produce_contains_edges() {
        let result = parse("# Title\n\n## Installation\n\n### Quick Start\n");
        assert_eq!(result.nodes.len(), 3);
        let contains: Vec<&Edge> = result
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .collect();
        assert_eq!(
            contains.len(),
            2,
            "Title->Installation, Installation->Quick Start"
        );
    }

    #[test]
    fn sibling_heading_closes_deeper_section() {
        // H2 "A" contains H3 "Sub". A following H2 "B" must NOT nest under "Sub".
        let result = parse("# Title\n\n## A\n\n### Sub\n\n## B\n");
        let b = result.nodes.iter().find(|n| n.name == "B").unwrap();
        let b_contains_edge = result
            .edges
            .iter()
            .find(|e| e.dst == b.id && e.kind == EdgeKind::Contains)
            .unwrap();
        let title = result.nodes.iter().find(|n| n.name == "Title").unwrap();
        assert_eq!(
            b_contains_edge.src, title.id,
            "B should nest under Title, not Sub"
        );
    }

    #[test]
    fn inline_code_span_matching_identifier_becomes_doc_ref() {
        let result = parse("# Title\n\nRun `validate_token` to check input.\n");
        assert_eq!(result.deferred_doc_refs.len(), 1);
        assert_eq!(result.deferred_doc_refs[0].1, "validate_token");
    }

    #[test]
    fn qualified_code_span_uses_trailing_segment() {
        let result = parse("# Title\n\nSee `auth::validate_token` for details.\n");
        assert_eq!(result.deferred_doc_refs[0].1, "validate_token");
    }

    #[test]
    fn non_symbol_code_spans_are_excluded() {
        let result = parse("# Title\n\nRun `npm install` then `cd ..`.\n");
        assert!(result.deferred_doc_refs.is_empty());
    }

    #[test]
    fn short_code_spans_are_excluded() {
        let result = parse("# Title\n\nUse `id` here.\n");
        assert!(result.deferred_doc_refs.is_empty());
    }

    #[test]
    fn doc_ref_before_first_heading_is_dropped() {
        let result = parse("Mentions `validate_token` before any heading.\n\n# Title\n");
        assert!(result.deferred_doc_refs.is_empty());
    }

    #[test]
    fn link_text_matching_identifier_becomes_doc_ref() {
        let result = parse("# Title\n\nSee [validate_token](src/auth.rs#L10) for details.\n");
        assert_eq!(result.deferred_doc_refs.len(), 1);
        assert_eq!(result.deferred_doc_refs[0].1, "validate_token");
    }

    #[test]
    fn slugify_handles_spaces_and_punctuation() {
        assert_eq!(slugify("Quick Start!"), "quick-start");
        assert_eq!(slugify("API Reference (v2)"), "api-reference-v2");
    }

    #[test]
    fn line_index_finds_correct_line() {
        let idx = LineIndex::new("a\nb\nc\n");
        assert_eq!(idx.line_at(0), 1); // 'a'
        assert_eq!(idx.line_at(2), 2); // 'b'
        assert_eq!(idx.line_at(4), 3); // 'c'
    }
}
