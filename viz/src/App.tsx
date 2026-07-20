import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
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
import { FilterRail, type Flag, type Visibility } from "./components/FilterRail";

const CosmosCanvas = lazy(() =>
  import("./components/CosmosCanvas").then((module) => ({ default: module.CosmosCanvas })),
);
import { Inspector } from "./components/Inspector";
import { StatusBar } from "./components/StatusBar";
import { SearchPalette } from "./components/SearchPalette";
import { KeyboardHelp } from "./components/KeyboardHelp";
import { applyDensity, type DensityMode } from "./graph/density";
import type { ViewMode } from "./graph/view";
import { useBranchDiff } from "./hooks/useBranchDiff";

export default function App() {
  const [rawData, setRawData] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadProgress, setLoadProgress] = useState<GraphLoadProgress | null>(null);
  const [selected, setSelected] = useState<RawNode | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("atlas");
  const [focusedData, setFocusedData] = useState<GraphData | null>(null);
  const [focusLimitReached, setFocusLimitReached] = useState(false);
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
  // Track whether a fetch has already been dispatched for the current overlay
  // toggle session — prevents an infinite re-fetch loop when the server returns
  // zero results (empty Set still has size=0, which would re-trigger the effect).
  const unusedFetchedRef = useRef(false);
  const godNodesFetchedRef = useRef(false);
  const hotFilesFetchedRef = useRef(false);
  const diffOverlay = useBranchDiff(activeBranch, diffHead);

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
    setRawData(null);
    setError(null);
    setLoadProgress(null);
    setSelected(null);
    setDiffHead(null);
    setUnusedIds(null);
    setGodNodeIds(null);
    setHotFiles(null);
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
          unusedFetchedRef.current = false;
          setUnusedIds((cur) => (cur ? null : new Set()));
          break;
        case "g":
        case "G":
          godNodesFetchedRef.current = false;
          setGodNodeIds((cur) => (cur ? null : new Set()));
          break;
        case "c":
        case "C":
          hotFilesFetchedRef.current = false;
          setHotFiles((current) => (current ? null : []));
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchOpen, helpOpen]);

  useEffect(() => {
    if (viewMode !== "investigate" || !selected || !activeBranch) {
      setFocusedData(null);
      setFocusLimitReached(false);
      return;
    }
    const controller = new AbortController();
    fetchNeighborhood(selected.id, activeBranch, "both", 500, controller.signal)
      .then((result) => {
        setFocusedData({ nodes: result.nodes, edges: result.edges });
        setFocusLimitReached(result.limit_reached);
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
    if (!rawData || !hotFiles || hotFiles.length === 0) return null;
    const touchesByFile = new Map(hotFiles.map((file) => [file.path, file.touches]));
    const maxTouches = Math.max(...hotFiles.map((file) => file.touches), 1);
    return new Map(
      rawData.nodes
        .filter((node) => touchesByFile.has(node.file))
        .map((node) => [node.id, (touchesByFile.get(node.file) ?? 0) / maxTouches]),
    );
  }, [rawData, hotFiles]);

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

  return (
    <div className="flex h-screen flex-col bg-(--color-void) text-(--color-text-primary)">
      <Header
        onSearch={() => setSearchOpen(true)}
        onShowHelp={() => setHelpOpen(true)}
        activeBranch={activeBranch}
        onSetActiveBranch={setActiveBranch}
        diffHead={diffHead}
        onSetDiffHead={setDiffHead}
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
            unusedActive={unusedIds !== null}
            onToggleUnused={() => {
              unusedFetchedRef.current = false;
              setUnusedIds((current) => (current ? null : new Set()));
            }}
            godNodesActive={godNodeIds !== null}
            onToggleGodNodes={() => {
              godNodesFetchedRef.current = false;
              setGodNodeIds((current) => (current ? null : new Set()));
            }}
            hotFilesActive={hotFiles !== null}
            onToggleHotFiles={() => {
              hotFilesFetchedRef.current = false;
              setHotFiles((current) => (current ? null : []));
            }}
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
              className="absolute top-3 left-3 z-10 rounded-md border border-(--color-border-subtle) bg-(--color-elevated)/80 px-2 py-1 text-(--color-text-muted) backdrop-blur-sm hover:text-(--color-text-primary)"
            >
              Filters
            </button>
          )}
          {error && (
            <div className="absolute inset-0 flex items-center justify-center p-6">
              <div className="max-w-[560px] rounded-2xl border border-red-500/20 bg-(--color-elevated)/95 p-5 shadow-2xl">
                <div className="mb-1 text-[13px] font-semibold text-red-300">
                  The graph could not be loaded
                </div>
                <p className="mb-3 text-[11px] text-(--color-text-muted)">{error}</p>
                <div className="rounded-lg bg-(--color-void) px-3 py-2 font-mono text-[10px] text-(--color-text-primary)">
                  gcx hook
                </div>
              </div>
            </div>
          )}
          {data && !error && data.nodes.length > 0 && (
            <Suspense
              fallback={
                <div className="absolute inset-0 flex items-center justify-center text-(--color-text-muted)">
                  Loading GPU renderer…
                </div>
              }
            >
              <CosmosCanvas
                data={data}
                hiddenKinds={hiddenKinds}
                hiddenEdgeKinds={hiddenEdgeKinds}
                hiddenConfidence={hiddenConfidence}
                selected={selected}
                onSelect={setSelected}
                depth={depth}
                diffOverlay={diffOverlay}
                unusedIds={unusedIds}
                godNodeIds={godNodeIds}
                hotspotScores={hotspotNodeScores}
              />
            </Suspense>
          )}
          {data && !error && data.nodes.length === 0 && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center text-(--color-text-muted)">
              <div className="text-lg">Graph is empty</div>
              <div className="font-mono text-xs">
                Run <span className="text-(--color-text-primary)">gcx hook</span> (or
                <span className="text-(--color-text-primary)"> gcx init</span>) to index this
                repository, then refresh.
              </div>
            </div>
          )}
          {!data && !error && (
            <div className="absolute inset-0 flex items-center justify-center text-(--color-text-muted)">
              Loading graph manifest…
            </div>
          )}
          {data && loadProgress && loadProgress.stage !== "complete" && (
            <div className="animate-fade-in absolute bottom-3 left-3 z-20 w-[320px] rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/90 p-3 font-mono text-[11px] backdrop-blur-sm">
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
            <div className="animate-fade-in absolute top-3 right-3 z-10 flex items-center gap-2 rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/90 px-3 py-1.5 font-mono text-[11px] backdrop-blur-sm">
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
            <div className="animate-fade-in absolute top-3 right-3 z-10 flex items-center gap-3 rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/90 px-3 py-1.5 font-mono text-[11px] backdrop-blur-sm">
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
            hotspot={hotFiles?.find((file) => file.path === selected.file) ?? null}
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
