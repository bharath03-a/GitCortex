import { GitBranch, Network, Waypoints } from "lucide-react";
import type { GraphData, RawNode } from "../api";
import type { DiffOverlay } from "../hooks/useBranchDiff";

interface Props {
  data: GraphData | null;
  selected: RawNode | null;
  activeBranch: string | null;
  lastSha: string | null;
  diffOverlay: DiffOverlay | null;
}

export function StatusBar({ data, selected, activeBranch, lastSha, diffOverlay }: Props) {
  return (
    <footer className="flex h-9 shrink-0 items-center justify-between border-t border-(--color-border-subtle) bg-(--color-void-deep) px-4 font-mono text-[9px] text-(--color-text-muted)">
      <div className="flex min-w-0 items-center gap-3">
        <span className="flex items-center gap-1.5">
          <Network className="size-3 text-(--color-text-dim)" />
          <strong className="font-semibold text-(--color-text-primary)">
            {(data?.nodes.length ?? 0).toLocaleString()}
          </strong>
          symbols
        </span>
        <span className="flex items-center gap-1.5">
          <Waypoints className="size-3 text-(--color-text-dim)" />
          <strong className="font-semibold text-(--color-text-primary)">
            {(data?.edges.length ?? 0).toLocaleString()}
          </strong>
          relations
        </span>
        {activeBranch && (
          <span className="flex min-w-0 items-center gap-1.5 border-l border-(--color-border-subtle) pl-3">
            <GitBranch className="size-3 text-(--color-text-dim)" />
            <span className="max-w-[240px] truncate">
              {activeBranch}
              {lastSha ? ` @ ${lastSha.slice(0, 7)}` : ""}
            </span>
          </span>
        )}
        {diffOverlay && (
          <span className="rounded bg-(--color-accent-soft) px-1.5 py-0.5 text-(--color-accent)">
            +{diffOverlay.addedIds.size} / −{diffOverlay.removedIds.size} vs {diffOverlay.head}
          </span>
        )}
      </div>
      <div className="min-w-0 pl-4">
        {selected ? (
          <span className="block max-w-[460px] truncate font-semibold text-(--color-text-primary)">
            {selected.qualified_name || selected.name}
          </span>
        ) : (
          <span className="text-(--color-text-dim)">Click a node or press / to investigate</span>
        )}
      </div>
    </footer>
  );
}
