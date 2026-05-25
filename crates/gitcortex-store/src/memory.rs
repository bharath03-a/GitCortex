//! In-memory stub backend — enables `cargo test --no-default-features
//! --features memory` without linking the kuzu C++ library. Not suitable
//! for production use; state is lost when the process exits.

use std::path::Path;

use gitcortex_core::{
    error::{GitCortexError, Result},
    graph::{Edge, GraphDiff, Node},
    schema::NodeKind,
    store::{CallersDeep, GraphStore, SubGraph, SymbolContext},
};

#[derive(Default)]
pub struct MemoryGraphStore;

impl MemoryGraphStore {
    pub fn open(_repo_root: &Path) -> Result<Self> {
        Ok(Self)
    }
}

impl GraphStore for MemoryGraphStore {
    fn apply_diff(&mut self, _branch: &str, _diff: &GraphDiff) -> Result<()> {
        Ok(())
    }

    fn lookup_symbol(&self, _branch: &str, _name: &str, _fuzzy: bool) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn find_callers(&self, _branch: &str, _function_name: &str) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn find_callers_deep(
        &self,
        _branch: &str,
        _function_name: &str,
        _depth: u8,
    ) -> Result<CallersDeep> {
        Ok(CallersDeep {
            hops: vec![],
            risk_level: "none",
        })
    }

    fn symbol_context(&self, _branch: &str, name: &str) -> Result<SymbolContext> {
        Err(GitCortexError::Store(format!(
            "memory backend: symbol '{name}' not found (store is empty)"
        )))
    }

    fn list_definitions(&self, _branch: &str, _file: &Path) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn list_all_nodes(&self, _branch: &str) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn list_all_edges(&self, _branch: &str) -> Result<Vec<Edge>> {
        Ok(vec![])
    }

    fn branch_diff(&self, _from: &str, _to: &str) -> Result<GraphDiff> {
        Ok(GraphDiff::default())
    }

    fn find_callees(&self, _branch: &str, _function_name: &str, _depth: u8) -> Result<CallersDeep> {
        Ok(CallersDeep {
            hops: vec![],
            risk_level: "none",
        })
    }

    fn find_implementors(
        &self,
        _branch: &str,
        _trait_or_interface_name: &str,
    ) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn trace_path(&self, _branch: &str, _from: &str, _to: &str) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn list_symbols_in_range(
        &self,
        _branch: &str,
        _file: &Path,
        _start_line: u32,
        _end_line: u32,
    ) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn find_unused_symbols(&self, _branch: &str, _kind: Option<NodeKind>) -> Result<Vec<Node>> {
        Ok(vec![])
    }

    fn get_subgraph(
        &self,
        _branch: &str,
        _seed_name: &str,
        _depth: u8,
        _direction: &str,
    ) -> Result<SubGraph> {
        Ok(SubGraph {
            nodes: vec![],
            edges: vec![],
        })
    }

    fn last_indexed_sha(&self, _branch: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn set_last_indexed_sha(&mut self, _branch: &str, _sha: &str) -> Result<()> {
        Ok(())
    }
}
