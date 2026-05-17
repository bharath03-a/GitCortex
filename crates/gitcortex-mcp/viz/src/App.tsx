import { useEffect, useMemo, useState } from "react";
import type { GraphData, RawNode } from "./api";
import { fetchBranches, fetchGraphData, fetchUnused } from "./api";
import { Header } from "./components/Header";
import { FilterRail, type Flag, type Visibility } from "./components/FilterRail";
import { CosmosCanvas } from "./components/CosmosCanvas";
import { Inspector } from "./components/Inspector";
import { StatusBar } from "./components/StatusBar";
import { SearchPalette } from "./components/SearchPalette";
import { KeyboardHelp } from "./components/KeyboardHelp";
import { applyDensity, type DensityMode } from "./graph/density";
import { useBranchDiff } from "./hooks/useBranchDiff";

export default function App() {
  const [rawData, setRawData] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<RawNode | null>(null);
  const [depth, setDepth] = useState(1);
  const [hiddenKinds, setHiddenKinds] = useState<Set<string>>(new Set());
  const [hiddenEdgeKinds, setHiddenEdgeKinds] = useState<Set<string>>(
    new Set(["contains", "imports"]),
  );
  const [hiddenVisibility, setHiddenVisibility] = useState<Set<Visibility>>(
    new Set(),
  );
  const [flagFilter, setFlagFilter] = useState<Set<Flag>>(new Set());
  const [railOpen, setRailOpen] = useState(true);
  const [density, setDensity] = useState<DensityMode>("focused");
  const [searchOpen, setSearchOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeBranch, setActiveBranch] = useState<string | null>(null);
  const [lastSha, setLastSha] = useState<string | null>(null);
  const [diffHead, setDiffHead] = useState<string | null>(null);
  const [unusedIds, setUnusedIds] = useState<Set<string> | null>(null);
  const diffOverlay = useBranchDiff(activeBranch, diffHead);

  useEffect(() => {
    fetchGraphData()
      .then(setRawData)
      .catch((e) => setError(String(e)));
    fetchBranches()
      .then((b) => {
        setActiveBranch(b.active);
        setLastSha(b.last_sha);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const isInput =
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement;
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
          setUnusedIds((cur) => (cur ? null : new Set()));
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchOpen, helpOpen]);

  // Fetch unused symbols when toggle flips on
  useEffect(() => {
    if (unusedIds !== null && unusedIds.size === 0) {
      fetchUnused()
        .then((r) => setUnusedIds(new Set(r.nodes.map((n) => n.id))))
        .catch(() => setUnusedIds(null));
    }
  }, [unusedIds]);

  const data = useMemo(() => {
    if (!rawData) return null;
    let d = applyDensity(rawData, density);
    // Apply visibility / flag filters in a single pass
    if (hiddenVisibility.size > 0 || flagFilter.size > 0) {
      const keep = new Set(
        d.nodes
          .filter(
            (n) =>
              !hiddenVisibility.has(n.visibility as Visibility) &&
              (flagFilter.size === 0 ||
                (flagFilter.has("async") && n.is_async) ||
                (flagFilter.has("unsafe") && n.is_unsafe)),
          )
          .map((n) => n.id),
      );
      d = {
        nodes: d.nodes.filter((n) => keep.has(n.id)),
        edges: d.edges.filter((e) => keep.has(e.src) && keep.has(e.dst)),
      };
    }
    return d;
  }, [rawData, density, hiddenVisibility, flagFilter]);

  return (
    <div className="flex h-screen flex-col bg-(--color-void) text-(--color-text-primary)">
      <Header
        nodeCount={data?.nodes.length ?? 0}
        totalNodeCount={rawData?.nodes.length ?? 0}
        density={density}
        onDensityChange={setDensity}
        onSearch={() => setSearchOpen(true)}
        onShowHelp={() => setHelpOpen(true)}
        activeBranch={activeBranch}
        diffHead={diffHead}
        onSetDiffHead={setDiffHead}
        unusedActive={unusedIds !== null}
        onToggleUnused={() => setUnusedIds((cur) => (cur ? null : new Set()))}
      />
      <main className="flex flex-1 overflow-hidden">
        {railOpen && (
          <FilterRail
            data={data}
            hiddenKinds={hiddenKinds}
            setHiddenKinds={setHiddenKinds}
            hiddenEdgeKinds={hiddenEdgeKinds}
            setHiddenEdgeKinds={setHiddenEdgeKinds}
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
            <div className="absolute inset-0 flex items-center justify-center text-(--color-bad)">
              {error}
            </div>
          )}
          {data && !error && (
            <CosmosCanvas
              data={data}
              hiddenKinds={hiddenKinds}
              hiddenEdgeKinds={hiddenEdgeKinds}
              selected={selected}
              onSelect={setSelected}
              depth={depth}
              diffOverlay={diffOverlay}
              unusedIds={unusedIds}
            />
          )}
          {!data && !error && (
            <div className="absolute inset-0 flex items-center justify-center text-(--color-text-muted)">
              Loading graph…
            </div>
          )}
          {diffOverlay && (
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
          {unusedIds && unusedIds.size > 0 && !diffOverlay && (
            <div className="animate-fade-in absolute top-3 right-3 z-10 flex items-center gap-2 rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/90 px-3 py-1.5 font-mono text-[11px] backdrop-blur-sm">
              <span className="size-2 rounded-full bg-(--color-warn)" />
              <span className="text-(--color-warn)">
                {unusedIds.size} unused symbols
              </span>
              <button
                onClick={() => setUnusedIds(null)}
                className="ml-1 text-(--color-text-dim) hover:text-(--color-text-primary)"
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
          data={data}
          onClose={() => setSearchOpen(false)}
          onSelect={(n) => setSelected(n)}
        />
      )}
      {helpOpen && <KeyboardHelp onClose={() => setHelpOpen(false)} />}
    </div>
  );
}
