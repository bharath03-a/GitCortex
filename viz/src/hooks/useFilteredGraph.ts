import { useEffect, useState } from "react";
import type { GraphData } from "../api";
import type { ViewInput } from "../graph/view";
import type { ViewWorkerResponse } from "../graph/viewWorker";

/**
 * Runs computeView (view-mode selection, diff merge, density + filter
 * passes) off the main thread. Keeps the previously painted graph visible
 * while a new one is computed instead of flashing to null on every filter
 * toggle.
 */
export function useFilteredGraph(input: ViewInput | null): GraphData | null {
  const [result, setResult] = useState<GraphData | null>(null);

  useEffect(() => {
    if (!input) return;
    const worker = new Worker(new URL("../graph/viewWorker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = (event: MessageEvent<ViewWorkerResponse>) => setResult(event.data);
    worker.postMessage(input);
    return () => worker.terminate();
  }, [input]);

  return input ? result : null;
}
