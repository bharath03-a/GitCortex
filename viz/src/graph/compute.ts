import type { GraphData } from "../api";

/**
 * Complexity risk per node: 55% log-scaled LOC, 45% log-scaled call/use
 * degree. Pure and side-effect-free so it can run on the main thread or
 * inside a Web Worker unchanged.
 */
export function computeComplexityScores(data: GraphData): Map<string, number> {
  const degree = new Map<string, number>();
  for (const edge of data.edges) {
    degree.set(edge.src, (degree.get(edge.src) ?? 0) + 1);
    degree.set(edge.dst, (degree.get(edge.dst) ?? 0) + 1);
  }

  let maxDegree = 1;
  for (const value of degree.values()) maxDegree = Math.max(maxDegree, value);
  let maxLoc = 1;
  for (const node of data.nodes) maxLoc = Math.max(maxLoc, node.loc);
  const locScale = Math.log2(maxLoc + 1);
  const degreeScale = Math.log2(maxDegree + 1);

  return new Map(
    data.nodes.map((node) => {
      const locRisk = Math.log2(node.loc + 1) / locScale;
      const couplingRisk = Math.log2((degree.get(node.id) ?? 0) + 1) / degreeScale;
      return [node.id, locRisk * 0.55 + couplingRisk * 0.45];
    }),
  );
}
