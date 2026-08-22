import type { GraphData } from "../api";
import { computeView, type ViewInput } from "./view";

export type ViewWorkerRequest = ViewInput;
export type ViewWorkerResponse = GraphData;

self.onmessage = (event: MessageEvent<ViewWorkerRequest>) => {
  const result = computeView(event.data);
  (self as unknown as Worker).postMessage(result);
};
