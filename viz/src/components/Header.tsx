import { CircleSlash, Keyboard, Network, Search, Zap } from "lucide-react";
import type { ViewMode } from "../App";
import { DENSITY_LABEL, type DensityMode } from "../graph/density";
import { BranchPicker } from "./BranchPicker";

interface Props {
  nodeCount: number;
  totalNodeCount: number;
  density: DensityMode;
  onDensityChange: (m: DensityMode) => void;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  canInvestigate: boolean;
  onSearch: () => void;
  onShowHelp: () => void;
  activeBranch: string | null;
  onSetActiveBranch: (branch: string) => void;
  diffHead: string | null;
  onSetDiffHead: (b: string | null) => void;
  unusedActive: boolean;
  onToggleUnused: () => void;
  godNodesActive: boolean;
  onToggleGodNodes: () => void;
}

const DENSITY_OPTIONS: DensityMode[] = ["focused", "public", "full"];

export function Header({
  nodeCount,
  totalNodeCount,
  density,
  onDensityChange,
  viewMode,
  onViewModeChange,
  canInvestigate,
  onSearch,
  onShowHelp,
  activeBranch,
  onSetActiveBranch,
  diffHead,
  onSetDiffHead,
  unusedActive,
  onToggleUnused,
  godNodesActive,
  onToggleGodNodes,
}: Props) {
  return (
    <header className="flex h-12 items-center justify-between border-b border-(--color-border-subtle) bg-(--color-void-deep) px-4">
      <div className="flex items-center gap-2">
        <Network className="size-4 text-(--color-accent)" />
        <span className="font-medium tracking-tight">GitCortex</span>
        <span className="ml-2 rounded bg-(--color-elevated) px-2 py-0.5 font-mono text-[11px] text-(--color-text-muted)">
          {nodeCount} / {totalNodeCount} nodes
        </span>
      </div>
      <div className="flex items-center gap-2">
        <BranchPicker
          active={activeBranch}
          onSetActive={onSetActiveBranch}
          diffHead={diffHead}
          onSetDiffHead={onSetDiffHead}
        />
        <div className="flex items-center gap-1 rounded-md border border-(--color-border-subtle) bg-(--color-elevated) p-0.5">
          <button
            onClick={() => onViewModeChange("atlas")}
            className={`rounded px-2 py-1 text-[11px] transition-colors ${
              viewMode === "atlas"
                ? "bg-(--color-accent-soft) text-(--color-accent)"
                : "text-(--color-text-muted) hover:text-(--color-text-primary)"
            }`}
          >
            Atlas
          </button>
          <button
            disabled={!canInvestigate}
            title={canInvestigate ? "Focus on selected symbol" : "Select a symbol first"}
            onClick={() => onViewModeChange("investigate")}
            className={`rounded px-2 py-1 text-[11px] transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
              viewMode === "investigate"
                ? "bg-(--color-accent-soft) text-(--color-accent)"
                : "text-(--color-text-muted) hover:text-(--color-text-primary)"
            }`}
          >
            Investigate
          </button>
        </div>
        <button
          onClick={onToggleUnused}
          title="Toggle dead-code overlay (U)"
          className={`flex items-center gap-1.5 rounded-md border border-(--color-border-subtle) px-2.5 py-1.5 text-[11px] transition-colors ${
            unusedActive
              ? "bg-(--color-warn)/15 text-(--color-warn)"
              : "bg-(--color-elevated) text-(--color-text-muted) hover:text-(--color-text-primary)"
          }`}
        >
          <CircleSlash className="size-3.5" />
          <span>Unused</span>
        </button>
        <button
          onClick={onToggleGodNodes}
          title="Toggle hub-node overlay — high fan-in symbols (G)"
          className={`flex items-center gap-1.5 rounded-md border border-(--color-border-subtle) px-2.5 py-1.5 text-[11px] transition-colors ${
            godNodesActive
              ? "bg-cyan-500/15 text-cyan-400"
              : "bg-(--color-elevated) text-(--color-text-muted) hover:text-(--color-text-primary)"
          }`}
        >
          <Zap className="size-3.5" />
          <span>Hubs</span>
        </button>
        <div className="flex items-center gap-1 rounded-md border border-(--color-border-subtle) bg-(--color-elevated) p-0.5">
          {DENSITY_OPTIONS.map((m, i) => {
            const active = density === m;
            return (
              <button
                key={m}
                onClick={() => onDensityChange(m)}
                title={`Density: ${DENSITY_LABEL[m]} (${i + 1})`}
                className={`rounded px-2 py-1 text-[11px] transition-colors ${
                  active
                    ? "bg-(--color-accent-soft) text-(--color-accent)"
                    : "text-(--color-text-muted) hover:text-(--color-text-primary)"
                }`}
              >
                {DENSITY_LABEL[m]}
              </button>
            );
          })}
        </div>
        <button
          onClick={onSearch}
          className="flex items-center gap-2 rounded-md border border-(--color-border-subtle) bg-(--color-elevated) px-3 py-1.5 text-(--color-text-muted) hover:text-(--color-text-primary)"
        >
          <Search className="size-3.5" />
          <span className="text-[12px]">Search</span>
          <kbd className="rounded bg-(--color-void) px-1.5 py-0.5 font-mono text-[10px]">⌘K</kbd>
        </button>
        <button
          onClick={onShowHelp}
          title="Keyboard shortcuts (?)"
          className="rounded-md p-1.5 text-(--color-text-muted) hover:text-(--color-text-primary)"
        >
          <Keyboard className="size-4" />
        </button>
      </div>
    </header>
  );
}
