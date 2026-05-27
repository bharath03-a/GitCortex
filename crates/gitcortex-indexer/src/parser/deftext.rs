//! Language-agnostic capture of a node's source text — signature, body,
//! preceding doc-comment, and byte range. Filled during pass 1 from the
//! tree-sitter node's byte range; no extra parsing work.
//!
//! Powers wiki rendering, tour narration, and downstream search.

use gitcortex_core::graph::DefinitionText;
use tree_sitter::Node as TsNode;

/// Maximum bytes captured per node body. The body is stored per node and
/// re-serialised on every insert, so an oversized cap bloats both the store
/// and full-index write time (a 2k-method repo at 16 KB each = ~30 MB of body
/// text shovelled through Cypher). Queries surface the signature + doc, not
/// the full body, so 2 KB keeps enough head-of-function context for future
/// semantic use without the I/O tax.
const MAX_BODY_BYTES: usize = 2 * 1024;

/// Capture `DefinitionText` for `ts_node` from `source`.
///
/// - `signature`: the slice up to the first `{`, `:` (Python), or end-of-line
///   if no body delimiter is found. Trimmed.
/// - `body`: full slice of the node (signature + body), truncated to
///   `MAX_BODY_BYTES`.
/// - `doc_comment`: contiguous comment nodes immediately preceding `ts_node`
///   at the same parent level. Returns `None` when no such comments exist.
/// - `start_byte` / `end_byte`: byte offsets into the file.
pub(crate) fn capture(source: &[u8], ts_node: TsNode<'_>) -> DefinitionText {
    let start = ts_node.start_byte();
    let end = ts_node.end_byte();
    let raw = source.get(start..end).unwrap_or(&[]);
    let body_full = std::str::from_utf8(raw).unwrap_or("");
    let body = truncate_to_char_boundary(body_full, MAX_BODY_BYTES).to_owned();

    let signature = extract_signature(&body).to_owned();
    let doc_comment = preceding_doc_comment(source, ts_node)
        // Fallback: many languages (Python, Ruby) place the doc inside the
        // body as the first statement (`"""..."""` / `'''...'''`). Check the
        // body slice when no preceding-sibling comment was found.
        .or_else(|| inline_docstring(&body));

    DefinitionText {
        signature,
        body,
        doc_comment,
        start_byte: start as u32,
        end_byte: end as u32,
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
}
