//! Language-agnostic capture of a node's source text — signature, own-code
//! hash, preceding doc-comment, and byte range. Filled during pass 1 from the
//! tree-sitter node's byte range; no extra parsing work.
//!
//! Powers wiki rendering, tour narration, and downstream search.

use std::{cmp::Reverse, collections::HashMap};

use gitcortex_core::graph::{DefinitionText, Node};
use tree_sitter::Node as TsNode;

/// Maximum bytes of the node's source scanned for the signature and inline
/// docstring. Bounds the scan on huge single-line nodes (minified code); the
/// own-code hash always covers the full source.
const MAX_BODY_BYTES: usize = 2 * 1024;

/// Separator hashed in place of each cut-out nested definition. Never valid
/// UTF-8, so it cannot collide with source text. Adding or removing a nested
/// definition therefore changes the parent's hash; editing one does not.
const NESTED_MARKER: &[u8] = &[0xFF];

/// Capture `DefinitionText` for `ts_node` from `source`.
///
/// - `signature`: the slice up to the first `{`, `:` (Python), or end-of-line
///   if no body delimiter is found. Trimmed.
/// - `hash`: left empty here; filled per file by [`assign_own_code_hashes`],
///   which needs every node's range to cut out nested definitions.
/// - `doc_comment`: contiguous comment nodes immediately preceding `ts_node`
///   at the same parent level. Returns `None` when no such comments exist.
/// - `start_byte` / `end_byte`: byte offsets into the file.
pub(crate) fn capture(source: &[u8], ts_node: TsNode<'_>) -> DefinitionText {
    let start = ts_node.start_byte();
    let end = ts_node.end_byte();
    let raw = source.get(start..end).unwrap_or(&[]);
    let body_full = std::str::from_utf8(raw).unwrap_or("");
    let head = truncate_to_char_boundary(body_full, MAX_BODY_BYTES);

    let signature = extract_signature(head).to_owned();
    let doc_comment = preceding_doc_comment(source, ts_node)
        // Fallback: many languages (Python, Ruby) place the doc inside the
        // body as the first statement (`"""..."""` / `'''...'''`). Check the
        // body slice when no preceding-sibling comment was found.
        .or_else(|| inline_docstring(head));

    DefinitionText {
        signature,
        hash: String::new(),
        doc_comment,
        start_byte: start as u32,
        end_byte: end as u32,
    }
}

/// Fill `definition.hash` for every node of one parsed file.
///
/// The hash covers the node's *own* code: its source range with the ranges of
/// directly nested definitions replaced by [`NESTED_MARKER`]. Editing a method
/// changes the method's hash but not its class's, so node comparison (branch
/// diff, blast radius) flags only what was actually edited. Nodes without a
/// captured range (`start_byte == end_byte`) keep an empty hash.
pub(crate) fn assign_own_code_hashes(nodes: &mut [Node], source: &[u8]) {
    // Distinct captured ranges, parents before children: earlier start first,
    // wider range first on ties. Nodes sharing a range share a hash.
    let mut ranges: Vec<(u32, u32)> = nodes
        .iter()
        .map(|n| {
            (
                n.metadata.definition.start_byte,
                n.metadata.definition.end_byte,
            )
        })
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort_by_key(|&(start, end)| (start, Reverse(end)));
    ranges.dedup();

    // Direct children of each range, found with a containment stack.
    let mut children: Vec<Vec<(u32, u32)>> = vec![Vec::new(); ranges.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (idx, &(start, end)) in ranges.iter().enumerate() {
        while let Some(&top) = stack.last() {
            let (top_start, top_end) = ranges[top];
            if start >= top_start && end <= top_end {
                break;
            }
            stack.pop();
        }
        if let Some(&parent) = stack.last() {
            children[parent].push((start, end));
        }
        stack.push(idx);
    }

    let slice = |from: u32, to: u32| source.get(from as usize..to as usize).unwrap_or(&[]);
    let hashes: HashMap<(u32, u32), String> = ranges
        .iter()
        .zip(&children)
        .map(|(&(start, end), kids)| {
            let mut hasher = blake3::Hasher::new();
            let mut cursor = start;
            for &(kid_start, kid_end) in kids {
                hasher.update(slice(cursor, kid_start.max(cursor)));
                hasher.update(NESTED_MARKER);
                cursor = cursor.max(kid_end);
            }
            hasher.update(slice(cursor, end));
            ((start, end), hasher.finalize().to_hex()[..16].to_owned())
        })
        .collect();

    for node in nodes {
        let def = &mut node.metadata.definition;
        if let Some(hash) = hashes.get(&(def.start_byte, def.end_byte)) {
            def.hash = hash.clone();
        }
    }
}

/// Signature = lines from the start up to (and including) the line that opens
/// the body. Body-opener heuristic: line ends with `{`, `:`, `=>`, or `=`
/// after stripping trailing whitespace. Falls back to first non-empty line.
///
/// Scans line-by-line (not byte-by-byte) to avoid mistaking braces inside
/// f-strings / template literals on the first line as the body opener.
fn extract_signature(body: &str) -> &str {
    let mut consumed: usize = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end();
        consumed += line.len();
        if trimmed.ends_with('{')
            || trimmed.ends_with(':')
            || trimmed.ends_with("=>")
            || trimmed.ends_with('=')
        {
            // Include the opener line, then strip trailing `{` and any
            // whitespace that preceded it (cosmetic — body follows on the
            // next line). Keeps `:` and `=>` since those are syntactically
            // part of the signature (Python, arrow functions).
            let sig = body[..consumed].trim_end();
            let sig = sig.strip_suffix('{').unwrap_or(sig).trim_end();
            return sig;
        }
    }
    body.lines().next().unwrap_or("").trim_end()
}

