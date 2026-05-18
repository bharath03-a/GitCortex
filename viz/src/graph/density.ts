import type { GraphData, RawEdge, RawNode } from "../api";

export type DensityMode = "focused" | "public" | "full";

export const DENSITY_LABEL: Record<DensityMode, string> = {
  focused: "Focused",
  public: "Public API",
  full: "Full graph",
};

const SEMANTIC_EDGE_KINDS = new Set(["calls", "implements", "inherits", "uses", "throws"]);

const STRUCTURAL_KINDS = new Set(["folder", "file", "module"]);

export function applyDensity(data: GraphData, mode: DensityMode): GraphData {
  if (mode === "full") return data;

  if (mode === "public") {
    const keep = new Set(data.nodes.filter((n) => n.visibility === "pub").map((n) => n.id));
    return filterByIds(data, keep);
  }

  const semDegree = new Map<string, number>();
  for (const e of data.edges) {
    if (!SEMANTIC_EDGE_KINDS.has(e.kind)) continue;
    semDegree.set(e.src, (semDegree.get(e.src) ?? 0) + 1);
    semDegree.set(e.dst, (semDegree.get(e.dst) ?? 0) + 1);
  }
  const keep = new Set<string>();
  for (const n of data.nodes) {
    if (STRUCTURAL_KINDS.has(n.kind)) continue;
    if ((semDegree.get(n.id) ?? 0) >= 1) keep.add(n.id);
  }
  return filterByIds(data, keep);
}

function filterByIds(data: GraphData, keep: Set<string>): GraphData {
  const nodes: RawNode[] = data.nodes.filter((n) => keep.has(n.id));
  const edges: RawEdge[] = data.edges.filter((e) => keep.has(e.src) && keep.has(e.dst));
  return { nodes, edges };
}
