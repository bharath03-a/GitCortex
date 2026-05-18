import { useMemo } from "react";
import { PanelLeftClose } from "lucide-react";
import type { GraphData } from "../api";
import { EDGE_COLOR, KIND_COLOR, KIND_LABEL } from "../theme/colors";

export type Visibility = "pub" | "pub_crate" | "private";
export type Flag = "async" | "unsafe";

const VIS_LABEL: Record<Visibility, string> = {
  pub: "pub",
  pub_crate: "pub(crate)",
  private: "private",
};

interface Props {
  data: GraphData | null;
  hiddenKinds: Set<string>;
  setHiddenKinds: (s: Set<string>) => void;
  hiddenEdgeKinds: Set<string>;
  setHiddenEdgeKinds: (s: Set<string>) => void;
  hiddenVisibility: Set<Visibility>;
  setHiddenVisibility: (s: Set<Visibility>) => void;
  flagFilter: Set<Flag>;
  setFlagFilter: (s: Set<Flag>) => void;
  onCollapse: () => void;
}

export function FilterRail({
  data,
  hiddenKinds,
  setHiddenKinds,
  hiddenEdgeKinds,
  setHiddenEdgeKinds,
  hiddenVisibility,
  setHiddenVisibility,
  flagFilter,
  setFlagFilter,
  onCollapse,
}: Props) {
  const { kindCounts, edgeCounts, visCounts, flagCounts } = useMemo(() => {
    const kindCounts: Record<string, number> = {};
    const edgeCounts: Record<string, number> = {};
    const visCounts: Record<string, number> = { pub: 0, pub_crate: 0, private: 0 };
    const flagCounts: Record<string, number> = { async: 0, unsafe: 0 };
    if (!data) return { kindCounts, edgeCounts, visCounts, flagCounts };
    for (const n of data.nodes) {
      kindCounts[n.kind] = (kindCounts[n.kind] ?? 0) + 1;
      if (visCounts[n.visibility] != null) visCounts[n.visibility] += 1;
      if (n.is_async) flagCounts.async += 1;
      if (n.is_unsafe) flagCounts.unsafe += 1;
    }
    for (const e of data.edges) {
      edgeCounts[e.kind] = (edgeCounts[e.kind] ?? 0) + 1;
    }
    return { kindCounts, edgeCounts, visCounts, flagCounts };
  }, [data]);

  const toggle = <T extends string>(set: Set<T>, key: T, apply: (s: Set<T>) => void) => {
    const next = new Set(set);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    apply(next);
  };

  const kinds = Object.keys(kindCounts).sort();
  const edges = Object.keys(edgeCounts).sort();
  const vis: Visibility[] = ["pub", "pub_crate", "private"];
  const flags: Flag[] = ["async", "unsafe"];

  return (
    <aside className="flex w-[280px] flex-col border-r border-(--color-border-subtle) bg-(--color-void-deep)">
      <div className="flex h-10 items-center justify-between border-b border-(--color-border-subtle) px-3">
        <span className="font-medium text-(--color-text-primary)">Filters</span>
        <button
          onClick={onCollapse}
          title="Collapse panel ([ )"
          className="rounded-md p-1 text-(--color-text-muted) hover:text-(--color-text-primary)"
        >
          <PanelLeftClose className="size-4" />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-3">
        <FilterSection title="Node Kinds">
          {kinds.map((k) => {
            const hidden = hiddenKinds.has(k);
            const color = KIND_COLOR[k] ?? "#888";
            return (
              <FilterRow
                key={k}
                checked={!hidden}
                onChange={() => toggle(hiddenKinds, k, setHiddenKinds)}
                swatchColor={color}
                label={KIND_LABEL[k] ?? k}
                count={kindCounts[k]}
              />
            );
          })}
        </FilterSection>

        <FilterSection title="Visibility">
          {vis.map((v) => (
            <FilterRow
              key={v}
              checked={!hiddenVisibility.has(v)}
              onChange={() => toggle(hiddenVisibility, v, setHiddenVisibility)}
              swatchColor={v === "pub" ? "#a6e3a1" : v === "pub_crate" ? "#f9e2af" : "#6c7086"}
              label={VIS_LABEL[v]}
              count={visCounts[v] ?? 0}
            />
          ))}
        </FilterSection>

        <FilterSection title="Flags">
          {flags.map((f) => (
            <FilterRow
              key={f}
              checked={flagFilter.has(f)}
              onChange={() => toggle(flagFilter, f, setFlagFilter)}
              swatchColor={f === "async" ? "#a78bfa" : "#fab387"}
              label={`only ${f}`}
              count={flagCounts[f] ?? 0}
            />
          ))}
        </FilterSection>

        <FilterSection title="Edge Kinds">
          {edges.map((k) => {
            const hidden = hiddenEdgeKinds.has(k);
            const color = EDGE_COLOR[k] ?? "#666";
            return (
              <FilterRow
                key={k}
                checked={!hidden}
                onChange={() => toggle(hiddenEdgeKinds, k, setHiddenEdgeKinds)}
                swatchColor={color}
                swatchKind="bar"
                label={k}
                count={edgeCounts[k]}
              />
            );
          })}
        </FilterSection>
      </div>
    </aside>
  );
}

function FilterSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-5">
      <h3 className="mb-2 text-[10px] font-semibold tracking-widest text-(--color-text-dim) uppercase">
        {title}
      </h3>
      <div className="space-y-1">{children}</div>
    </section>
  );
}

function FilterRow({
  checked,
  onChange,
  swatchColor,
  swatchKind = "dot",
  label,
  count,
}: {
  checked: boolean;
  onChange: () => void;
  swatchColor: string;
  swatchKind?: "dot" | "bar";
  label: string;
  count: number;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-2 rounded px-2 py-1 hover:bg-(--color-elevated)">
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
        className="accent-(--color-accent)"
      />
      <span
        className={swatchKind === "dot" ? "size-2.5 rounded-full" : "h-0.5 w-3 rounded"}
        style={{ background: swatchColor }}
      />
      <span className="flex-1 text-[12px] capitalize">{label}</span>
      <span className="font-mono text-[10px] text-(--color-text-dim)">{count}</span>
    </label>
  );
}
