import { useEffect, useRef, useState } from "react";
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
  const workerRef = useRef<Worker | null>(null);

  // One worker for the component's lifetime — re-instantiating per filter
  // toggle re-parses the worker module on every change.
  useEffect(() => {
    const worker = new Worker(new URL("../graph/viewWorker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = (event: MessageEvent<ViewWorkerResponse>) => setResult(event.data);
    workerRef.current = worker;
    return () => {
      worker.terminate();
      workerRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!input) return;
    workerRef.current?.postMessage(input);
  }, [input]);

  return input ? result : null;
}
