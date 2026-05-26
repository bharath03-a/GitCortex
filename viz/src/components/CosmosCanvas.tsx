import { useEffect, useMemo, useRef } from "react";
import { Cosmograph, CosmographProvider } from "@cosmograph/react";
import type { CosmographRef } from "@cosmograph/react";
import type { GraphData, RawNode } from "../api";
import { EDGE_COLOR, EDGE_WIDTH, KIND_COLOR, dimColor } from "../theme/colors";
import { CanvasControls } from "./CanvasControls";
import type { DiffOverlay } from "../hooks/useBranchDiff";

const DIFF_ADDED = "#10b981";
const DIFF_REMOVED = "#ef4444";

interface PointRow extends Record<string, unknown> {
  id: string;
  index: number;
  kind: string;
  name: string;
  loc: number;
}

interface LinkRow extends Record<string, unknown> {
  source: string;
  target: string;
  sourceIndex: number;
  targetIndex: number;
  kind: string;
}

interface Props {
  data: GraphData;
  hiddenKinds: Set<string>;
  hiddenEdgeKinds: Set<string>;
  selected: RawNode | null;
  onSelect: (n: RawNode | null) => void;
  depth: number;
  diffOverlay: DiffOverlay | null;
  unusedIds: Set<string> | null;
}

export function CosmosCanvas({
  data,
  hiddenKinds,
  hiddenEdgeKinds,
  selected,
  onSelect,
  depth,
  diffOverlay,
  unusedIds,
}: Props) {
  const ref = useRef<CosmographRef>(null);

  const { points, links, nodeIndexById, indexToNode, neighbors } = useMemo(() => {
    const points: PointRow[] = [];
    const nodeIndexById = new Map<string, number>();
    const indexToNode = new Map<number, RawNode>();
    data.nodes.forEach((n, i) => {
      nodeIndexById.set(n.id, i);
      indexToNode.set(i, n);
      points.push({
        id: n.id,
        index: i,
        kind: n.kind,
        name: n.name,
        loc: n.loc,
      });
    });

    const links: LinkRow[] = [];
    const neighbors = new Map<string, Set<string>>();
    for (const e of data.edges) {
      const si = nodeIndexById.get(e.src);
      const ti = nodeIndexById.get(e.dst);
      if (si == null || ti == null) continue;
      links.push({
        source: e.src,
        target: e.dst,
        sourceIndex: si,
        targetIndex: ti,
        kind: e.kind,
      });
      if (!neighbors.has(e.src)) neighbors.set(e.src, new Set());
      if (!neighbors.has(e.dst)) neighbors.set(e.dst, new Set());
      neighbors.get(e.src)!.add(e.dst);
      neighbors.get(e.dst)!.add(e.src);
    }
    return { points, links, nodeIndexById, indexToNode, neighbors };
  }, [data]);

  const highlightSet = useMemo(() => {
    if (!selected) return null;
    const visited = new Set<string>([selected.id]);
    let frontier: string[] = [selected.id];
    for (let hop = 0; hop < Math.max(1, depth); hop++) {
      const next: string[] = [];
      for (const id of frontier) {
        const ns = neighbors.get(id);
        if (!ns) continue;
        for (const nb of ns) {
          if (!visited.has(nb)) {
            visited.add(nb);
            next.push(nb);
          }
        }
      }
      if (next.length === 0) break;
      frontier = next;
    }
    return visited;
  }, [selected, neighbors, depth]);

  useEffect(() => {
    if (!selected || !ref.current) return;
    const idx = nodeIndexById.get(selected.id);
    if (idx == null) return;
    ref.current.setFocusedPoint(idx);
    ref.current.zoomToPoint(idx, 600, 5, true);
  }, [selected, nodeIndexById]);

  return (
    <CosmographProvider>
      <Cosmograph
        ref={ref}
        points={points}
        links={links}
        pointIdBy="id"
        pointIndexBy="index"
        pointColorBy="kind"
        pointColorByFn={(value: unknown, index?: number): string => {
          const kind = String(value);
          if (hiddenKinds.has(kind)) return "rgba(0,0,0,0)";
          const node = index != null ? indexToNode.get(index) : undefined;
          if (diffOverlay && node) {
            if (diffOverlay.addedIds.has(node.id)) return DIFF_ADDED;
            if (diffOverlay.removedIds.has(node.id)) return DIFF_REMOVED;
          }
          if (unusedIds && node) {
            if (unusedIds.has(node.id)) return "#f59e0b";
            return dimColor(KIND_COLOR[kind] ?? "#89b4fa", 0.82);
          }
          return KIND_COLOR[kind] ?? "#89b4fa";
        }}
        pointSizeBy="loc"
        pointSizeByFn={(value: unknown, index?: number): number => {
          const node = index != null ? indexToNode.get(index) : undefined;
          const loc = typeof value === "number" ? value : 0;
          const base = Math.sqrt(loc + 1) * 1.4 + 2.5;
          if (!node) return base;
          if (selected?.id === node.id) return base * 1.8;
          if (highlightSet?.has(node.id)) return base * 1.3;
          return base;
        }}
        pointLabelBy="name"
        pointLabelColor="#e6e6f0"
        pointLabelFontSize={11}
        linkSourceBy="source"
        linkTargetBy="target"
        linkSourceIndexBy="sourceIndex"
        linkTargetIndexBy="targetIndex"
        linkColorBy="kind"
        linkColorByFn={(value: unknown, index?: number): string => {
          const kind = String(value);
          if (hiddenEdgeKinds.has(kind)) return "rgba(0,0,0,0)";
          const base = EDGE_COLOR[kind] ?? "#666";
          if (!highlightSet) return base;
          const link = index != null ? links[index] : undefined;
          if (!link) return base;
          const lit =
            highlightSet.has(String(link.source)) && highlightSet.has(String(link.target));
          return lit ? base : dimColor(base, 0.82);
        }}
        linkWidthBy="kind"
        linkWidthByFn={(value: unknown): number => EDGE_WIDTH[String(value)] ?? 1}
        linkArrowsSizeScale={0.6}
        backgroundColor="rgba(0,0,0,0)"
        spaceSize={4096}
        simulationGravity={0.15}
        simulationRepulsion={1.3}
        simulationFriction={0.85}
        simulationDecay={1000}
        simulationLinkSpring={1.1}
        simulationLinkDistance={4}
        showDynamicLabels
        showDynamicLabelsLimit={20}
        showTopLabels
        showTopLabelsLimit={40}
        showHoveredPointLabel
        hoveredPointRingColor="#a78bfa"
        focusedPointRingColor="#a78bfa"
        fitViewOnInit
        fitViewDelay={1500}
        selectPointOnClick={false}
        focusPointOnClick={false}
        onClick={(index) => {
          if (index == null) {
            onSelect(null);
            return;
          }
          const node = indexToNode.get(index);
          if (node) onSelect(node);
        }}
        onLabelClick={(index) => {
          const node = indexToNode.get(index);
          if (node) onSelect(node);
        }}
        style={{ height: "100%", width: "100%" }}
      />
      <CanvasControls cosmoRef={ref} />
    </CosmographProvider>
  );
}
