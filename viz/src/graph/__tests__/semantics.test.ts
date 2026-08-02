import { describe, expect, it } from "vitest";
import { graphClusterForFile, graphScaleProfile } from "../semantics";

describe("graphClusterForFile", () => {
  it("groups workspace crates by package", () => {
    expect(graphClusterForFile("crates/gitcortex-store/src/kuzu/mod.rs")).toBe(
      "crates/gitcortex-store",
    );
  });

  it("groups conventional source trees by their package root", () => {
    expect(graphClusterForFile("frontend/src/components/Graph.tsx")).toBe("frontend");
    expect(graphClusterForFile("README.md")).toBe("repository root");
  });
});

describe("graphScaleProfile", () => {
  it("keeps detailed rendering for focused graphs", () => {
    const profile = graphScaleProfile(500, 2_000);
    expect(profile.tier).toBe("focused");
    expect(profile.edgeBudget).toBe(2_000);
    expect(profile.showArrows).toBe(true);
  });

  it("bounds overview paint for large and massive graphs", () => {
    expect(graphScaleProfile(20_000, 150_000)).toMatchObject({
      tier: "large",
      edgeBudget: 18_000,
      showArrows: false,
    });
    expect(graphScaleProfile(60_000, 400_000)).toMatchObject({
      tier: "massive",
      edgeBudget: 10_000,
      dynamicLabelLimit: 3,
    });
  });
});
