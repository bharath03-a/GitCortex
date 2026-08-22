import { describe, expect, it } from "vitest";
import { computeView, type ViewInput } from "../view";
import type { GraphData, RawEdge, RawNode } from "../../api";

const node = (id: string, kind: string, opts: Partial<RawNode> = {}): RawNode => ({
  id,
  name: id,
  kind,
  file: `${id}.rs`,
  start_line: 1,
  end_line: 10,
  qualified_name: id,
  loc: 5,
  visibility: "pub",
  is_async: false,
  is_unsafe: false,
  ...opts,
});

const edge = (src: string, dst: string, kind: string, opts: Partial<RawEdge> = {}): RawEdge => ({
  src,
  dst,
  kind,
  ...opts,
});

const baseInput = (overrides: Partial<ViewInput> = {}): ViewInput => ({
  rawData: { nodes: [], edges: [] },
  viewMode: "atlas",
  focusedData: null,
  selectedFallback: null,
  diffOverlay: null,
  density: "all",
  hiddenKinds: new Set(),
  hiddenVisibility: new Set(),
  flagFilter: new Set(),
  hiddenEdgeKinds: new Set(),
  hiddenConfidence: new Set(),
  ...overrides,
});

describe("computeView", () => {
  it("returns the raw graph unfiltered in atlas mode with no filters", () => {
    const rawData: GraphData = {
      nodes: [node("a", "function"), node("b", "function")],
      edges: [edge("a", "b", "calls")],
    };
    const out = computeView(baseInput({ rawData }));
    expect(out.nodes.map((n) => n.id).sort()).toEqual(["a", "b"]);
    expect(out.edges).toHaveLength(1);
  });

  it("uses focusedData in investigate mode instead of rawData", () => {
    const rawData: GraphData = { nodes: [node("a", "function")], edges: [] };
    const focusedData: GraphData = { nodes: [node("b", "function")], edges: [] };
    const out = computeView(baseInput({ rawData, viewMode: "investigate", focusedData }));
    expect(out.nodes.map((n) => n.id)).toEqual(["b"]);
  });

  it("falls back to the selected node alone when investigate has no focused data yet", () => {
    const out = computeView(
      baseInput({
        rawData: { nodes: [], edges: [] },
        viewMode: "investigate",
        focusedData: null,
        selectedFallback: node("solo", "function"),
      }),
    );
    expect(out.nodes.map((n) => n.id)).toEqual(["solo"]);
  });

  it("merges diff-overlay added nodes and edges in atlas mode without duplicating", () => {
    const rawData: GraphData = {
      nodes: [node("a", "function")],
      edges: [edge("a", "a", "calls")],
    };
    const out = computeView(
      baseInput({
        rawData,
        diffOverlay: {
          addedNodes: [node("b", "function")],
          addedEdges: [edge("a", "b", "calls"), edge("a", "a", "calls")],
        },
      }),
    );
    expect(out.nodes.map((n) => n.id).sort()).toEqual(["a", "b"]);
    expect(out.edges).toHaveLength(2);
  });

  it("does not apply diff overlay in investigate mode", () => {
    const rawData: GraphData = { nodes: [node("a", "function")], edges: [] };
    const out = computeView(
      baseInput({
        rawData,
        viewMode: "investigate",
        focusedData: { nodes: [], edges: [] },
        diffOverlay: { addedNodes: [node("b", "function")], addedEdges: [] },
      }),
    );
    expect(out.nodes).toHaveLength(0);
  });

  it("excludes hidden kinds and hidden visibility", () => {
    const rawData: GraphData = {
      nodes: [
        node("a", "function", { visibility: "pub" }),
        node("b", "struct", { visibility: "private" }),
      ],
      edges: [],
    };
    const out = computeView(
      baseInput({ rawData, hiddenKinds: new Set(["struct"]), hiddenVisibility: new Set() }),
    );
    expect(out.nodes.map((n) => n.id)).toEqual(["a"]);
  });

  it("applies the async/unsafe flag filter", () => {
    const rawData: GraphData = {
      nodes: [
        node("a", "function", { is_async: true }),
        node("b", "function", { is_async: false }),
      ],
      edges: [],
    };
    const out = computeView(baseInput({ rawData, flagFilter: new Set(["async"]) }));
    expect(out.nodes.map((n) => n.id)).toEqual(["a"]);
  });

  it("drops edges pointing at filtered-out nodes and hidden edge kinds/confidence", () => {
    const rawData: GraphData = {
      nodes: [node("a", "function"), node("b", "function"), node("c", "struct")],
      edges: [
        edge("a", "b", "calls", { confidence: "inferred" }),
        edge("a", "b", "imports"),
        edge("a", "c", "calls"),
      ],
    };
    const out = computeView(
      baseInput({
        rawData,
        hiddenKinds: new Set(["struct"]),
        hiddenEdgeKinds: new Set(["imports"]),
        hiddenConfidence: new Set(["inferred"]),
      }),
    );
    expect(out.edges).toHaveLength(0);
  });
});
