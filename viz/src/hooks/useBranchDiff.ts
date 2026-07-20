import { useEffect, useState } from "react";
import type { RawEdge, RawNode } from "../api";
import { fetchBranchDiff } from "../api";

export function diffEdgeKey(edge: Pick<RawEdge, "src" | "dst" | "kind">): string {
  return `${edge.src}\u0000${edge.dst}\u0000${edge.kind}`;
}

export interface DiffOverlay {
  base: string;
  head: string;
  addedIds: Set<string>;
  removedIds: Set<string>;
  addedNodes: RawNode[];
  addedEdges: RawEdge[];
  addedEdgeKeys: Set<string>;
  removedEdgeKeys: Set<string>;
}

export function useBranchDiff(base: string | null, head: string | null): DiffOverlay | null {
  const [overlay, setOverlay] = useState<DiffOverlay | null>(null);

  useEffect(() => {
    if (!base || !head) {
      setOverlay(null);
      return;
    }
    let cancelled = false;
    fetchBranchDiff(base, head)
      .then((d) => {
        if (cancelled) return;
        setOverlay({
          base: d.base,
          head: d.head,
          addedIds: new Set(d.added_nodes.map((node) => node.id)),
          removedIds: new Set(d.removed_node_ids),
          addedNodes: d.added_nodes,
          addedEdges: d.added_edges,
          addedEdgeKeys: new Set(d.added_edges.map(diffEdgeKey)),
          removedEdgeKeys: new Set(d.removed_edges.map(diffEdgeKey)),
        });
      })
      .catch(() => {
        if (!cancelled) setOverlay(null);
      });
    return () => {
      cancelled = true;
    };
  }, [base, head]);

  return overlay;
}
