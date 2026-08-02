import {
  Component,
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import type { FileHotspot, GraphData, GraphLoadProgress, RawNode } from "./api";
import {
  fetchBranches,
  fetchGodNodes,
  fetchHotFiles,
  fetchNeighborhood,
  fetchUnused,
  loadGraphData,
} from "./api";
import { Header } from "./components/Header";
import { CompatibilityAtlas } from "./components/CompatibilityAtlas";
import { FilterRail, type Flag, type InsightLens, type Visibility } from "./components/FilterRail";

const CosmosCanvas = lazy(() =>
  import("./components/CosmosCanvas").then((module) => ({ default: module.CosmosCanvas })),
);
import { Inspector } from "./components/Inspector";
import { StatusBar } from "./components/StatusBar";
import { SearchPalette } from "./components/SearchPalette";
import { KeyboardHelp } from "./components/KeyboardHelp";
import { applyDensity, type DensityMode } from "./graph/density";
import { graphClusterForFile } from "./graph/semantics";
import type { ViewMode } from "./graph/view";
import { useBranchDiff } from "./hooks/useBranchDiff";
import type { ProductTheme } from "./theme/colors";

type RendererMode = "webgl" | "compatibility";

function supportsAcceleratedWebGl(): boolean {
  try {
    const canvas = document.createElement("canvas");
    return Boolean(
      canvas.getContext("webgl2", {
        failIfMajorPerformanceCaveat: true,
        powerPreference: "high-performance",
      }),
    );
  } catch {
    return false;
  }
}

class RendererBoundary extends Component<
  { children: ReactNode; onFailure: () => void },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    this.props.onFailure();
  }

  render() {
    return this.state.failed ? null : this.props.children;
  }
}

