import { Boxes, Cpu, Search } from "lucide-react";
import { useMemo } from "react";
import type { GraphData, RawNode } from "../api";
import { summarizeGroups, type GroupSummary } from "../graph/compatibility";
import { KIND_LABEL } from "../theme/colors";

interface Props {
  data: GraphData;
  selected: RawNode | null;
  onSelect: (node: RawNode | null) => void;
  onSearch: () => void;
  onTryWebGl: () => void;
}

export function CompatibilityAtlas({ data, selected, onSelect, onSearch, onTryWebGl }: Props) {
  const groups = useMemo(() => summarizeGroups(data), [data]);
  const shownGroups = groups.slice(0, 120);

  return (
    <div className="absolute inset-0 overflow-auto p-5 lg:p-7">
      <div className="mx-auto max-w-[1280px]">
        <div className="graph-panel mb-5 flex flex-wrap items-center justify-between gap-4 rounded-lg p-4">
          <div className="flex items-start gap-3">
            <div className="rounded-md bg-(--color-accent-soft) p-2 text-(--color-accent)">
              <Cpu className="size-4" />
            </div>
            <div>
              <div className="text-[13px] font-semibold">Compatibility atlas</div>
              <p className="mt-0.5 text-[11px] text-(--color-text-muted)">
                WebGL acceleration is unavailable. Repository groups and exact symbol access remain
                active without running a large CPU force simulation.
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={onSearch}
              className="flex items-center gap-2 rounded-md border border-(--color-border-subtle) bg-(--color-elevated) px-3 py-2 text-[11px] text-(--color-text-muted) hover:border-(--color-border-strong) hover:text-(--color-text-primary)"
            >
              <Search className="size-3.5" /> Search all symbols
            </button>
            <button
              onClick={onTryWebGl}
              className="rounded-md border border-(--color-border-subtle) px-3 py-2 font-mono text-[10px] text-(--color-text-dim) hover:text-(--color-accent)"
            >
              retry WebGL
            </button>
          </div>
        </div>

        <div className="mb-3 flex items-end justify-between gap-4">
          <div>
            <div className="font-mono text-[9px] font-semibold tracking-[0.14em] text-(--color-text-dim) uppercase">
              Repository structure
            </div>
            <div className="mt-1 text-[15px] font-semibold">
              {groups.length.toLocaleString()} architecture groups
            </div>
          </div>
          <div className="font-mono text-[9px] text-(--color-text-dim)">
            {data.nodes.length.toLocaleString()} symbols · {data.edges.length.toLocaleString()}{" "}
            relations
          </div>
        </div>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {shownGroups.map((group) => (
            <GroupCard key={group.name} group={group} selected={selected} onSelect={onSelect} />
          ))}
        </div>
        {groups.length > shownGroups.length && (
          <div className="mt-4 rounded-md border border-(--color-border-subtle) p-3 text-center font-mono text-[10px] text-(--color-text-dim)">
            Showing the 120 largest groups. Search covers all {groups.length.toLocaleString()}{" "}
            groups.
          </div>
        )}
      </div>
    </div>
  );
}

function GroupCard({
  group,
  selected,
  onSelect,
}: {
  group: GroupSummary;
  selected: RawNode | null;
  onSelect: (node: RawNode) => void;
}) {
  const topKinds = [...group.kinds.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3);
  const topSymbols = group.nodes.slice(0, 4);
  const selectedHere = selected ? group.nodes.some((node) => node.id === selected.id) : false;

  return (
    <section
      className={`rounded-lg border bg-(--color-void-deep)/90 p-4 transition-colors ${
        selectedHere
          ? "border-(--color-accent) shadow-[inset_3px_0_0_var(--color-accent)]"
          : "border-(--color-border-subtle) hover:border-(--color-border-strong)"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Boxes className="size-4 shrink-0 text-(--color-accent)" />
          <h3 className="truncate font-mono text-[11px] font-semibold">{group.name}</h3>
        </div>
        <span className="shrink-0 rounded-full bg-(--color-elevated) px-2 py-1 font-mono text-[8px] text-(--color-text-dim)">
          {group.nodes.length.toLocaleString()} symbols
        </span>
      </div>
      <div className="mt-3 grid grid-cols-3 gap-2 border-y border-(--color-border-subtle) py-2.5 font-mono text-[9px]">
        <Metric label="relations" value={group.relations} />
        <Metric label="boundary" value={group.boundaryRelations} />
        <Metric label="LOC" value={group.loc} />
      </div>
      <div className="mt-3 flex min-h-5 flex-wrap gap-1.5">
        {topKinds.map(([kind, count]) => (
          <span
            key={kind}
            className="rounded border border-(--color-border-subtle) bg-(--color-elevated) px-1.5 py-0.5 font-mono text-[8px] text-(--color-text-muted)"
          >
            {KIND_LABEL[kind] ?? kind} {count}
          </span>
        ))}
      </div>
      <div className="mt-3 space-y-1">
        {topSymbols.map((node) => (
          <button
            key={node.id}
            onClick={() => onSelect(node)}
            className={`flex w-full items-center justify-between gap-3 rounded px-2 py-1.5 text-left font-mono text-[9px] hover:bg-(--color-elevated) ${
              selected?.id === node.id ? "bg-(--color-accent-soft) text-(--color-accent)" : ""
            }`}
          >
            <span className="truncate">{node.name}</span>
            <span className="shrink-0 text-[8px] text-(--color-text-dim)">{node.kind}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="text-(--color-text-primary)">{value.toLocaleString()}</div>
      <div className="mt-0.5 text-[8px] text-(--color-text-dim)">{label}</div>
    </div>
  );
}
