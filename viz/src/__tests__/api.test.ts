import { afterEach, describe, expect, it, vi } from "vitest";
import { loadGraphData, type RawNode } from "../api";

const node = (id: string): RawNode => ({
  id,
  name: id,
  kind: "function",
  file: `src/${id}.rs`,
  start_line: 1,
  end_line: 2,
  qualified_name: `crate::${id}`,
  loc: 2,
  visibility: "pub",
  is_async: false,
  is_unsafe: false,
});

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

afterEach(() => vi.unstubAllGlobals());

describe("loadGraphData", () => {
  it("loads deterministic node and edge pages and reports progress", async () => {
    const responses = [
      {
        branch: "main",
        snapshot: "abc",
        total_nodes: 2,
        total_edges: 1,
        nodes_by_kind: [["function", 2]],
        edges_by_kind: [["calls", 1]],
        recommended_chunk: 1,
        max_chunk: 10,
      },
      { branch: "main", snapshot: "abc", offset: 0, count: 1, next_offset: 1, nodes: [node("a")] },
      { branch: "main", snapshot: "abc", offset: 1, count: 1, next_offset: 2, nodes: [node("b")] },
      {
        branch: "main",
        snapshot: "abc",
        offset: 0,
        count: 1,
        next_offset: 1,
        edges: [{ src: "a", dst: "b", kind: "calls", confidence: "extracted" }],
      },
    ];
    const fetchMock = vi.fn(() => Promise.resolve(jsonResponse(responses.shift())));
    vi.stubGlobal("fetch", fetchMock);
    const progress: string[] = [];

    const graph = await loadGraphData("main", (_partial, update) => {
      progress.push(`${update.stage}:${update.loadedNodes}:${update.loadedEdges}`);
    });

    expect(graph.nodes.map((item) => item.id)).toEqual(["a", "b"]);
    expect(graph.edges).toHaveLength(1);
    expect(progress).toEqual(["nodes:1:0", "nodes:2:0", "edges:2:1", "complete:2:1"]);
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("rejects pages from a different graph snapshot", async () => {
    const responses = [
      {
        branch: "main",
        snapshot: "before",
        total_nodes: 1,
        total_edges: 0,
        nodes_by_kind: [],
        edges_by_kind: [],
        recommended_chunk: 10,
        max_chunk: 10,
      },
      {
        branch: "main",
        snapshot: "after",
        offset: 0,
        count: 1,
        next_offset: null,
        nodes: [node("a")],
      },
    ];
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve(jsonResponse(responses.shift()))),
    );

    await expect(loadGraphData("main", () => {})).rejects.toThrow("graph changed");
  });
});
