import { describe, expect, it } from "vitest";
import type { GraphData, RawNode } from "../../api";
import { summarizeGroups } from "../../graph/compatibility";

function node(id: string, file: string, loc: number): RawNode {
  return {
    id,
    name: id,
    kind: "function",
    file,
    start_line: 1,
    end_line: loc,
    qualified_name: id,
    loc,
    visibility: "public",
    is_async: false,
    is_unsafe: false,
  };
}

describe("summarizeGroups", () => {
  it("builds package summaries and counts cross-boundary relations", () => {
    const data: GraphData = {
      nodes: [
        node("a", "crates/gitcortex-core/src/a.rs", 10),
        node("b", "crates/gitcortex-core/src/b.rs", 30),
        node("c", "crates/gitcortex-store/src/c.rs", 20),
      ],
      edges: [
        { src: "a", dst: "b", kind: "calls" },
        { src: "b", dst: "c", kind: "uses" },
      ],
    };

    const groups = summarizeGroups(data);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toMatchObject({
      name: "crates/gitcortex-core",
      relations: 2,
      boundaryRelations: 1,
      loc: 40,
    });
    expect(groups[0].nodes.map((item) => item.id)).toEqual(["b", "a"]);
    expect(groups[1]).toMatchObject({
      name: "crates/gitcortex-store",
      relations: 1,
      boundaryRelations: 1,
      loc: 20,
    });
  });
});
