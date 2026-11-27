//! Language-agnostic capture of a node's source text — signature, content
//! hash, preceding doc-comment, and byte range. Filled during pass 1 from the
//! tree-sitter node's byte range; no extra parsing work.
//!
//! Powers wiki rendering, tour narration, and downstream search.

use gitcortex_core::graph::DefinitionText;
use tree_sitter::Node as TsNode;

/// Maximum bytes of the node's source scanned for the signature and inline
/// docstring. Bounds the scan on huge single-line nodes (minified code); the
/// content hash always covers the full source range.
const MAX_BODY_BYTES: usize = 2 * 1024;

/// Capture `DefinitionText` for `ts_node` from `source`.
///
/// - `signature`: the slice up to the first `{`, `:` (Python), or end-of-line
///   if no body delimiter is found. Trimmed.
/// - `hash`: [`content_hash`] of the node's full source range, so node
///   comparison (branch diff, blast radius) sees edits anywhere in the body.
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
        hash: content_hash(raw),
        doc_comment,
        start_byte: start as u32,
        end_byte: end as u32,
    }
}

/// First 16 hex chars of blake3 over `raw`; empty for an empty range, matching
/// the "not captured" default. 64 bits is plenty: hashes are only compared
/// pairwise for the same node id.
fn content_hash(raw: &[u8]) -> String {
    if raw.is_empty() {
        return String::new();
    }
    blake3::hash(raw).to_hex()[..16].to_owned()
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
        assert_eq!(def.hash.len(), 16);
        assert_eq!(
            &src[def.start_byte as usize..def.end_byte as usize],
            "fn foo(a: u32) -> u32 {\n    a + 1\n}"
        );
    }

    /// Parse Python `src` and return `name -> content hash`.
    fn python_hashes(src: &str) -> std::collections::HashMap<String, String> {
        use crate::parser::LanguageParser;
        crate::parser::python::PythonParser::new()
            .parse(std::path::Path::new("m.py"), src)
            .expect("parse")
            .nodes
            .into_iter()
            .filter(|n| !n.metadata.definition.hash.is_empty())
            .map(|n| (n.name, n.metadata.definition.hash))
            .collect()
    }

    const CLASS_SRC: &str = "class User:\n    def save(self):\n        return 41\n\n    def load(self):\n        return 1\n";

    #[test]
    fn hash_is_deterministic() {
        let hashes = python_hashes(CLASS_SRC);
        assert_eq!(hashes["save"].len(), 16);
        assert_eq!(hashes, python_hashes(CLASS_SRC));
    }

    #[test]
    fn same_length_body_edit_changes_hash() {
        // `41` -> `42` keeps every span and byte offset identical, so the
        // hash is the only field that can tell the two versions apart.
        let before = python_hashes(CLASS_SRC);
        let after = python_hashes(&CLASS_SRC.replace("41", "42"));
        assert_ne!(before["save"], after["save"]);
        assert_ne!(before["User"], after["User"], "full range includes methods");
        assert_eq!(before["load"], after["load"]);
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
