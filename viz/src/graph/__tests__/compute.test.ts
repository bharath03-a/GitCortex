import { describe, expect, it } from "vitest";
import { computeComplexityScores } from "../compute";
import type { GraphData, RawEdge, RawNode } from "../../api";

const node = (id: string, loc: number): RawNode => ({
  id,
  name: id,
  kind: "function",
  file: `${id}.rs`,
  start_line: 1,
  end_line: 10,
  qualified_name: id,
  loc,
  visibility: "pub",
  is_async: false,
  is_unsafe: false,
});

const edge = (src: string, dst: string): RawEdge => ({ src, dst, kind: "calls" });

describe("computeComplexityScores", () => {
  it("returns a score for every node", () => {
    const data: GraphData = {
      nodes: [node("a", 10), node("b", 20), node("c", 5)],
      edges: [edge("a", "b"), edge("b", "c")],
    };
    const scores = computeComplexityScores(data);
    expect(scores.size).toBe(3);
    expect(scores.has("a")).toBe(true);
    expect(scores.has("b")).toBe(true);
    expect(scores.has("c")).toBe(true);
  });

  it("scores every value between 0 and 1", () => {
    const data: GraphData = {
      nodes: [node("a", 10), node("b", 400), node("c", 1)],
      edges: [edge("a", "b"), edge("b", "c"), edge("c", "a")],
    };
    const scores = computeComplexityScores(data);
    for (const score of scores.values()) {
      expect(score).toBeGreaterThanOrEqual(0);
      expect(score).toBeLessThanOrEqual(1);
    }
  });

  it("gives a higher-degree node a higher coupling contribution than an isolated node of equal size", () => {
    const data: GraphData = {
      nodes: [node("hub", 10), node("a", 10), node("b", 10), node("isolated", 10)],
      edges: [edge("hub", "a"), edge("hub", "b"), edge("a", "b")],
    };
    const scores = computeComplexityScores(data);
    expect(scores.get("hub")!).toBeGreaterThan(scores.get("isolated")!);
  });

  it("gives a node with more lines of code a higher score than an equally-connected smaller node", () => {
    const data: GraphData = {
      nodes: [node("big", 1000), node("small", 1), node("other", 10)],
      edges: [edge("big", "other"), edge("small", "other")],
    };
    const scores = computeComplexityScores(data);
    expect(scores.get("big")!).toBeGreaterThan(scores.get("small")!);
  });

  it("returns an empty map for an empty graph", () => {
    const scores = computeComplexityScores({ nodes: [], edges: [] });
    expect(scores.size).toBe(0);
  });
});
