import { CircleSlash, Keyboard, Network, Search } from "lucide-react";
import { DENSITY_LABEL, type DensityMode } from "../graph/density";
import { BranchPicker } from "./BranchPicker";

interface Props {
  nodeCount: number;
  totalNodeCount: number;
  density: DensityMode;
  onDensityChange: (m: DensityMode) => void;
  onSearch: () => void;
  onShowHelp: () => void;
  activeBranch: string | null;
  diffHead: string | null;
  onSetDiffHead: (b: string | null) => void;
  unusedActive: boolean;
  onToggleUnused: () => void;
}

const DENSITY_OPTIONS: DensityMode[] = ["focused", "public", "full"];

export function Header({
  nodeCount,
  totalNodeCount,
  density,
  onDensityChange,
  onSearch,
  onShowHelp,
  activeBranch,
  diffHead,
  onSetDiffHead,
  unusedActive,
  onToggleUnused,
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
          diffHead={diffHead}
          onSetDiffHead={onSetDiffHead}
        />
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
          <kbd className="rounded bg-(--color-void) px-1.5 py-0.5 font-mono text-[10px]">
            ⌘K
          </kbd>
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
