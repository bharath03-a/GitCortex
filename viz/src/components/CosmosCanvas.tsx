import { useEffect, useMemo, useRef, useState } from "react";
import { Cosmograph, CosmographProvider } from "@cosmograph/react";
import type { CosmographRef } from "@cosmograph/react";
import type { GraphData, RawNode } from "../api";
import {
  EDGE_WIDTH,
  confidenceAlpha,
  dimColor,
  edgeColors,
  kindColors,
  type ProductTheme,
} from "../theme/colors";
import { CanvasControls } from "./CanvasControls";
import type { DiffOverlay } from "../hooks/useBranchDiff";
import { graphClusterForFile, graphScaleProfile } from "../graph/semantics";

const GRAPH_THEME = {
  light: {
    brand: "#C4633F",
    added: "#2F6F5E",
    removed: "#B84B42",
    impact: "#167A86",
    warn: "#8A6D28",
    boundary: "#6B4CA8",
    neutral: "#74747D",
    edgeNeutral: "#AAA79E",
    greyout: "#D7D5CE",
    label: "#55555F",
  },
  dark: {
    brand: "#E17B54",
    added: "#72B69F",
    removed: "#DF756C",
    impact: "#67C5D0",
    warn: "#D2AC5C",
    boundary: "#A98BDD",
    neutral: "#8E929C",
    edgeNeutral: "#777B84",
    greyout: "#343942",
    label: "#D8D5CE",
  },
} as const;

interface PointRow extends Record<string, unknown> {
  id: string;
  index: number;
  kind: string;
  name: string;
  loc: number;
  cluster: string;
  clusterStrength: number;
  shape: string;
  labelWeight: number;
}

interface LinkRow extends Record<string, unknown> {
  source: string;
  target: string;
  sourceIndex: number;
  targetIndex: number;
  kind: string;
  confidence: string;
  crossBoundary: boolean;
}

interface HoveredNode {
  node: RawNode;
  x: number;
  y: number;
  degree: number;
}

interface Props {
  data: GraphData;
  selected: RawNode | null;
  onSelect: (n: RawNode | null) => void;
  depth: number;
  diffOverlay: DiffOverlay | null;
  unusedIds: Set<string> | null;
  godNodeIds: Set<string> | null;
  hotspotScores: Map<string, number> | null;
  complexityScores: Map<string, number> | null;
  boundaryNodeIds: Set<string> | null;
  boundaryEdgeKeys: Set<string> | null;
  theme: ProductTheme;
  onShowOverview?: () => void;
}

const TYPE_KINDS = new Set(["struct", "enum", "trait", "interface", "typealias"]);
const STRUCTURAL_KINDS = new Set(["folder", "file", "module"]);

function shapeForKind(kind: string): string {
  if (kind === "folder" || kind === "module") return "hexagon";
  if (kind === "file" || kind === "section") return "square";
  if (kind === "enum") return "pentagon";
  if (TYPE_KINDS.has(kind)) return "diamond";
  if (kind === "macro" || kind === "annotation") return "star";
  if (kind === "constant" || kind === "property" || kind === "enummember") return "triangle";
  return "circle";
}

function edgeKey(link: Pick<LinkRow, "source" | "target" | "kind">): string {
  return `${link.source}\u0000${link.target}\u0000${link.kind}`;
}

function sampleLinks(links: LinkRow[], budget: number): LinkRow[] {
  if (links.length <= budget) return links;
  const result: LinkRow[] = [];
  const selected = new Set<LinkRow>();
  const boundaryCount = links.reduce((count, link) => count + Number(link.crossBoundary), 0);
  const boundaryBudget = Math.min(Math.floor(budget * 0.34), boundaryCount);
  const boundaryStride = Math.max(1, Math.ceil(boundaryCount / Math.max(boundaryBudget, 1)));
  let seenBoundaries = 0;
  for (const link of links) {
    if (!link.crossBoundary) continue;
    if (seenBoundaries % boundaryStride === 0 && result.length < boundaryBudget) {
      result.push(link);
      selected.add(link);
    }
    seenBoundaries += 1;
  }
  const remaining = budget - result.length;
  const stride = Math.max(1, Math.ceil((links.length - selected.size) / Math.max(remaining, 1)));
  let seen = 0;
  for (const link of links) {
    if (selected.has(link)) continue;
    if (seen % stride === 0 && result.length < budget) result.push(link);
    seen += 1;
    if (result.length >= budget) break;
  }
  return result;
}

