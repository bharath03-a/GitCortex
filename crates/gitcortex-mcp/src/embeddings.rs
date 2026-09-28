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
const FORMAT_VERSION: u32 = 3;
type Fingerprint = [u8; 32];
type StoredIndex = (HashMap<String, Vec<f32>>, HashMap<String, Fingerprint>);

// ── Vector index ──────────────────────────────────────────────────────────────

pub struct SemanticIndex {
    /// node_id → unit-normalised embedding
    vectors: HashMap<String, Vec<f32>>,
    fingerprints: HashMap<String, Fingerprint>,
    path: PathBuf,
}

impl SemanticIndex {
    pub fn load_or_create(path: &Path) -> Self {
        let (vectors, fingerprints) = load_bin(path).unwrap_or_default();
        if !vectors.is_empty() {
            tracing::info!(
                "semantic index loaded: {} vectors from {}",
                vectors.len(),
                path.display()
            );
        }
        Self {
            vectors,
            fingerprints,
            path: path.to_owned(),
        }
    }

    pub fn has(&self, node_id: &str) -> bool {
        self.vectors.contains_key(node_id)
    }

    pub fn insert(&mut self, node_id: String, vec: Vec<f32>, fingerprint: Fingerprint) {
        self.fingerprints.insert(node_id.clone(), fingerprint);
        self.vectors.insert(node_id, unit_normalise(vec));
    }

    pub fn needs_embedding(&self, node: &Node) -> bool {
        self.fingerprints.get(&node.id.as_str()) != Some(&node_fingerprint(node))
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Drop vectors whose node ID is not in `live_ids`. Stable IDs preserve
    /// vectors across edits, while deleted/renamed nodes must still be pruned.
    /// Returns the number of vectors removed.
    pub fn retain_ids(&mut self, live_ids: &std::collections::HashSet<String>) -> usize {
        let before = self.vectors.len();
        self.vectors.retain(|id, _| live_ids.contains(id));
        self.fingerprints.retain(|id, _| live_ids.contains(id));
        before - self.vectors.len()
    }

    pub fn save(&self) {
        if let Err(e) = save_bin(&self.path, &self.vectors, &self.fingerprints) {
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
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
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

pub fn node_fingerprint(node: &Node) -> Fingerprint {
    *blake3::hash(node_text(node).as_bytes()).as_bytes()
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
//     [32]      BLAKE3 fingerprint of semantic node text
//     [dim × 4] f32 values

fn load_bin(path: &Path) -> Option<StoredIndex> {
    let data = std::fs::read(path).ok()?;
    let mut p = 0usize;

    macro_rules! read_u32 {
        () => {{
            let end = p.checked_add(4)?;
            let b: [u8; 4] = data.get(p..end)?.try_into().ok()?;
            p = end;
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
    if dim != DIM {
        return None;
    }
    let count = read_u32!() as usize;
    let vector_bytes = dim.checked_mul(4)?;
    let minimum_record_bytes = 4usize.checked_add(32)?.checked_add(vector_bytes)?;
    if count > data.len().saturating_sub(p) / minimum_record_bytes {
        return None;
    }

    let mut vectors = HashMap::with_capacity(count);
    let mut fingerprints = HashMap::with_capacity(count);
    for _ in 0..count {
        let id_len = read_u32!() as usize;
        let id_end = p.checked_add(id_len)?;
        let id = String::from_utf8(data.get(p..id_end)?.to_vec()).ok()?;
        p = id_end;
        let fingerprint_end = p.checked_add(32)?;
        let fingerprint: Fingerprint = data.get(p..fingerprint_end)?.try_into().ok()?;
        p = fingerprint_end;
        let end = p.checked_add(vector_bytes)?;
        let vec: Vec<f32> = data
            .get(p..end)?
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        p = end;
        fingerprints.insert(id.clone(), fingerprint);
        vectors.insert(id, vec);
    }
    Some((vectors, fingerprints))
}

fn save_bin(
    path: &Path,
    vectors: &HashMap<String, Vec<f32>>,
    fingerprints: &HashMap<String, Fingerprint>,
) -> std::io::Result<()> {
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
            w.write_all(fingerprints.get(id).unwrap_or(&[0; 32]))?;
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
    fn stable_node_id_reembeds_when_semantic_text_changes() {
        let mut original = make_node("parse_json", "util::parse_json", "fn parse_json()", "old");
        original.id = NodeId::stable("semantic-refresh-test");
        let mut index = SemanticIndex {
            vectors: HashMap::new(),
            fingerprints: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        index.insert(
            original.id.as_str(),
            vec![1.0; DIM],
            node_fingerprint(&original),
        );
        assert!(!index.needs_embedding(&original));

        let mut changed = original.clone();
        changed.metadata.definition.signature = "fn parse_json(input: &str)".to_owned();
        assert!(index.needs_embedding(&changed));
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
        let fingerprints = HashMap::from([
            ("node-1".to_owned(), [1; 32]),
            ("node-2".to_owned(), [2; 32]),
        ]);
        save_bin(&path, &vecs, &fingerprints).unwrap();
        let (loaded, loaded_fingerprints) = load_bin(&path).expect("should load v3 file");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded_fingerprints["node-1"], [1; 32]);
    }

    #[test]
    fn top_k_returns_scores_in_range() {
        let mut index = SemanticIndex {
            vectors: HashMap::new(),
            fingerprints: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        let v: Vec<f32> = {
            let mut raw = vec![0.0f32; 384];
            raw[0] = 1.0;
            raw
        };
        index.insert("a".to_owned(), v.clone(), [1; 32]);
        index.insert("b".to_owned(), v.clone(), [2; 32]);
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
            fingerprints: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        let v: Vec<f32> = {
            let mut raw = vec![0.0f32; 384];
            raw[0] = 1.0;
            raw
        };
        for i in 0..20u32 {
            index.insert(format!("node-{i}"), v.clone(), [i as u8; 32]);
        }
        let results = index.top_k(&v, 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn top_k_breaks_equal_similarity_by_node_id() {
        let mut index = SemanticIndex {
            vectors: HashMap::new(),
            fingerprints: HashMap::new(),
            path: PathBuf::from("/tmp/unused"),
        };
        let vector = vec![1.0; DIM];
        index.insert("b".to_owned(), vector.clone(), [1; 32]);
        index.insert("a".to_owned(), vector.clone(), [2; 32]);
        let ids: Vec<_> = index
            .top_k(&vector, 2)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn load_bin_rejects_wrong_dimension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wrong-dim.bin");
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        std::fs::write(&path, buf).unwrap();
        assert!(load_bin(&path).is_none());
    }

    #[test]
    fn load_bin_rejects_impossible_record_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge-count.bin");
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf.extend_from_slice(&(DIM as u32).to_le_bytes());
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&path, buf).unwrap();
        assert!(load_bin(&path).is_none());
    }
}
