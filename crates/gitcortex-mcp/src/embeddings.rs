//! Semantic search via local embeddings (AllMiniLM-L6-v2, 384 dims).
//!
//! Model is downloaded from HuggingFace on first use (~23 MB), cached in
//! `$XDG_DATA_HOME/gitcortex/models` (never inside a repo). All subsequent
//! starts load from cache.
//!
//! Vector index is persisted per-branch at:
//!   `~/.local/share/gitcortex/{repo_id}/embeddings_{branch}.bin`
//!
//! Background indexer (`index_missing`) embeds nodes that don't yet have a
//! vector. Call it once after `gcx serve` opens the store. Search stays
//! text-only while the indexer runs; it automatically uses semantic hits once
//! at least one vector is loaded.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use gitcortex_core::graph::Node;

use crate::mcp::search::tokenize;

/// Minimum cosine similarity to surface as a semantic hit.
const SIMILARITY_THRESHOLD: f32 = 0.50;
const DIM: usize = 384;

// Binary format: magic + version + dim + count + entries
const MAGIC: &[u8; 4] = b"GCXV";
const FORMAT_VERSION: u32 = 2;

// ── Vector index ──────────────────────────────────────────────────────────────

pub struct SemanticIndex {
    /// node_id → unit-normalised embedding
    vectors: HashMap<String, Vec<f32>>,
    path: PathBuf,
}

impl SemanticIndex {
    pub fn load_or_create(path: &Path) -> Self {
        let vectors = load_bin(path).unwrap_or_default();
        if !vectors.is_empty() {
            tracing::info!(
                "semantic index loaded: {} vectors from {}",
                vectors.len(),
                path.display()
            );
        }
        Self {
            vectors,
            path: path.to_owned(),
        }
    }

    pub fn has(&self, node_id: &str) -> bool {
        self.vectors.contains_key(node_id)
    }

    pub fn insert(&mut self, node_id: String, vec: Vec<f32>) {
        self.vectors.insert(node_id, unit_normalise(vec));
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Drop vectors whose node ID is not in `live_ids`. Node UUIDs regenerate
    /// on every re-index, so without pruning the index file grows with
    /// orphaned vectors that can still surface as (unresolvable) hits.
    /// Returns the number of vectors removed.
    pub fn retain_ids(&mut self, live_ids: &std::collections::HashSet<String>) -> usize {
        let before = self.vectors.len();
        self.vectors.retain(|id, _| live_ids.contains(id));
        before - self.vectors.len()
    }

    pub fn save(&self) {
        if let Err(e) = save_bin(&self.path, &self.vectors) {
            tracing::warn!("failed to save semantic index: {e}");
        }
    }

    /// Return up to `k` `(node_id, similarity)` pairs with cosine similarity ≥ SIMILARITY_THRESHOLD.
    /// Query vector need not be pre-normalised — normalised internally.
    pub fn top_k(&self, query_vec: &[f32], k: usize) -> Vec<(String, f32)> {
        let q = unit_normalise(query_vec.to_vec());
        let mut scores: Vec<(&String, f32)> = self
            .vectors
            .iter()
            .map(|(id, v)| (id, dot(&q, v)))
            .filter(|(_, s)| *s >= SIMILARITY_THRESHOLD)
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
            .into_iter()
            .take(k)
            .map(|(id, s)| (id.clone(), s))
            .collect()
    }
}

// ── Embedder ──────────────────────────────────────────────────────────────────

pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    /// Download (first run) or load (cached) AllMiniLM-L6-v2.
    ///
    /// `cache_dir` is where fastembed stores the downloaded model weights.
    /// Pass `branch::models_dir()` so the cache lands in
    /// `$XDG_DATA_HOME/gitcortex/models`, never inside a repo.
    pub fn new(cache_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        tracing::info!("initialising semantic embedder (AllMiniLM-L6-v2) …");
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(false)
                .with_cache_dir(cache_dir.to_path_buf()),
        )?;
        tracing::info!("semantic embedder ready");
        Ok(Self { model })
    }

    pub fn embed_one(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut out = self.model.embed(vec![text.to_owned()], None)?;
        out.pop()
            .ok_or_else(|| anyhow::anyhow!("embedder returned no vectors"))
    }

    /// Embed a batch of texts. Returns one vector per input in order.
    pub fn embed_batch(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        self.model.embed(texts, None)
    }
}

