export interface RawNode {
  id: string;
  name: string;
  kind: string;
  file: string;
  start_line: number;
  end_line: number;
  qualified_name: string;
  loc: number;
  visibility: string;
  is_async: boolean;
  is_unsafe: boolean;
}

export interface RawEdge {
  src: string;
  dst: string;
  kind: string;
  line?: number | null;
  confidence?: "extracted" | "resolved" | "inferred";
}

export interface GraphData {
  nodes: RawNode[];
  edges: RawEdge[];
}

export async function fetchGraphData(): Promise<GraphData> {
  const r = await fetch("/data");
  if (!r.ok) throw new Error(`/data returned ${r.status}`);
  return r.json();
}

export interface GraphManifest {
  branch: string;
  snapshot: string | null;
  total_nodes: number;
  total_edges: number;
  nodes_by_kind: [string, number][];
  edges_by_kind: [string, number][];
  recommended_chunk: number;
  max_chunk: number;
}

export interface GraphLoadProgress {
  stage: "nodes" | "edges" | "complete";
  snapshot: string | null;
  loadedNodes: number;
  loadedEdges: number;
  totalNodes: number;
  totalEdges: number;
}

interface NodePage {
  branch: string;
  snapshot: string | null;
  offset: number;
  count: number;
  next_offset: number | null;
  nodes: RawNode[];
}

interface EdgePage {
  branch: string;
  snapshot: string | null;
  offset: number;
  count: number;
  next_offset: number | null;
  edges: RawEdge[];
}

async function fetchJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`${url} returned ${response.status}`);
  const value = (await response.json()) as T & { error?: string };
  if (value.error) throw new Error(value.error);
  return value;
}

function assertSnapshot(expected: string | null, actual: string | null) {
  if (expected !== actual) {
    throw new Error(
      "The graph changed while it was loading. Refreshing it from a new snapshot is required.",
    );
  }
}

export async function fetchGraphManifest(
  branch: string,
  signal?: AbortSignal,
): Promise<GraphManifest> {
  return fetchJson(`/api/graph/manifest?${new URLSearchParams({ branch })}`, signal);
}

/** Progressively load a complete branch graph in deterministic bounded pages. */
export async function loadGraphData(
  branch: string,
  onProgress: (data: GraphData, progress: GraphLoadProgress) => void,
  signal?: AbortSignal,
): Promise<GraphData> {
  const manifest = await fetchGraphManifest(branch, signal);
  const chunk = Math.min(manifest.recommended_chunk, manifest.max_chunk);
  const nodes: RawNode[] = [];
  const edges: RawEdge[] = [];

  for (let offset = 0; offset < manifest.total_nodes; offset += chunk) {
    const page = await fetchJson<NodePage>(
      `/api/graph/nodes?${new URLSearchParams({ branch, offset: String(offset), limit: String(chunk) })}`,
      signal,
    );
    assertSnapshot(manifest.snapshot, page.snapshot);
    nodes.push(...page.nodes);
    onProgress(
      { nodes: nodes.slice(), edges },
      {
        stage: "nodes",
        snapshot: manifest.snapshot,
        loadedNodes: nodes.length,
        loadedEdges: 0,
        totalNodes: manifest.total_nodes,
        totalEdges: manifest.total_edges,
      },
    );
  }

  for (let offset = 0; offset < manifest.total_edges; offset += chunk) {
    const page = await fetchJson<EdgePage>(
      `/api/graph/edges?${new URLSearchParams({ branch, offset: String(offset), limit: String(chunk) })}`,
      signal,
    );
    assertSnapshot(manifest.snapshot, page.snapshot);
    edges.push(...page.edges);
    onProgress(
      { nodes, edges: edges.slice() },
      {
        stage: "edges",
        snapshot: manifest.snapshot,
        loadedNodes: nodes.length,
        loadedEdges: edges.length,
        totalNodes: manifest.total_nodes,
        totalEdges: manifest.total_edges,
      },
    );
  }

  const data = { nodes, edges };
  onProgress(data, {
    stage: "complete",
    snapshot: manifest.snapshot,
    loadedNodes: nodes.length,
    loadedEdges: edges.length,
    totalNodes: manifest.total_nodes,
    totalEdges: manifest.total_edges,
  });
  return data;
}

export interface DeepCallersHop {
  hop: number;
  nodes: RawNode[];
}

export interface DeepCallersResult {
  id: string;
  depth: number;
  risk_level: string;
  truncated: boolean;
  hops: DeepCallersHop[];
}

export async function fetchDeepCallers(
  id: string,
  branch: string,
  depth = 3,
): Promise<DeepCallersResult> {
  const params = new URLSearchParams({ branch, depth: String(depth) });
  return fetchJson(`/api/callers-by-id/${encodeURIComponent(id)}?${params}`);
}

export interface BranchListResult {
  active: string;
  branches: string[];
  last_sha: string | null;
}

export interface UnusedResult {
  count: number;
  nodes: RawNode[];
}

export async function fetchUnused(branch: string, kind?: string): Promise<UnusedResult> {
  const params = new URLSearchParams({ branch });
  if (kind) params.set("kind", kind);
  const url = `/api/unused${params.toString() ? "?" + params.toString() : ""}`;
  const r = await fetch(url);
  if (!r.ok) throw new Error(`/api/unused returned ${r.status}`);
  return r.json();
}

export async function fetchBranches(): Promise<BranchListResult> {
  const r = await fetch("/api/branches");
  if (!r.ok) throw new Error(`/api/branches returned ${r.status}`);
  return r.json();
}

export interface BranchDiffResult {
  base: string;
  head: string;
  added_nodes: RawNode[];
  removed_node_ids: string[];
  added_edges: RawEdge[];
  removed_edges: Pick<RawEdge, "src" | "dst" | "kind">[];
}

export async function fetchBranchDiff(base: string, head: string): Promise<BranchDiffResult> {
  const params = new URLSearchParams({ base, head });
  const r = await fetch(`/api/branch-diff?${params.toString()}`);
  if (!r.ok) throw new Error(`/api/branch-diff returned ${r.status}`);
  return r.json();
}

export interface GodNodesResult {
  count: number;
  nodes: (RawNode & { in_degree: number })[];
  min_in_degree: number;
}

export async function fetchGodNodes(branch: string, minInDegree?: number): Promise<GodNodesResult> {
  const params = new URLSearchParams({ branch });
  if (minInDegree !== undefined) params.set("min_in_degree", String(minInDegree));
  const url = `/api/god_nodes${params.toString() ? "?" + params.toString() : ""}`;
  const r = await fetch(url);
  if (!r.ok) throw new Error(`/api/god_nodes returned ${r.status}`);
  return r.json();
}