/// Truncate `s` to at most `max_bytes`, snapping back to a UTF-8 char boundary.
fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}

/// Walk preceding siblings of `ts_node`, collecting contiguous comment nodes.
/// Returns combined text (with original newlines) or `None` if none found.
///
/// Recognises tree-sitter comment kinds across the supported languages:
/// `line_comment`, `block_comment` (Rust), `comment` (everything else).
fn preceding_doc_comment(source: &[u8], ts_node: TsNode<'_>) -> Option<String> {
    let parent = ts_node.parent()?;
    let mut cursor = parent.walk();
    let siblings: Vec<TsNode<'_>> = parent.named_children(&mut cursor).collect();
    let pos = siblings.iter().position(|n| n.id() == ts_node.id())?;
    if pos == 0 {
        return None;
    }

    let mut comments: Vec<&str> = Vec::new();
    for sib in siblings[..pos].iter().rev() {
        let kind = sib.kind();
        if kind == "line_comment" || kind == "block_comment" || kind == "comment" {
            let text = sib.utf8_text(source).unwrap_or("");
            // Only keep doc-style comments; skip regular code-comments to avoid
            // capturing unrelated chatter. Conservative: `///`, `//!`, `/**`,
            // and Python/Go `#`/`//` that look like real docs (heuristic: more
            // than two consecutive lines OR a leading capital letter).
            if is_doc_style(text) {
                comments.push(text);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if comments.is_empty() {
        return None;
    }
    comments.reverse();
    Some(comments.join("\n"))
}

/// Extract a Python-style docstring from `body` — a `"""..."""` (or `'''...'''`)
/// string literal that appears as the first non-empty statement after the
/// signature line. Returns `None` when not found.
fn inline_docstring(body: &str) -> Option<String> {
    // Skip the signature line(s) up to and including the opener (`:` for
    // Python). We rely on the post-signature region beginning after the first
    // line that ends with `:`.
    let mut after_sig = body;
    for (idx, line) in body.split_inclusive('\n').enumerate() {
        if line.trim_end().ends_with(':') {
            // Take everything after this line.
            let consumed: usize = body.split_inclusive('\n').take(idx + 1).map(str::len).sum();
            after_sig = &body[consumed..];
            break;
        }
    }

    let trimmed = after_sig.trim_start();
    for marker in ["\"\"\"", "'''"] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if let Some(end) = rest.find(marker) {
                let inner = rest[..end].trim();
                if !inner.is_empty() {
                    return Some(inner.to_owned());
                }
            }
        }
    }
    None
}

/// Heuristic: does this comment look like a doc-comment worth capturing?
fn is_doc_style(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("///")
        || t.starts_with("//!")
        || t.starts_with("/**")
        || t.starts_with("\"\"\"")
        || t.starts_with("'''")
        // Plain `//` or `#` lines: keep — caller pieces them together;
        // upstream language layers can drop noise later.
        || t.starts_with("//")
        || t.starts_with("#")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_stops_at_brace() {
        assert_eq!(
            extract_signature("fn foo(a: u32) -> u32 {\n    a + 1\n}"),
            "fn foo(a: u32) -> u32"
        );
    }

    #[test]
    fn signature_python_def() {
        assert_eq!(
            extract_signature("def greet(name):\n    return f'hi {name}'"),
            "def greet(name):"
        );
    }

    #[test]
    fn signature_falls_back_to_first_line() {
        assert_eq!(extract_signature("const X = 42;"), "const X = 42;");
    }

    #[test]
    fn python_docstring_extracted() {
        let body = "def greet(name):\n    \"\"\"Return a greeting.\"\"\"\n    return name\n";
        assert_eq!(
            inline_docstring(body).as_deref(),
            Some("Return a greeting.")
        );
    }

    #[test]
    fn python_docstring_single_quotes() {
        let body = "def greet(name):\n    '''hi'''\n";
        assert_eq!(inline_docstring(body).as_deref(), Some("hi"));
    }

    #[test]
    fn no_docstring_returns_none() {
        let body = "def greet(name):\n    return name\n";
        assert!(inline_docstring(body).is_none());
    }

    #[test]
    fn truncate_respects_char_boundary() {
        let s = "héllo"; // 'é' = 2 bytes
        let out = truncate_to_char_boundary(s, 2);
        assert!(s.starts_with(out));
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn capture_keeps_signature_and_range() {
        let src = "/// Adds one.\nfn foo(a: u32) -> u32 {\n    a + 1\n}\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("rust grammar");
        let tree = parser.parse(src, None).expect("parse");
        let root = tree.root_node();
        let mut cursor = root.walk();
        let func = root
            .named_children(&mut cursor)
            .find(|n| n.kind() == "function_item")
            .expect("function_item");

        let def = capture(src.as_bytes(), func);

        assert_eq!(def.signature, "fn foo(a: u32) -> u32");
        assert!(def.hash.is_empty(), "hash is filled per file, not here");
        assert_eq!(
            &src[def.start_byte as usize..def.end_byte as usize],
            "fn foo(a: u32) -> u32 {\n    a + 1\n}"
        );
    }

    /// Parse Python `src` and return `name -> own-code hash`.
    fn python_hashes(src: &str) -> HashMap<String, String> {
        use crate::parser::LanguageParser;
        let parsed = crate::parser::python::PythonParser::new()
            .parse(std::path::Path::new("m.py"), src)
            .expect("parse");
        let mut nodes = parsed.nodes;
        assign_own_code_hashes(&mut nodes, src.as_bytes());
        nodes
            .into_iter()
            .filter(|n| !n.metadata.definition.hash.is_empty())
            .map(|n| (n.name, n.metadata.definition.hash))
            .collect()
    }

    const CLASS_SRC: &str = "class User:\n    def save(self):\n        return 41\n\n    def load(self):\n        return 1\n";

    #[test]
    fn own_hash_is_deterministic() {
        let hashes = python_hashes(CLASS_SRC);
        assert_eq!(hashes["save"].len(), 16);
        assert_eq!(hashes, python_hashes(CLASS_SRC));
    }

    #[test]
    fn editing_a_method_changes_only_that_method() {
        let before = python_hashes(CLASS_SRC);
        let after = python_hashes(&CLASS_SRC.replace("41", "42"));
        assert_ne!(before["save"], after["save"]);
        assert_eq!(before["load"], after["load"]);
        assert_eq!(before["User"], after["User"], "class own code unchanged");
    }

    #[test]
    fn adding_a_method_changes_the_class() {
        let before = python_hashes(CLASS_SRC);
        let after = python_hashes(&format!("{CLASS_SRC}\n    def drop(self):\n        pass\n"));
        assert_ne!(before["User"], after["User"]);
    }

    #[test]
    fn edit_past_scan_window_is_detected() {
        let filler = "    x = 1\n".repeat(MAX_BODY_BYTES / 4);
        let src = format!("def long():\n{filler}    return 41\n");
        let before = python_hashes(&src);
        let after = python_hashes(&src.replace("41", "42"));
        assert_ne!(before["long"], after["long"]);
    }
}