export default function App() {
  const [theme, setTheme] = useState<ProductTheme>(() =>
    document.documentElement.dataset.theme === "dark" ? "dark" : "light",
  );
  const [rendererMode, setRendererMode] = useState<RendererMode>(() =>
    supportsAcceleratedWebGl() ? "webgl" : "compatibility",
  );
  const [rawData, setRawData] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadProgress, setLoadProgress] = useState<GraphLoadProgress | null>(null);
  const [selected, setSelected] = useState<RawNode | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("atlas");
  const [focusResult, setFocusResult] = useState<{
    nodeId: string;
    branch: string;
    data: GraphData;
    limitReached: boolean;
  } | null>(null);
  const [depth, setDepth] = useState(1);
  const [hiddenKinds, setHiddenKinds] = useState<Set<string>>(new Set());
  const [hiddenEdgeKinds, setHiddenEdgeKinds] = useState<Set<string>>(
    new Set(["contains", "imports"]),
  );
  const [hiddenConfidence, setHiddenConfidence] = useState<Set<string>>(new Set());
  const [hiddenVisibility, setHiddenVisibility] = useState<Set<Visibility>>(new Set());
  const [flagFilter, setFlagFilter] = useState<Set<Flag>>(new Set());
  const [railOpen, setRailOpen] = useState(true);
  const [density, setDensity] = useState<DensityMode>("focused");
  const [searchOpen, setSearchOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeBranch, setActiveBranch] = useState<string | null>(null);
  const [lastSha, setLastSha] = useState<string | null>(null);
  const [diffHead, setDiffHead] = useState<string | null>(null);
  const [unusedIds, setUnusedIds] = useState<Set<string> | null>(null);
  const [godNodeIds, setGodNodeIds] = useState<Set<string> | null>(null);
  const [hotFiles, setHotFiles] = useState<FileHotspot[] | null>(null);
  const [insightLens, setInsightLens] = useState<InsightLens>(null);
  // Track whether a fetch has already been dispatched for the current overlay
  // toggle session — prevents an infinite re-fetch loop when the server returns
  // zero results (empty Set still has size=0, which would re-trigger the effect).
  const unusedFetchedRef = useRef(false);
  const godNodesFetchedRef = useRef(false);
  const hotFilesFetchedRef = useRef(false);
  const diffOverlay = useBranchDiff(activeBranch, diffHead);
  const focusIsCurrent =
    viewMode === "investigate" &&
    selected !== null &&
    focusResult?.nodeId === selected.id &&
    focusResult.branch === activeBranch;
  const focusedData = focusIsCurrent ? focusResult.data : null;
  const focusLimitReached = focusIsCurrent ? focusResult.limitReached : false;

  useEffect(() => {
    const onContextLost = (event: Event) => {
      event.preventDefault();
      setRendererMode("compatibility");
    };
    document.addEventListener("webglcontextlost", onContextLost, true);
    return () => document.removeEventListener("webglcontextlost", onContextLost, true);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    localStorage.setItem("gitcortex-theme", theme);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", theme === "dark" ? "#101216" : "#fcfcfa");
  }, [theme]);

  const selectInsightLens = useCallback(
    (lens: Exclude<InsightLens, null>) => {
      const next = insightLens === lens ? null : lens;
      setInsightLens(next);
      if (next === "changes" && hotFiles === null) {
        hotFilesFetchedRef.current = false;
        setHotFiles([]);
      } else if (next === "impact" && godNodeIds === null) {
        godNodesFetchedRef.current = false;
        setGodNodeIds(new Set());
      } else if (next === "unused" && unusedIds === null) {
        unusedFetchedRef.current = false;
        setUnusedIds(new Set());
      }
    },
    [godNodeIds, hotFiles, insightLens, unusedIds],
  );

  useEffect(() => {
    fetchBranches()
      .then((branches) => {
        setActiveBranch(branches.active);
        setLastSha(branches.last_sha);
      })
      .catch((fetchError) => setError(String(fetchError)));
  }, []);

  useEffect(() => {
    if (!activeBranch) return;
    const controller = new AbortController();
    loadGraphData(
      activeBranch,
      (partial, progress) => {
        setRawData(partial);
        setLoadProgress(progress);
        setLastSha(progress.snapshot);
      },
      controller.signal,
    ).catch((loadError) => {
      if (loadError instanceof DOMException && loadError.name === "AbortError") return;
      setError(String(loadError));
    });
    return () => controller.abort();
  }, [activeBranch]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const isInput =
        e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
        return;
      }
      if (e.key === "Escape") {
        if (searchOpen) setSearchOpen(false);
        else if (helpOpen) setHelpOpen(false);
        else setSelected(null);
        return;
      }
      if (isInput) return;
      switch (e.key) {
        case "/":
          e.preventDefault();
          setSearchOpen(true);
          break;
        case "?":
          e.preventDefault();
          setHelpOpen(true);
          break;
        case "[":
          setRailOpen((r) => !r);
          break;
        case "]":
          setSelected(null);
          break;
        case "1":
          setDensity("focused");
          break;
        case "2":
          setDensity("public");
          break;
        case "3":
          setDensity("full");
          break;
        case "u":
        case "U":
          selectInsightLens("unused");
          break;
        case "g":
        case "G":
          selectInsightLens("impact");
          break;
        case "c":
        case "C":
          selectInsightLens("changes");
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchOpen, helpOpen, selectInsightLens]);

  useEffect(() => {
    if (viewMode !== "investigate" || !selected || !activeBranch) return;
    const controller = new AbortController();
    fetchNeighborhood(selected.id, activeBranch, "both", 500, controller.signal)
      .then((result) => {
        setFocusResult({
          nodeId: selected.id,
          branch: activeBranch,
          data: { nodes: result.nodes, edges: result.edges },
          limitReached: result.limit_reached,
        });
      })
      .catch((focusError) => {
        if (focusError instanceof DOMException && focusError.name === "AbortError") return;
        setError(String(focusError));
      });
    return () => controller.abort();
  }, [viewMode, selected, activeBranch]);

  // Fetch unused symbols when toggle flips on (guard ref prevents re-fetch loop
  // when server returns zero results — empty Set still has size=0).
  useEffect(() => {
    if (unusedIds !== null && unusedIds.size === 0 && !unusedFetchedRef.current) {
      unusedFetchedRef.current = true;
      if (!activeBranch) return;
      fetchUnused(activeBranch)
        .then((result) => setUnusedIds(new Set(result.nodes.map((node) => node.id))))
        .catch(() => setUnusedIds(null));
    }
  }, [unusedIds, activeBranch]);

  // Fetch god nodes when toggle flips on (same guard pattern as above).
  useEffect(() => {
    if (godNodeIds !== null && godNodeIds.size === 0 && !godNodesFetchedRef.current) {
      godNodesFetchedRef.current = true;
      if (!activeBranch) return;
      fetchGodNodes(activeBranch)
        .then((result) => setGodNodeIds(new Set(result.nodes.map((node) => node.id))))
        .catch(() => setGodNodeIds(null));
    }
  }, [godNodeIds, activeBranch]);

  useEffect(() => {
    if (hotFiles !== null && hotFiles.length === 0 && !hotFilesFetchedRef.current) {
      if (!activeBranch) return;
      hotFilesFetchedRef.current = true;
      fetchHotFiles(activeBranch)
        .then((result) => setHotFiles(result.files))
        .catch(() => setHotFiles(null));
    }
  }, [hotFiles, activeBranch]);

  const hotspotNodeScores = useMemo(() => {
    if (insightLens !== "changes" || !rawData || !hotFiles || hotFiles.length === 0) return null;
    const touchesByFile = new Map(hotFiles.map((file) => [file.path, file.touches]));
    const maxTouches = Math.max(...hotFiles.map((file) => file.touches), 1);
    return new Map(
      rawData.nodes
        .filter((node) => touchesByFile.has(node.file))
        .map((node) => [node.id, (touchesByFile.get(node.file) ?? 0) / maxTouches]),
    );
  }, [rawData, hotFiles, insightLens]);

  const complexityScores = useMemo(() => {
    if (insightLens !== "complexity" || !rawData) return null;
    const degree = new Map<string, number>();
    for (const edge of rawData.edges) {
      degree.set(edge.src, (degree.get(edge.src) ?? 0) + 1);
      degree.set(edge.dst, (degree.get(edge.dst) ?? 0) + 1);
    }
    let maxDegree = 1;
    for (const value of degree.values()) maxDegree = Math.max(maxDegree, value);
    let maxLoc = 1;
    for (const node of rawData.nodes) maxLoc = Math.max(maxLoc, node.loc);
    const locScale = Math.log2(maxLoc + 1);
    return new Map(
      rawData.nodes.map((node) => {
        const locRisk = Math.log2(node.loc + 1) / locScale;
        const couplingRisk = Math.log2((degree.get(node.id) ?? 0) + 1) / Math.log2(maxDegree + 1);
        return [node.id, locRisk * 0.55 + couplingRisk * 0.45];
      }),
    );
  }, [rawData, insightLens]);

  const boundaryLens = useMemo(() => {
    if (insightLens !== "boundaries" || !rawData) return null;
    const clusterById = new Map(
      rawData.nodes.map((node) => [node.id, graphClusterForFile(node.file)]),
    );
    const nodeIds = new Set<string>();
    const edgeKeys = new Set<string>();
    for (const edge of rawData.edges) {
      if (clusterById.get(edge.src) === clusterById.get(edge.dst)) continue;
      nodeIds.add(edge.src);
      nodeIds.add(edge.dst);
      edgeKeys.add(`${edge.src}\u0000${edge.dst}\u0000${edge.kind}`);
    }
    return { nodeIds, edgeKeys };
  }, [rawData, insightLens]);

  const data = useMemo(() => {
    if (!rawData) return null;
    let source =
      viewMode === "investigate"
        ? (focusedData ?? { nodes: selected ? [selected] : [], edges: [] })
        : rawData;
    if (diffOverlay && viewMode === "atlas") {
      const nodesById = new Map(rawData.nodes.map((node) => [node.id, node]));
      for (const node of diffOverlay.addedNodes) nodesById.set(node.id, node);
      const edgeKeys = new Set(
        rawData.edges.map((edge) => `${edge.src}\u0000${edge.dst}\u0000${edge.kind}`),
      );
      const edges = rawData.edges.slice();
      for (const edge of diffOverlay.addedEdges) {
        const key = `${edge.src}\u0000${edge.dst}\u0000${edge.kind}`;
        if (!edgeKeys.has(key)) {
          edgeKeys.add(key);
          edges.push(edge);
        }
      }
      source = { nodes: [...nodesById.values()], edges };
    }
    let d = applyDensity(source, density);
    const keep = new Set(
      d.nodes
        .filter(
          (node) =>
            !hiddenKinds.has(node.kind) &&
            !hiddenVisibility.has(node.visibility as Visibility) &&
            (flagFilter.size === 0 ||
              (flagFilter.has("async") && node.is_async) ||
              (flagFilter.has("unsafe") && node.is_unsafe)),
        )
        .map((node) => node.id),
    );
    d = {
      nodes: d.nodes.filter((node) => keep.has(node.id)),
      edges: d.edges.filter(
        (edge) =>
          keep.has(edge.src) &&
          keep.has(edge.dst) &&
          !hiddenEdgeKinds.has(edge.kind) &&
          !hiddenConfidence.has(edge.confidence ?? "extracted"),
      ),
    };
    return d;
  }, [
    rawData,
    focusedData,
    selected,
    viewMode,
    density,
    hiddenKinds,
    hiddenEdgeKinds,
    hiddenConfidence,
    hiddenVisibility,
    flagFilter,
    diffOverlay,
  ]);

  const insightSummary =
    insightLens === "changes"
      ? `Change hotspots · ${hotFiles?.length ?? 0} files ranked`
      : insightLens === "impact"
        ? `High-impact hubs · ${godNodeIds?.size ?? 0} symbols`
        : insightLens === "unused"
          ? `Unused candidates · ${unusedIds?.size ?? 0} symbols`
          : insightLens === "complexity"
            ? `Complexity risk · ${complexityScores?.size ?? 0} symbols scored`
            : insightLens === "boundaries"
              ? `Boundary crossings · ${boundaryLens?.edgeKeys.size ?? 0} relations`
              : null;

  const selectActiveBranch = (branch: string) => {
    setRawData(null);
    setError(null);
    setLoadProgress(null);
    setSelected(null);
    setFocusResult(null);
    setDiffHead(null);
    setUnusedIds(null);
    setGodNodeIds(null);
    setHotFiles(null);
    setInsightLens(null);
    setActiveBranch(branch);
  };

  return (
    <div className="flex h-screen flex-col bg-(--color-void) text-(--color-text-primary)">
      <Header
        onSearch={() => setSearchOpen(true)}
        onShowHelp={() => setHelpOpen(true)}
        activeBranch={activeBranch}
        onSetActiveBranch={selectActiveBranch}
        diffHead={diffHead}
        onSetDiffHead={setDiffHead}
        theme={theme}
        onToggleTheme={() => setTheme((current) => (current === "light" ? "dark" : "light"))}
      />
      <main className="flex flex-1 overflow-hidden">
        {railOpen && (
          <FilterRail
            data={rawData}
            visibleNodeCount={data?.nodes.length ?? 0}
            hotFiles={hotFiles}
            density={density}
            onDensityChange={setDensity}
            viewMode={viewMode}
            onViewModeChange={(mode) => {
              setViewMode(mode);
              if (mode === "investigate") setDensity("full");
            }}
            canInvestigate={selected !== null}
            insightLens={insightLens}
            onInsightLensChange={selectInsightLens}
            hiddenKinds={hiddenKinds}
            setHiddenKinds={setHiddenKinds}
            hiddenEdgeKinds={hiddenEdgeKinds}
            setHiddenEdgeKinds={setHiddenEdgeKinds}
            hiddenConfidence={hiddenConfidence}
            setHiddenConfidence={setHiddenConfidence}
            hiddenVisibility={hiddenVisibility}
            setHiddenVisibility={setHiddenVisibility}
            flagFilter={flagFilter}
            setFlagFilter={setFlagFilter}
            onCollapse={() => setRailOpen(false)}
          />
        )}
        <div className="canvas-bg relative flex-1">
          {!railOpen && (
            <button
              onClick={() => setRailOpen(true)}
              title="Show filters ([ )"
              className="graph-panel absolute top-4 left-4 z-10 rounded-md px-2.5 py-1.5 text-[11px] font-medium text-(--color-text-muted) hover:text-(--color-accent)"
            >
              Filters
            </button>
          )}
          {error && (
            <div className="absolute inset-0 flex items-center justify-center p-6">
              <div className="graph-panel max-w-[560px] rounded-lg border-(--color-bad)/25 p-5">
                <div className="mb-1 text-[13px] font-semibold text-(--color-bad)">
                  The graph could not be loaded
                </div>
                <p className="mb-3 text-[11px] text-(--color-text-muted)">{error}</p>
                <div className="rounded-md border border-(--color-border-subtle) bg-(--color-elevated) px-3 py-2 font-mono text-[10px] text-(--color-text-primary)">
                  gcx hook
                </div>
              </div>
            </div>
          )}
          {data && !error && data.nodes.length > 0 && rendererMode === "compatibility" && (
            <CompatibilityAtlas
              data={data}
              selected={selected}
              onSelect={setSelected}
              onSearch={() => setSearchOpen(true)}
              onTryWebGl={() => setRendererMode("webgl")}
            />
          )}
          {data && !error && data.nodes.length > 0 && rendererMode === "webgl" && (
            <RendererBoundary onFailure={() => setRendererMode("compatibility")}>
              <Suspense
                fallback={
                  <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-(--color-text-muted)">
                    <span className="size-5 animate-spin rounded-full border-2 border-(--color-border-subtle) border-t-(--color-accent)" />
                    <span className="font-mono text-[10px]">Preparing accelerated renderer…</span>
                  </div>
                }
              >
                <CosmosCanvas
                  data={data}
                  selected={selected}
                  onSelect={setSelected}
                  depth={depth}
                  diffOverlay={diffOverlay}
                  unusedIds={insightLens === "unused" ? unusedIds : null}
                  godNodeIds={insightLens === "impact" ? godNodeIds : null}
                  hotspotScores={hotspotNodeScores}
                  complexityScores={complexityScores}
                  boundaryNodeIds={boundaryLens?.nodeIds ?? null}
                  boundaryEdgeKeys={boundaryLens?.edgeKeys ?? null}
                  theme={theme}
                />
              </Suspense>
            </RendererBoundary>
          )}
          {data && !error && data.nodes.length === 0 && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center text-(--color-text-muted)">
              <div className="text-[15px] font-semibold">No indexed symbols on this branch</div>
              <div className="font-mono text-xs">
                Run <span className="text-(--color-text-primary)">gcx hook</span> (or
                <span className="text-(--color-text-primary)"> gcx init</span>) to index this
                repository, then refresh.
              </div>
            </div>
          )}
          {!data && !error && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-(--color-text-muted)">
              <span className="size-5 animate-spin rounded-full border-2 border-(--color-border-subtle) border-t-(--color-accent)" />
              <span className="font-mono text-[10px]">Reading graph manifest…</span>
            </div>
          )}
          {data && loadProgress && loadProgress.stage !== "complete" && (
            <div className="graph-panel animate-fade-in absolute bottom-4 left-4 z-20 w-[320px] rounded-md p-3 font-mono text-[10px]">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-(--color-text-primary)">Loading {loadProgress.stage}</span>
                <span className="text-(--color-text-dim)">
                  {loadProgress.stage === "nodes"
                    ? `${loadProgress.loadedNodes} / ${loadProgress.totalNodes}`
                    : `${loadProgress.loadedEdges} / ${loadProgress.totalEdges}`}
                </span>
              </div>
              <progress
                className="h-1.5 w-full accent-(--color-accent)"
                max={
                  loadProgress.stage === "nodes"
                    ? Math.max(loadProgress.totalNodes, 1)
                    : Math.max(loadProgress.totalEdges, 1)
                }
                value={
                  loadProgress.stage === "nodes"
                    ? loadProgress.loadedNodes
                    : loadProgress.loadedEdges
                }
              />
            </div>
          )}
          {viewMode === "investigate" && selected && (
            <div className="graph-panel animate-fade-in absolute top-4 right-4 z-10 flex items-center gap-2 rounded-md px-3 py-2 font-mono text-[10px]">
              <span className="size-2 rounded-full bg-(--color-accent)" />
              <span>Investigation · {selected.name}</span>
              {focusLimitReached && <span className="text-(--color-warn)">first 500 edges</span>}
              <button
                onClick={() => setViewMode("atlas")}
                className="ml-1 text-(--color-text-dim) hover:text-(--color-text-primary)"
              >
                back to atlas
              </button>
            </div>
          )}
          {diffOverlay && viewMode === "atlas" && (
            <div className="graph-panel animate-fade-in absolute top-4 right-4 z-10 flex items-center gap-3 rounded-md px-3 py-2 font-mono text-[10px]">
              <span className="text-(--color-text-dim)">
                {diffOverlay.base} ↔ {diffOverlay.head}
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2 rounded-full bg-emerald-500" />
                <span>added {diffOverlay.addedIds.size}</span>
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2 rounded-full bg-red-500" />
                <span>removed {diffOverlay.removedIds.size}</span>
              </span>
            </div>
          )}
          {insightSummary && viewMode === "atlas" && !diffOverlay && (
            <div className="graph-panel animate-fade-in absolute top-4 right-4 z-10 flex items-center gap-2 rounded-md px-3 py-2 font-mono text-[9px]">
              <span className="size-2 rounded-full bg-(--color-accent)" />
              <span className="text-(--color-text-muted)">{insightSummary}</span>
              <button
                onClick={() => setInsightLens(null)}
                className="ml-1 text-(--color-text-dim) hover:text-(--color-accent)"
              >
                clear
              </button>
            </div>
          )}
        </div>
        {selected && (
          <Inspector
            node={selected}
            data={data}
            onClose={() => setSelected(null)}
            onSelect={setSelected}
            depth={depth}
            onDepthChange={setDepth}
            branch={activeBranch ?? "main"}
            hotspot={
              insightLens === "changes"
                ? (hotFiles?.find((file) => file.path === selected.file) ?? null)
                : null
            }
          />
        )}
      </main>
      <StatusBar
        data={data}
        selected={selected}
        activeBranch={activeBranch}
        lastSha={lastSha}
        diffOverlay={diffOverlay}
      />
      {searchOpen && (
        <SearchPalette
          data={rawData}
          onClose={() => setSearchOpen(false)}
          onSelect={(node) => {
            setDensity("full");
            setViewMode("investigate");
            setHiddenKinds((hidden) => {
              const next = new Set(hidden);
              next.delete(node.kind);
              return next;
            });
            setHiddenVisibility((hidden) => {
              const next = new Set(hidden);
              next.delete(node.visibility as Visibility);
              return next;
            });
            setFlagFilter(new Set());
            setSelected(node);
          }}
        />
      )}
      {helpOpen && <KeyboardHelp onClose={() => setHelpOpen(false)} />}
    </div>
  );
}
