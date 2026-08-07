import { describe, expect, it } from "vitest";
import { generateSyntheticGraph } from "../fixtures";

describe("generateSyntheticGraph", () => {
  it("produces exactly the requested node count", () => {
    const graph = generateSyntheticGraph({ nodeCount: 250, seed: 1 });
    expect(graph.nodes).toHaveLength(250);
  });

  it("is deterministic for a given seed", () => {
    const a = generateSyntheticGraph({ nodeCount: 500, seed: 42 });
    const b = generateSyntheticGraph({ nodeCount: 500, seed: 42 });
    expect(a).toEqual(b);
  });

  it("produces different graphs for different seeds", () => {
    const a = generateSyntheticGraph({ nodeCount: 500, seed: 1 });
    const b = generateSyntheticGraph({ nodeCount: 500, seed: 2 });
    expect(a).not.toEqual(b);
  });

  it("gives every node a unique id and all RawNode fields populated", () => {
    const graph = generateSyntheticGraph({ nodeCount: 200, seed: 7 });
    const ids = new Set(graph.nodes.map((n) => n.id));
    expect(ids.size).toBe(200);
    for (const node of graph.nodes) {
      expect(node.id).toBeTruthy();
      expect(node.name).toBeTruthy();
      expect(node.kind).toBeTruthy();
      expect(node.file).toBeTruthy();
      expect(node.qualified_name).toBeTruthy();
      expect(typeof node.start_line).toBe("number");
      expect(typeof node.end_line).toBe("number");
      expect(node.end_line).toBeGreaterThanOrEqual(node.start_line);
      expect(typeof node.loc).toBe("number");
      expect(typeof node.is_async).toBe("boolean");
      expect(typeof node.is_unsafe).toBe("boolean");
    }
  });

  it("every edge references node ids that exist in the graph", () => {
    const graph = generateSyntheticGraph({ nodeCount: 300, seed: 3, edgeFactor: 4 });
    const ids = new Set(graph.nodes.map((n) => n.id));
    expect(graph.edges.length).toBeGreaterThan(0);
    for (const edge of graph.edges) {
      expect(ids.has(edge.src)).toBe(true);
      expect(ids.has(edge.dst)).toBe(true);
    }
  });

  it("scales edge count roughly with edgeFactor", () => {
    const graph = generateSyntheticGraph({ nodeCount: 1000, seed: 9, edgeFactor: 3 });
    expect(graph.edges.length).toBeGreaterThan(2000);
    expect(graph.edges.length).toBeLessThan(4000);
  });
});
