import type { GraphData } from "../api";
import { computeComplexityScores } from "./compute";

export interface ComplexityWorkerRequest {
  data: GraphData;
}

export type ComplexityWorkerResponse = [string, number][];

self.onmessage = (event: MessageEvent<ComplexityWorkerRequest>) => {
  const scores = computeComplexityScores(event.data.data);
  const response: ComplexityWorkerResponse = [...scores];
  (self as unknown as Worker).postMessage(response);
};
