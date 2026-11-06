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
    <footer className="flex min-h-9 flex-wrap gap-2 py-2 shrink-0 items-center justify-between border-t border-(--color-border-subtle) bg-(--color-void-deep) px-4 text-ui-xs text-(--color-text-muted)">
      <div className="flex min-w-0 items-center gap-3">
        <span className="flex items-center gap-1.5">
          <Network className="size-3 text-(--color-text-dim)" />
          <strong className="font-heading text-(--color-text-primary)">
            {(data?.nodes.length ?? 0).toLocaleString()}
          </strong>
          symbols
        </span>
        <span className="flex items-center gap-1.5">
          <Waypoints className="size-3 text-(--color-text-dim)" />
          <strong className="font-heading text-(--color-text-primary)">
            {(data?.edges.length ?? 0).toLocaleString()}
          </strong>
          relations
        </span>
        {activeBranch && (
          <span className="flex min-w-0 items-center gap-1.5 border-l border-(--color-border-subtle) pl-3">
            <GitBranch className="size-3 text-(--color-text-dim)" />
            <span
              title={`${activeBranch}${lastSha ? ` @ ${lastSha}` : ""}`}
              className="max-w-[240px] truncate font-mono"
            >
              {activeBranch}
              {lastSha ? ` @ ${lastSha.slice(0, 7)}` : ""}
            </span>
          </span>
        )}
        {diffOverlay && (
          <span className="rounded bg-(--color-accent-soft) px-1.5 py-0.5 text-(--color-accent-text)">
            +{diffOverlay.addedIds.size} / −{diffOverlay.removedIds.size} vs {diffOverlay.head}
          </span>
        )}
      </div>
      <div className="min-w-0 pl-4">
        {selected ? (
          <span
            title={selected.qualified_name || selected.name}
            className="font-mono block max-w-[460px] truncate font-heading text-(--color-text-primary)"
          >
            {selected.qualified_name || selected.name}
          </span>
        ) : (
          <span className="text-(--color-text-dim)">Click a node or press / to investigate</span>
        )}
      </div>
    </footer>
  );
}
