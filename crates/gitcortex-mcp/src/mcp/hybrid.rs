//! Reciprocal Rank Fusion (RRF) over lexical + semantic ranked lists.
//!
//! Formula: score(d) = Σ_i  1 / (k + rank_i(d))
//! where k=60 (Cormack et al. 2009), ranks are 1-based, 0 contribution when absent.

use super::search::SearchHit;

const RRF_K: f64 = 60.0;

/// Merge lexical and semantic ranked lists via RRF.
///
/// Returns node IDs ordered by RRF score descending, deduplicated, capped at `limit`.
/// Lexical hits carry `id`; semantic hits are `(node_id, cosine_sim)` sorted desc.
pub(crate) fn rrf_merge(
    lexical: &[SearchHit],
    semantic: &[(String, f32)],
    limit: usize,
) -> Vec<String> {
    use std::collections::HashMap;

    let mut scores: HashMap<String, f64> = HashMap::new();

    for (rank, hit) in lexical.iter().enumerate() {
        *scores.entry(hit.id.clone()).or_default() += 1.0 / (RRF_K + (rank + 1) as f64);
    }
    for (rank, (id, _sim)) in semantic.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + (rank + 1) as f64);
    }

    let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);
    ranked.into_iter().map(|(id, _)| id).collect()
}
