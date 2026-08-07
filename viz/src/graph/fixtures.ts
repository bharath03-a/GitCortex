import type { GraphData, RawEdge, RawNode } from "../api";

// Deterministic PRNG (mulberry32) so the same seed always produces the same
// graph — required for reproducible scale benchmarks across runs/machines.
function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const NODE_KINDS = [
  "function",
  "function",
  "function",
  "method",
  "method",
  "struct",
  "trait",
  "enum",
  "constant",
] as const;

const EDGE_KINDS = ["calls", "calls", "calls", "uses"] as const;
const CONFIDENCES = ["extracted", "extracted", "resolved", "inferred"] as const;

const NODES_PER_FILE = 18;
const FILES_PER_MODULE = 12;
const MODULES_PER_PACKAGE = 8;

function filePathFor(index: number): string {
  const fileIndex = Math.floor(index / NODES_PER_FILE);
  const moduleIndex = Math.floor(fileIndex / FILES_PER_MODULE);
  const packageIndex = Math.floor(moduleIndex / MODULES_PER_PACKAGE);
  const pkg = packageIndex % MODULES_PER_PACKAGE;
  const mod = moduleIndex % FILES_PER_MODULE;
  const file = fileIndex % (NODES_PER_FILE * 4);
  return `pkg${pkg}/mod${mod}/file${file}.rs`;
}

export interface GenerateGraphOptions {
  nodeCount: number;
  seed?: number;
  /** Approximate ratio of edges to nodes. */
  edgeFactor?: number;
}

export function generateSyntheticGraph({
  nodeCount,
  seed = 1,
  edgeFactor = 3,
}: GenerateGraphOptions): GraphData {
  const rand = mulberry32(seed);
  const nodes: RawNode[] = [];

  for (let i = 0; i < nodeCount; i++) {
    const kind = NODE_KINDS[Math.floor(rand() * NODE_KINDS.length)];
    const file = filePathFor(i);
    const startLine = 1 + Math.floor(rand() * 400);
    const loc = 3 + Math.floor(rand() * 60);
    const name = `${kind}_${i}`;
    const qualifiedName = `${file.replace(/\//g, "::").replace(".rs", "")}::${name}`;
    nodes.push({
      id: `n${i}`,
      name,
      kind,
      file,
      start_line: startLine,
      end_line: startLine + loc,
      qualified_name: qualifiedName,
      loc,
      visibility: rand() < 0.6 ? "pub" : "private",
      is_async: rand() < 0.1,
      is_unsafe: rand() < 0.02,
    });
  }

  const edgeCount = Math.round(nodeCount * edgeFactor);
  const edges: RawEdge[] = [];
  for (let i = 0; i < edgeCount; i++) {
    const srcIndex = Math.floor(rand() * nodeCount);
    // Bias destinations toward nearby indices to mimic module-local call
    // locality instead of a uniformly random (unrealistic) graph.
    const locality = Math.floor(rand() * Math.min(200, nodeCount));
    const direction = rand() < 0.5 ? 1 : -1;
    let dstIndex = (srcIndex + direction * locality) % nodeCount;
    if (dstIndex < 0) dstIndex += nodeCount;
    if (dstIndex === srcIndex) dstIndex = (dstIndex + 1) % nodeCount;

    edges.push({
      src: `n${srcIndex}`,
      dst: `n${dstIndex}`,
      kind: EDGE_KINDS[Math.floor(rand() * EDGE_KINDS.length)],
      line: nodes[srcIndex].start_line + Math.floor(rand() * nodes[srcIndex].loc),
      confidence: CONFIDENCES[Math.floor(rand() * CONFIDENCES.length)],
    });
  }

  return { nodes, edges };
}
