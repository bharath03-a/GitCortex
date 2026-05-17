import type { GraphData, RawNode } from "../api";
import type { DiffOverlay } from "../hooks/useBranchDiff";

interface Props {
  data: GraphData | null;
  selected: RawNode | null;
  activeBranch: string | null;
  lastSha: string | null;
  diffOverlay: DiffOverlay | null;
}

export function StatusBar({
  data,
  selected,
  activeBranch,
  lastSha,
  diffOverlay,
}: Props) {
  return (
    <footer className="flex h-7 items-center justify-between border-t border-(--color-border-subtle) bg-(--color-void-deep) px-3 font-mono text-[11px] text-(--color-text-muted)">
      <div className="flex items-center gap-3">
        <span>
          <span className="text-(--color-text-dim)">nodes</span>{" "}
          {data?.nodes.length ?? "—"}
        </span>
        <span>
          <span className="text-(--color-text-dim)">edges</span>{" "}
          {data?.edges.length ?? "—"}
        </span>
        {activeBranch && (
          <span>
            <span className="text-(--color-text-dim)">branch</span>{" "}
            {activeBranch}
          </span>
        )}
        {lastSha && (
          <span>
            <span className="text-(--color-text-dim)">sha</span>{" "}
            {lastSha.slice(0, 7)}
          </span>
        )}
        {diffOverlay && (
          <span className="text-(--color-accent)">
            ↔ {diffOverlay.head} · +{diffOverlay.addedIds.size} / −
            {diffOverlay.removedIds.size}
          </span>
        )}
      </div>
      <div className="flex items-center gap-3">
        {selected ? (
          <span className="text-(--color-accent)">
            {selected.kind}: {selected.name}
          </span>
        ) : (
          <span className="text-(--color-text-dim)">no selection</span>
        )}
        <span className="text-(--color-text-dim)">cosmograph · GPGPU</span>
      </div>
    </footer>
  );
}
