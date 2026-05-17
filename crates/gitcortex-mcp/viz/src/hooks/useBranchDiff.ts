import { useEffect, useState } from "react";
import { fetchBranchDiff } from "../api";

export interface DiffOverlay {
  base: string;
  head: string;
  addedIds: Set<string>;
  removedIds: Set<string>;
}

export function useBranchDiff(
  base: string | null,
  head: string | null,
): DiffOverlay | null {
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
          addedIds: new Set(d.added_nodes.map((n) => n.id)),
          removedIds: new Set(d.removed_node_ids),
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
