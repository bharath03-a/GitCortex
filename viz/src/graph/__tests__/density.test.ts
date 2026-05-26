import { describe, expect, it } from "vitest";
import { applyDensity } from "../density";
import type { GraphData, RawEdge, RawNode } from "../../api";

const node = (id: string, kind: string, visibility = "pub"): RawNode => ({
  id,
  name: id,
  kind,
  file: `${id}.rs`,
  start_line: 1,
  end_line: 10,
  qualified_name: id,
  loc: 5,
  visibility,
  is_async: false,
  is_unsafe: false,
});

const edge = (src: string, dst: string, kind: string): RawEdge => ({ src, dst, kind });

const buildGraph = (): GraphData => ({
  nodes: [
    node("file1", "file"),
    node("foo", "function", "pub"),
    node("bar", "function", "private"),
    node("baz", "function", "pub"),
    node("orphan", "struct", "pub"),
  ],
  edges: [
    edge("file1", "foo", "contains"),
    edge("file1", "bar", "contains"),
    edge("foo", "bar", "calls"),
    edge("bar", "baz", "calls"),
  ],
});

describe("applyDensity", () => {
  it("returns the full graph unchanged in `full` mode", () => {
    const g = buildGraph();
    expect(applyDensity(g, "full")).toEqual(g);
  });

  it("keeps only pub symbols in `public` mode", () => {
    const out = applyDensity(buildGraph(), "public");
    const names = out.nodes.map((n) => n.name).sort();
    expect(names).toEqual(["baz", "file1", "foo", "orphan"]);
  });

  it("drops File/Folder/Module + orphan semantic nodes in `focused` mode", () => {
    const out = applyDensity(buildGraph(), "focused");
    const names = out.nodes.map((n) => n.name).sort();
    expect(names).toEqual(["bar", "baz", "foo"]);
  });

  it("filters edges to surviving endpoints", () => {
    const out = applyDensity(buildGraph(), "focused");
    expect(out.edges.every((e) => e.kind !== "contains")).toBe(true);
    for (const e of out.edges) {
      const ids = new Set(out.nodes.map((n) => n.id));
      expect(ids.has(e.src)).toBe(true);
      expect(ids.has(e.dst)).toBe(true);
    }
  });
});
