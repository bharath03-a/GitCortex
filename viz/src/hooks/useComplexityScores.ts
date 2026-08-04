import { useEffect, useState } from "react";
import type { GraphData } from "../api";
import type { ComplexityWorkerResponse } from "../graph/complexityWorker";

/**
 * Runs computeComplexityScores off the main thread. Degree-map construction
 * and the LOC/coupling pass are O(nodes + edges); at repository scale that
 * is real main-thread work, so it moves to a Web Worker instead of a
 * useMemo.
 */
export function useComplexityScores(
  data: GraphData | null,
  enabled: boolean,
): Map<string, number> | null {
  const [scores, setScores] = useState<Map<string, number> | null>(null);

  useEffect(() => {
    if (!enabled || !data) return;
    const worker = new Worker(new URL("../graph/complexityWorker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = (event: MessageEvent<ComplexityWorkerResponse>) => {
      setScores(new Map(event.data));
    };
    worker.postMessage({ data });
    return () => worker.terminate();
  }, [data, enabled]);

  return enabled && data ? scores : null;
}
