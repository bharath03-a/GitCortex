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
  it("returns the full graph unchanged in `all` mode", () => {
    const g = buildGraph();
    expect(applyDensity(g, "all")).toEqual(g);
  });

  it("keeps only exposed symbols in `api` mode", () => {
    const out = applyDensity(buildGraph(), "api");
    const names = out.nodes.map((n) => n.name).sort();
    expect(names).toEqual(["baz", "file1", "foo", "orphan"]);
  });

  it("drops structural and orphan nodes in `connected` mode", () => {
    const out = applyDensity(buildGraph(), "connected");
    const names = out.nodes.map((n) => n.name).sort();
    expect(names).toEqual(["bar", "baz", "foo"]);
  });

  it("filters edges to surviving endpoints", () => {
    const out = applyDensity(buildGraph(), "connected");
    expect(out.edges.every((e) => e.kind !== "contains")).toBe(true);
    for (const e of out.edges) {
      const ids = new Set(out.nodes.map((n) => n.id));
      expect(ids.has(e.src)).toBe(true);
      expect(ids.has(e.dst)).toBe(true);
    }
  });
});
