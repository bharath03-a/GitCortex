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

export interface DeepCallersHop {
  hop: number;
  nodes: RawNode[];
}

export interface DeepCallersResult {
  name: string;
  depth: number;
  risk_level: string;
  hops: DeepCallersHop[];
}

export async function fetchDeepCallers(
  name: string,
  depth = 3,
): Promise<DeepCallersResult> {
  const r = await fetch(
    `/api/callers/${encodeURIComponent(name)}?depth=${depth}`,
  );
  if (!r.ok) throw new Error(`/api/callers returned ${r.status}`);
  return r.json();
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

export async function fetchUnused(kind?: string): Promise<UnusedResult> {
  const params = new URLSearchParams();
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
}

export async function fetchBranchDiff(
  base: string,
  head: string,
): Promise<BranchDiffResult> {
  const params = new URLSearchParams({ base, head });
  const r = await fetch(`/api/branch-diff?${params.toString()}`);
  if (!r.ok) throw new Error(`/api/branch-diff returned ${r.status}`);
  return r.json();
}
