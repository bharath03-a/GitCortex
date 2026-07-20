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
    <footer className="flex h-8 items-center justify-between border-t border-(--color-border-subtle) bg-(--color-void-deep) px-4 text-[10px] text-(--color-text-muted)">
      <div className="flex min-w-0 items-center gap-2.5">
        <span>{(data?.nodes.length ?? 0).toLocaleString()} symbols</span>
        <span className="text-(--color-border-strong)">·</span>
        <span>{(data?.edges.length ?? 0).toLocaleString()} relationships</span>
        {activeBranch && (
          <>
            <span className="text-(--color-border-strong)">·</span>
            <span className="max-w-[240px] truncate font-mono text-(--color-text-dim)">
              {activeBranch}
              {lastSha ? ` @ ${lastSha.slice(0, 7)}` : ""}
            </span>
          </>
        )}
        {diffOverlay && (
          <span className="rounded bg-(--color-accent-soft) px-1.5 py-0.5 text-(--color-accent)">
            +{diffOverlay.addedIds.size} / −{diffOverlay.removedIds.size} vs {diffOverlay.head}
          </span>
        )}
      </div>
      <div className="min-w-0 pl-4">
        {selected ? (
          <span className="block max-w-[420px] truncate font-mono text-(--color-text-primary)">
            {selected.qualified_name || selected.name}
          </span>
        ) : (
          <span className="text-(--color-text-dim)">Select a symbol to inspect it</span>
        )}
      </div>
    </footer>
  );
}