// ── Text representation for a node ────────────────────────────────────────────

/// Build the text string that gets embedded for a node.
///
/// Appends tokenized identifier words (CamelCase/snake_case → space-separated
/// lowercase) so NL queries like "validate token" match `validate_token`
/// without relying on the model to unsplit glued identifiers.
pub fn node_text(n: &Node) -> String {
    let kind = n.kind.to_string();
    let sig = &n.metadata.definition.signature;
    let doc = n.metadata.definition.doc_comment.as_deref().unwrap_or("");

    // Tokenize the simple name and the last segment of the qualified path.
    let name_words = tokenize(&n.name).join(" ");
    let qname_last = n
        .qualified_name
        .rsplit("::")
        .next()
        .unwrap_or(&n.qualified_name);
    let qname_words = if qname_last != n.name {
        tokenize(qname_last).join(" ")
    } else {
        String::new()
    };

    let mut parts = vec![kind.as_str(), n.qualified_name.as_str()];
    if !sig.is_empty() {
        parts.push(sig.as_str());
    }
    if !doc.is_empty() {
        parts.push(doc);
    }
    parts.push(name_words.as_str());
    if !qname_words.is_empty() {
        parts.push(qname_words.as_str());
    }
    parts.join(" ")
}

// ── Math helpers ──────────────────────────────────────────────────────────────

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn unit_normalise(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

// ── Binary storage ────────────────────────────────────────────────────────────
//
// Layout (all integers little-endian):
//   [4]  magic "GCXV"
//   [4]  format version (u32)
//   [4]  embedding dimension (u32)
//   [4]  record count (u32)
//   per record:
//     [4]       id_len (u32)
//     [id_len]  node_id (UTF-8)
//     [dim × 4] f32 values

fn load_bin(path: &Path) -> Option<HashMap<String, Vec<f32>>> {
    let data = std::fs::read(path).ok()?;
    let mut p = 0usize;

    macro_rules! read_u32 {
        () => {{
            let b: [u8; 4] = data.get(p..p + 4)?.try_into().ok()?;
            p += 4;
            u32::from_le_bytes(b)
        }};
    }

    if data.get(p..p + 4)? != MAGIC {
        return None;
    }
    p += 4;

    let ver = read_u32!();
    if ver != FORMAT_VERSION {
        return None;
    }
    let dim = read_u32!() as usize;
    let count = read_u32!() as usize;

    let mut map = HashMap::with_capacity(count);
    for _ in 0..count {
        let id_len = read_u32!() as usize;
        let id = String::from_utf8(data.get(p..p + id_len)?.to_vec()).ok()?;
        p += id_len;
        let end = p + dim * 4;
        let vec: Vec<f32> = data
            .get(p..end)?
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        p = end;
        map.insert(id, vec);
    }
    Some(map)
}

fn save_bin(path: &Path, vectors: &HashMap<String, Vec<f32>>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let f = std::fs::File::create(&tmp)?;
        let mut w = BufWriter::new(f);
        w.write_all(MAGIC)?;
        w.write_all(&FORMAT_VERSION.to_le_bytes())?;
        w.write_all(&(DIM as u32).to_le_bytes())?;
        w.write_all(&(vectors.len() as u32).to_le_bytes())?;
        for (id, vec) in vectors {
            let id_b = id.as_bytes();
            w.write_all(&(id_b.len() as u32).to_le_bytes())?;
            w.write_all(id_b)?;
            for &v in vec {
                w.write_all(&v.to_le_bytes())?;
            }
        }
        w.flush()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcortex_core::graph::{NodeId, NodeMetadata, Span};
    use gitcortex_core::schema::NodeKind;
    use std::path::PathBuf;

    fn make_node(name: &str, qualified_name: &str, sig: &str, doc: &str) -> Node {
        let mut meta = NodeMetadata::default();
        meta.definition.signature = sig.to_owned();
        meta.definition.doc_comment = if doc.is_empty() {
            None
        } else {
            Some(doc.to_owned())
        };
        Node {
            id: NodeId::default(),
            kind: NodeKind::Function,
            name: name.to_owned(),
            qualified_name: qualified_name.to_owned(),
            file: PathBuf::from("src/lib.rs"),
            span: Span {
                start_line: 1,
                end_line: 5,
            },
            metadata: meta,
        }
    }

    #[test]
    fn node_text_contains_tokenized_words() {
        let n = make_node(
            "validate_token",
            "auth::validate_token",
            "fn validate_token(t: &str) -> bool",
            "",
        );
        let text = node_text(&n);
        assert!(
            text.contains("validate token"),
            "expected 'validate token' in: {text}"
        );
        assert!(
            text.contains("auth::validate_token"),
            "expected qualified name in: {text}"
        );
    }

    #[test]
    fn node_text_qualified_segment_tokenized_when_differs_from_name() {
        let n = make_node("new", "http::HttpClient::new", "", "");
        let text = node_text(&n);
        assert!(text.contains("new"), "expected 'new' in: {text}");
    }

    #[test]
    fn node_text_includes_doc_and_sig() {
        let n = make_node(
            "parse_json",
            "util::parse_json",
            "fn parse_json(s: &str) -> Value",
            "Parse a JSON string.",
        );
        let text = node_text(&n);
        assert!(text.contains("Parse a JSON string."));
        assert!(text.contains("fn parse_json"));
        assert!(text.contains("parse json"));
    }

    #[test]
    fn load_bin_rejects_stale_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GCXV");
        buf.extend_from_slice(&1u32.to_le_bytes()); // old version
        buf.extend_from_slice(&384u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        std::fs::write(&path, &buf).unwrap();
        assert!(load_bin(&path).is_none(), "v1 file should be rejected");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idx.bin");
        let mut vecs: HashMap<String, Vec<f32>> = HashMap::new();
        vecs.insert("node-1".to_owned(), vec![1.0; 384]);
        vecs.insert("node-2".to_owned(), vec![0.5; 384]);
        save_bin(&path, &vecs).unwrap();
        let loaded = load_bin(&path).expect("should load v2 file");
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains_key("node-1"));
    }

    #[test]
    fn top_k_returns_scores_in_range() {
        let mut index = SemanticIndex {
            vectors: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        let v: Vec<f32> = {
            let mut raw = vec![0.0f32; 384];
            raw[0] = 1.0;
            raw
        };
        index.insert("a".to_owned(), v.clone());
        index.insert("b".to_owned(), v.clone());
        let results = index.top_k(&v, 10);
        assert_eq!(results.len(), 2);
        for (_, score) in &results {
            assert!(
                *score >= SIMILARITY_THRESHOLD,
                "score {score} below threshold"
            );
            assert!(*score <= 1.001, "score {score} above 1.0");
        }
        assert!(results[0].1 >= results[1].1);
    }

    #[test]
    fn top_k_respects_k_limit() {
        let mut index = SemanticIndex {
            vectors: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        let v: Vec<f32> = {
            let mut raw = vec![0.0f32; 384];
            raw[0] = 1.0;
            raw
        };
        for i in 0..20u32 {
            index.insert(format!("node-{i}"), v.clone());
        }
        let results = index.top_k(&v, 5);
        assert_eq!(results.len(), 5);
    }
}
