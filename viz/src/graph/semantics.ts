export type GraphScaleTier = "focused" | "repository" | "large" | "massive";

export interface GraphScaleProfile {
  tier: GraphScaleTier;
  label: string;
  edgeBudget: number;
  topLabelLimit: number;
  dynamicLabelLimit: number;
  showArrows: boolean;
  useCurves: boolean;
}

/** Directory/package grouping used for the architecture-level Atlas layout. */
export function graphClusterForFile(file: string): string {
  const parts = file.replaceAll("\\", "/").split("/").filter(Boolean);
  const sourceIndex = parts.findIndex((part) => part === "src");
  if (sourceIndex > 0) {
    const packageRoot = parts[sourceIndex - 1];
    if (["crates", "packages", "apps"].includes(parts[sourceIndex - 2] ?? "")) {
      return `${parts[sourceIndex - 2]}/${packageRoot}`;
    }
    return packageRoot;
  }
  if (["crates", "packages", "apps"].includes(parts[0] ?? "") && parts[1]) {
    return `${parts[0]}/${parts[1]}`;
  }
  if (parts.length > 1) return parts[0];
  return "repository root";
}

/**
 * Keep every node addressable while bounding overview edge paint and label work.
 * Exact neighborhoods replace the sampled overview as soon as a node is selected.
 */
export function graphScaleProfile(nodeCount: number, edgeCount: number): GraphScaleProfile {
  if (nodeCount <= 1_000) {
    return {
      tier: "focused",
      label: "Detailed",
      edgeBudget: edgeCount,
      topLabelLimit: 10,
      dynamicLabelLimit: 8,
      showArrows: true,
      useCurves: true,
    };
  }
  if (nodeCount <= 10_000) {
    return {
      tier: "repository",
      label: "Repository",
      edgeBudget: Math.min(edgeCount, 30_000),
      topLabelLimit: 7,
      dynamicLabelLimit: 6,
      showArrows: true,
      useCurves: edgeCount <= 30_000,
    };
  }
  if (nodeCount <= 40_000) {
    return {
      tier: "large",
      label: "Large graph",
      edgeBudget: Math.min(edgeCount, 18_000),
      topLabelLimit: 4,
      dynamicLabelLimit: 4,
      showArrows: false,
      useCurves: false,
    };
  }
  return {
    tier: "massive",
    label: "Massive graph",
    edgeBudget: Math.min(edgeCount, 10_000),
    topLabelLimit: 2,
    dynamicLabelLimit: 3,
    showArrows: false,
    useCurves: false,
  };
}