function pointerFromEvent(event: unknown): { clientX: number; clientY: number } | null {
  if (event instanceof MouseEvent) return event;
  const sourceEvent = (event as { sourceEvent?: unknown } | undefined)?.sourceEvent;
  return sourceEvent instanceof MouseEvent ? sourceEvent : null;
}

export function CosmosCanvas({
  data,
  selected,
  onSelect,
  depth,
  diffOverlay,
  unusedIds,
  godNodeIds,
  hotspotScores,
  complexityScores,
  boundaryNodeIds,
  boundaryEdgeKeys,
  theme,
  onShowOverview,
}: Props) {
  const ref = useRef<CosmographRef>(null);
  const graphTheme = GRAPH_THEME[theme];
  const kindPalette = kindColors(theme);
  const edgePalette = edgeColors(theme);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<HoveredNode | null>(null);

  const { points, links, nodeIndexById, indexToNode, neighbors, degreeById } = useMemo(() => {
    const nodeIndexById = new Map<string, number>();
    const indexToNode = new Map<number, RawNode>();
    data.nodes.forEach((node, index) => {
      nodeIndexById.set(node.id, index);
      indexToNode.set(index, node);
    });

    const clusterById = new Map(
      data.nodes.map((node) => [node.id, graphClusterForFile(node.file)]),
    );
    const links: LinkRow[] = [];
    const neighbors = new Map<string, Set<string>>();
    const degreeById = new Map<string, number>();
    for (const edge of data.edges) {
      const sourceIndex = nodeIndexById.get(edge.src);
      const targetIndex = nodeIndexById.get(edge.dst);
      if (sourceIndex == null || targetIndex == null) continue;
      links.push({
        source: edge.src,
        target: edge.dst,
        sourceIndex,
        targetIndex,
        kind: edge.kind,
        confidence: edge.confidence ?? "extracted",
        crossBoundary: clusterById.get(edge.src) !== clusterById.get(edge.dst),
      });
      if (!neighbors.has(edge.src)) neighbors.set(edge.src, new Set());
      if (!neighbors.has(edge.dst)) neighbors.set(edge.dst, new Set());
      neighbors.get(edge.src)?.add(edge.dst);
      neighbors.get(edge.dst)?.add(edge.src);
      degreeById.set(edge.src, (degreeById.get(edge.src) ?? 0) + 1);
      degreeById.set(edge.dst, (degreeById.get(edge.dst) ?? 0) + 1);
    }

    let maxDegree = 1;
    for (const value of degreeById.values()) maxDegree = Math.max(maxDegree, value);
    const points: PointRow[] = data.nodes.map((node, index) => ({
      id: node.id,
      index,
      kind: node.kind,
      name: node.name,
      loc: node.loc,
      cluster: clusterById.get(node.id) ?? "repository root",
      clusterStrength: STRUCTURAL_KINDS.has(node.kind) ? 0.9 : 0.74,
      shape: shapeForKind(node.kind),
      labelWeight: Math.min(1, 0.2 + (degreeById.get(node.id) ?? 0) / maxDegree),
    }));

    return { points, links, nodeIndexById, indexToNode, neighbors, degreeById };
  }, [data]);

  const highlightSet = useMemo(() => {
    if (!selected) return null;
    const visited = new Set<string>([selected.id]);
    let frontier: string[] = [selected.id];
    for (let hop = 0; hop < Math.max(1, depth); hop++) {
      const next: string[] = [];
      for (const id of frontier) {
        const adjacent = neighbors.get(id);
        if (!adjacent) continue;
        for (const neighbor of adjacent) {
          if (!visited.has(neighbor)) {
            visited.add(neighbor);
            next.push(neighbor);
          }
        }
      }
      if (next.length === 0) break;
      frontier = next;
    }
    return visited;
  }, [selected, neighbors, depth]);

  const scaleProfile = useMemo(
    () => graphScaleProfile(points.length, links.length),
    [points.length, links.length],
  );
  const paintedLinks = useMemo(() => {
    const candidates =
      highlightSet && scaleProfile.tier !== "focused"
        ? links.filter(
            (link) =>
              highlightSet.has(String(link.source)) && highlightSet.has(String(link.target)),
          )
        : links;
    return sampleLinks(candidates, scaleProfile.edgeBudget);
  }, [links, highlightSet, scaleProfile]);

  useEffect(() => {
    if (!selected || !ref.current) return;
    const index = nodeIndexById.get(selected.id);
    if (index == null) return;
    ref.current.setFocusedPoint(index);
    ref.current.zoomToPoint(index, 600, 4.2, true);
  }, [selected, nodeIndexById]);

  return (
    <div ref={wrapperRef} className="absolute inset-0 overflow-hidden">
      <CosmographProvider>
        <p className="sr-only">
          Interactive force-directed graph visualization. The canvas is not navigable by keyboard.
          Press / to open the search palette and find symbols by name.
        </p>
        <Cosmograph
          ref={ref}
          points={points}
          links={paintedLinks}
          pointIdBy="id"
          pointIndexBy="index"
          pointColorBy="kind"
          pointColorByFn={(value: unknown, index?: number): string => {
            const kind = String(value);
            const node = index != null ? indexToNode.get(index) : undefined;
            let color = kindPalette[kind] ?? graphTheme.neutral;
            if (diffOverlay && node) {
              if (diffOverlay.addedIds.has(node.id)) color = graphTheme.added;
              else if (diffOverlay.removedIds.has(node.id)) color = graphTheme.removed;
            } else if (godNodeIds && node) {
              color = godNodeIds.has(node.id) ? graphTheme.impact : dimColor(color, 0.84, theme);
            } else if (unusedIds && node) {
              color = unusedIds.has(node.id) ? graphTheme.warn : dimColor(color, 0.84, theme);
            } else if (hotspotScores && node) {
              const score = hotspotScores.get(node.id);
              if (score != null) {
                color =
                  score >= 0.66
                    ? graphTheme.removed
                    : score >= 0.33
                      ? graphTheme.brand
                      : graphTheme.warn;
              } else {
                color = dimColor(color, 0.84, theme);
              }
            } else if (complexityScores && node) {
              const score = complexityScores.get(node.id) ?? 0;
              color =
                score >= 0.72
                  ? graphTheme.removed
                  : score >= 0.4
                    ? graphTheme.brand
                    : graphTheme.warn;
            } else if (boundaryNodeIds && node) {
              color = boundaryNodeIds.has(node.id)
                ? graphTheme.boundary
                : dimColor(color, 0.86, theme);
            }
            if (highlightSet && node && !highlightSet.has(node.id)) {
              return dimColor(color, 0.86, theme);
            }
            return color;
          }}
          pointShapeBy="shape"
          pointClusterBy="cluster"
          pointClusterByFn={(value: unknown) => String(value).split("/").at(-1) ?? String(value)}
          pointClusterStrengthBy="clusterStrength"
          pointSizeBy="loc"
          pointSizeByFn={(value: unknown, index?: number): number => {
            const node = index != null ? indexToNode.get(index) : undefined;
            const loc = typeof value === "number" ? value : 0;
            const base = Math.min(13, Math.max(4.8, Math.log2(loc + 2) * 1.2));
            if (!node) return base;
            if (selected?.id === node.id) return Math.min(19, base * 1.55);
            if (highlightSet?.has(node.id)) return Math.min(16, base * 1.22);
            const hotspot = hotspotScores?.get(node.id);
            if (hotspot != null) return Math.min(17, base * (1.08 + hotspot * 0.38));
            return base;
          }}
          pointLabelBy="name"
          pointLabelWeightBy="labelWeight"
          pointLabelClassName="graph-point-label"
          pointLabelFontSize={11}
          pointLabelPosition="above"
          pointLabelColor={graphTheme.label}
          pointOpacity={0.96}
          pointGreyoutColor={graphTheme.greyout}
          pointGreyoutOpacity={0.34}
          linkSourceBy="source"
          linkTargetBy="target"
          linkSourceIndexBy="sourceIndex"
          linkTargetIndexBy="targetIndex"
          linkColorBy="kind"
          linkColorByFn={(value: unknown, index?: number): string => {
            const kind = String(value);
            const link = index != null ? paintedLinks[index] : undefined;
            const confidence = link?.confidence;
            const key = link ? edgeKey(link) : null;
            const sourceHotspot = link ? (hotspotScores?.get(String(link.source)) ?? 0) : 0;
            const targetHotspot = link ? (hotspotScores?.get(String(link.target)) ?? 0) : 0;
            const relationHotspot = Math.max(sourceHotspot, targetHotspot);
            const base =
              key && diffOverlay?.addedEdgeKeys.has(key)
                ? graphTheme.added
                : key && diffOverlay?.removedEdgeKeys.has(key)
                  ? graphTheme.removed
                  : key && boundaryEdgeKeys?.has(key)
                    ? graphTheme.boundary
                    : boundaryEdgeKeys
                      ? graphTheme.greyout
                      : relationHotspot >= 0.66
                        ? graphTheme.removed
                        : relationHotspot >= 0.33
                          ? graphTheme.brand
                          : relationHotspot > 0
                            ? graphTheme.warn
                            : (edgePalette[kind] ?? graphTheme.edgeNeutral);
            if (highlightSet) {
              if (!link) return base;
              const lit =
                highlightSet.has(String(link.source)) && highlightSet.has(String(link.target));
              return lit ? base : dimColor(base, 0.9, theme);
            }
            const alpha = confidenceAlpha(confidence);
            const color = base.replace("#", "");
            const red = parseInt(color.slice(0, 2), 16);
            const green = parseInt(color.slice(2, 4), 16);
            const blue = parseInt(color.slice(4, 6), 16);
            return `rgba(${red},${green},${blue},${alpha})`;
          }}
          linkWidthBy="kind"
          linkWidthByFn={(value: unknown, index?: number): number => {
            const base = EDGE_WIDTH[String(value)] ?? 1;
            const link = index != null ? paintedLinks[index] : undefined;
            const confidence = link?.confidence;
            const relationHotspot = link
              ? Math.max(
                  hotspotScores?.get(String(link.source)) ?? 0,
                  hotspotScores?.get(String(link.target)) ?? 0,
                )
              : 0;
            const hotspotScale = 1 + relationHotspot * 1.3;
            if (confidence === "inferred") return base * 0.5 * hotspotScale;
            if (confidence === "resolved") return base * 0.78 * hotspotScale;
            return base * hotspotScale;
          }}
          linkDefaultArrows={selected !== null && scaleProfile.showArrows}
          linkArrowsSizeScale={0.28}
          linkOpacity={selected ? 0.72 : 0.34}
          linkGreyoutOpacity={0.08}
          linkVisibilityDistanceRange={[90, 320]}
          linkVisibilityMinTransparency={0.16}
          curvedLinks={selected !== null && scaleProfile.useCurves}
          curvedLinkSegments={12}
          curvedLinkWeight={0.32}
          curvedLinkControlPointDistance={0.16}
          backgroundColor="rgba(0,0,0,0)"
          spaceSize={4096}
          simulationGravity={0.16}
          simulationCenter={0.04}
          simulationRepulsion={1.45}
          simulationRepulsionTheta={1.1}
          simulationFriction={0.84}
          simulationDecay={1200}
          simulationLinkSpring={1.05}
          simulationLinkDistance={5}
          simulationLinkDistRandomVariationRange={[0.85, 1.15]}
          preservePointPositionsOnDataUpdate
          showDynamicLabels
          showDynamicLabelsLimit={scaleProfile.dynamicLabelLimit}
          showTopLabels
          showTopLabelsLimit={scaleProfile.topLabelLimit}
          showClusterLabels={!selected && points.length > 1_000}
          showClusterLabelsLimit={12}
          clusterLabelFontSize={11}
          clusterLabelClassName="graph-cluster-label"
          scaleClusterLabels={false}
          usePointColorStrategyForClusterLabels={false}
          showHoveredPointLabel
          renderHoveredPointRing
          hoveredPointCursor="pointer"
          hoveredPointRingColor={graphTheme.brand}
          focusedPointRingColor={graphTheme.brand}
          fitViewOnInit
          fitViewDelay={1000}
          selectPointOnClick={false}
          focusPointOnClick={false}
          onPointMouseOver={(index, _position, event) => {
            const node = indexToNode.get(index);
            const pointer = pointerFromEvent(event);
            const bounds = wrapperRef.current?.getBoundingClientRect();
            if (!node || !pointer || !bounds) return;
            setHovered({
              node,
              x: Math.max(12, Math.min(bounds.width - 292, pointer.clientX - bounds.left + 14)),
              y: Math.max(12, Math.min(bounds.height - 126, pointer.clientY - bounds.top + 14)),
              degree: degreeById.get(node.id) ?? 0,
            });
          }}
          onPointMouseOut={() => setHovered(null)}
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
        <div className="graph-panel pointer-events-none absolute top-4 left-4 rounded-md px-3 py-2.5">
          <div className="mb-1 font-mono text-[9px] font-semibold tracking-[0.14em] text-(--color-text-dim) uppercase">
            Architecture map
          </div>
          <div className="mb-2 font-mono text-[8px] text-(--color-text-dim)">
            {scaleProfile.label} · {paintedLinks.length.toLocaleString()} /{" "}
            {links.length.toLocaleString()} relations painted
          </div>
          {onShowOverview && (
            <button
              onClick={onShowOverview}
              className="pointer-events-auto mb-2 rounded border border-(--color-border-subtle) px-2 py-1 font-mono text-[8px] text-(--color-text-muted) hover:border-(--color-accent) hover:text-(--color-accent)"
            >
              grouped overview
            </button>
          )}
          <div className="grid grid-cols-3 gap-x-3 gap-y-1.5 text-[10px] text-(--color-text-muted)">
            <Key color={kindPalette.function} shape="rounded-full" label="Callable" />
            <Key color={kindPalette.struct} shape="rotate-45" label="Type" />
            <Key color={kindPalette.module} shape="rounded-[2px]" label="Module" />
          </div>
        </div>
        {hovered && (
          <div
            className="graph-panel pointer-events-none absolute z-30 w-[278px] rounded-md p-3"
            style={{ left: hovered.x, top: hovered.y }}
          >
            <div className="mb-1 flex items-center justify-between gap-3">
              <span className="truncate font-mono text-[12px] font-semibold text-(--color-text-primary)">
                {hovered.node.name}
              </span>
              <span className="shrink-0 font-mono text-[9px] tracking-wider text-(--color-text-dim) uppercase">
                {hovered.node.kind}
              </span>
            </div>
            <div className="truncate font-mono text-[10px] text-(--color-text-muted)">
              {hovered.node.qualified_name || hovered.node.name}
            </div>
            <div className="mt-2 flex items-center gap-2 border-t border-(--color-border-subtle) pt-2 font-mono text-[9px] text-(--color-text-dim)">
              <span>{hovered.degree} relations</span>
              <span>·</span>
              <span>{hovered.node.loc} LOC</span>
              <span className="min-w-0 flex-1 truncate text-right">
                {hovered.node.file.split("/").slice(-2).join("/")}
              </span>
            </div>
          </div>
        )}
        <CanvasControls cosmoRef={ref} />
      </CosmographProvider>
    </div>
  );
}

function Key({ color, shape, label }: { color: string; shape: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className={`size-2 ${shape}`} style={{ background: color }} />
      <span>{label}</span>
    </span>
  );
}
