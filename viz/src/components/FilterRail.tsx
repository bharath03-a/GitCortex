import { useMemo } from "react";
import {
  Boxes,
  CircleSlash,
  Flame,
  Focus,
  Gauge,
  Globe2,
  PanelLeftClose,
  SlidersHorizontal,
  Zap,
} from "lucide-react";
import type { FileHotspot, GraphData } from "../api";
import { DENSITY_LABEL, type DensityMode } from "../graph/density";
import type { ViewMode } from "../graph/view";
import {
  CONFIDENCE_COLOR,
  CONFIDENCE_LABEL,
  EDGE_COLOR,
  KIND_COLOR,
  KIND_LABEL,
} from "../theme/colors";

export type Visibility = "pub" | "pub_crate" | "private";
export type Flag = "async" | "unsafe";
export type InsightLens = "changes" | "impact" | "unused" | "complexity" | "boundaries" | null;

const VIS_LABEL: Record<Visibility, string> = {
  pub: "pub",
  pub_crate: "pub(crate)",
  private: "private",
};

interface Props {
  data: GraphData | null;
  visibleNodeCount: number;
  hotFiles: FileHotspot[] | null;
  density: DensityMode;
  onDensityChange: (mode: DensityMode) => void;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  canInvestigate: boolean;
  insightLens: InsightLens;
  onInsightLensChange: (lens: Exclude<InsightLens, null>) => void;
  hiddenKinds: Set<string>;
  setHiddenKinds: (set: Set<string>) => void;
  hiddenEdgeKinds: Set<string>;
  setHiddenEdgeKinds: (set: Set<string>) => void;
  hiddenConfidence: Set<string>;
  setHiddenConfidence: (set: Set<string>) => void;
  hiddenVisibility: Set<Visibility>;
  setHiddenVisibility: (set: Set<Visibility>) => void;
  flagFilter: Set<Flag>;
  setFlagFilter: (set: Set<Flag>) => void;
  onCollapse: () => void;
}

export function FilterRail({
  data,
  visibleNodeCount,
  hotFiles,
  density,
  onDensityChange,
  viewMode,
  onViewModeChange,
  canInvestigate,
  insightLens,
  onInsightLensChange,
  hiddenKinds,
  setHiddenKinds,
  hiddenEdgeKinds,
  setHiddenEdgeKinds,
  hiddenConfidence,
  setHiddenConfidence,
  hiddenVisibility,
  setHiddenVisibility,
  flagFilter,
  setFlagFilter,
  onCollapse,
}: Props) {
  const { kindCounts, edgeCounts, visCounts, flagCounts, confidenceCounts } = useMemo(() => {
    const kindCounts: Record<string, number> = {};
    const edgeCounts: Record<string, number> = {};
    const visCounts: Record<string, number> = { pub: 0, pub_crate: 0, private: 0 };
    const flagCounts: Record<string, number> = { async: 0, unsafe: 0 };
    const confidenceCounts: Record<string, number> = { extracted: 0, resolved: 0, inferred: 0 };
    if (!data) return { kindCounts, edgeCounts, visCounts, flagCounts, confidenceCounts };
    for (const node of data.nodes) {
      kindCounts[node.kind] = (kindCounts[node.kind] ?? 0) + 1;
      if (visCounts[node.visibility] != null) visCounts[node.visibility] += 1;
      if (node.is_async) flagCounts.async += 1;
      if (node.is_unsafe) flagCounts.unsafe += 1;
    }
    for (const edge of data.edges) {
      edgeCounts[edge.kind] = (edgeCounts[edge.kind] ?? 0) + 1;
      const confidence = edge.confidence ?? "extracted";
      confidenceCounts[confidence] = (confidenceCounts[confidence] ?? 0) + 1;
    }
    return { kindCounts, edgeCounts, visCounts, flagCounts, confidenceCounts };
  }, [data]);

  const toggle = <T extends string>(set: Set<T>, key: T, apply: (next: Set<T>) => void) => {
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
    <aside className="flex w-[288px] shrink-0 flex-col border-r border-(--color-border-subtle) bg-(--color-void-deep)">
      <div className="flex h-14 items-center justify-between border-b border-(--color-border-subtle) px-4">
        <div>
          <div className="text-[12px] font-semibold tracking-[-0.01em]">Graph workspace</div>
          <div className="font-mono text-[9px] text-(--color-text-dim)">
            {visibleNodeCount.toLocaleString()} visible ·{" "}
            {(data?.nodes.length ?? 0).toLocaleString()} loaded
          </div>
        </div>
        <button
          onClick={onCollapse}
          title="Collapse exploration panel"
          aria-label="Collapse exploration panel"
          className="rounded-lg p-1.5 text-(--color-text-muted) hover:bg-(--color-elevated) hover:text-(--color-text-primary)"
        >
          <PanelLeftClose className="size-4" />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-3 py-4">
        <SectionLabel>Workspace</SectionLabel>
        <div className="grid grid-cols-2 gap-2">
          <ModeButton
            active={viewMode === "atlas"}
            icon={<Globe2 className="size-4" />}
            title="Atlas"
            description="Whole repository"
            onClick={() => onViewModeChange("atlas")}
          />
          <ModeButton
            active={viewMode === "investigate"}
            disabled={!canInvestigate}
            icon={<Focus className="size-4" />}
            title="Investigate"
            description={canInvestigate ? "Selected symbol" : "Select a node"}
            onClick={() => onViewModeChange("investigate")}
          />
        </div>

        <SectionLabel className="mt-5">Detail</SectionLabel>
        <div className="grid grid-cols-3 gap-px overflow-hidden rounded-md border border-(--color-border-subtle) bg-(--color-border-subtle)">
          {(["focused", "public", "full"] as DensityMode[]).map((mode) => (
            <button
              key={mode}
              onClick={() => onDensityChange(mode)}
              title={DENSITY_LABEL[mode]}
              className={`rounded-md px-1.5 py-1.5 text-[10px] transition-colors ${
                density === mode
                  ? "bg-(--color-void-deep) font-semibold text-(--color-accent)"
                  : "bg-(--color-elevated) text-(--color-text-muted) hover:bg-(--color-void-deep) hover:text-(--color-text-primary)"
              }`}
            >
              {mode === "public" ? "Public" : DENSITY_LABEL[mode]}
            </button>
          ))}
        </div>

        <SectionLabel className="mt-5">Insights</SectionLabel>
        <div className="space-y-1.5">
          <InsightButton
            active={insightLens === "changes"}
            icon={<Flame className="size-4" />}
            title="Change hotspots"
            description="Frequently changed files and relations"
            tone="hot"
            onClick={() => onInsightLensChange("changes")}
          />
          <InsightButton
            active={insightLens === "impact"}
            icon={<Zap className="size-4" />}
            title="High-impact hubs"
            description="Symbols with high inbound fan-in"
            tone="cyan"
            onClick={() => onInsightLensChange("impact")}
          />
          <InsightButton
            active={insightLens === "complexity"}
            icon={<Gauge className="size-4" />}
            title="Complexity risk"
            description="Code volume × graph connectivity"
            tone="hot"
            onClick={() => onInsightLensChange("complexity")}
          />
          <InsightButton
            active={insightLens === "boundaries"}
            icon={<Boxes className="size-4" />}
            title="Boundary crossings"
            description="Relations crossing package boundaries"
            tone="purple"
            onClick={() => onInsightLensChange("boundaries")}
          />
          <InsightButton
            active={insightLens === "unused"}
            icon={<CircleSlash className="size-4" />}
            title="Unused candidates"
            description="Symbols without incoming usage"
            tone="warn"
            onClick={() => onInsightLensChange("unused")}
          />
        </div>

        {insightLens === "changes" && hotFiles && hotFiles.length > 0 && (
          <div className="mt-3 rounded-md border border-(--color-accent)/20 bg-(--color-accent-soft) p-2">
            <div className="mb-1.5 flex items-center justify-between px-1 font-mono text-[9px] font-semibold tracking-wider text-(--color-accent) uppercase">
              <span>Most changed</span>
              <span>commits</span>
            </div>
            <ol className="space-y-0.5">
              {hotFiles.slice(0, 7).map((file, index) => (
                <li
                  key={file.path}
                  title={`${file.path} · +${file.additions} / −${file.deletions}`}
                  className="flex items-center gap-2 rounded px-1.5 py-1 text-[10px] hover:bg-(--color-void-deep)/70"
                >
                  <span className="w-3 shrink-0 font-mono text-(--color-text-dim)">
                    {index + 1}
                  </span>
                  <span className="flex-1 truncate font-mono text-(--color-text-muted)">
                    {file.path}
                  </span>
                  <span className="shrink-0 font-mono font-semibold text-(--color-accent)">
                    {file.touches}
                  </span>
                </li>
              ))}
            </ol>
          </div>
        )}

        <div className="my-5 h-px bg-(--color-border-subtle)" />
        <div className="mb-2 flex items-center gap-2 text-[10px] font-semibold tracking-[0.12em] text-(--color-text-dim) uppercase">
          <SlidersHorizontal className="size-3" /> Advanced filters
        </div>

        <FilterSection title="Node kinds" summary={`${kinds.length} kinds`}>
          {kinds.map((kind) => (
            <FilterRow
              key={kind}
              checked={!hiddenKinds.has(kind)}
              onChange={() => toggle(hiddenKinds, kind, setHiddenKinds)}
              swatchColor={KIND_COLOR[kind] ?? "#888"}
              label={KIND_LABEL[kind] ?? kind}
              count={kindCounts[kind]}
            />
          ))}
        </FilterSection>

        <FilterSection title="Visibility" summary="Access level">
          {vis.map((visibility) => (
            <FilterRow
              key={visibility}
              checked={!hiddenVisibility.has(visibility)}
              onChange={() => toggle(hiddenVisibility, visibility, setHiddenVisibility)}
              swatchColor={
                visibility === "pub"
                  ? "#2F6F5E"
                  : visibility === "pub_crate"
                    ? "#8A6D28"
                    : "#9A978F"
              }
              label={VIS_LABEL[visibility]}
              count={visCounts[visibility] ?? 0}
            />
          ))}
        </FilterSection>

        <FilterSection title="Flags" summary="Symbol attributes">
          {flags.map((flag) => (
            <FilterRow
              key={flag}
              checked={flagFilter.has(flag)}
              onChange={() => toggle(flagFilter, flag, setFlagFilter)}
              swatchColor={flag === "async" ? "#6B4CA8" : "#C4633F"}
              label={`only ${flag}`}
              count={flagCounts[flag] ?? 0}
            />
          ))}
        </FilterSection>

        <FilterSection title="Confidence" summary="Edge quality">
          {(["extracted", "resolved", "inferred"] as const).map((confidence) => (
            <FilterRow
              key={confidence}
              checked={!hiddenConfidence.has(confidence)}
              onChange={() => toggle(hiddenConfidence, confidence, setHiddenConfidence)}
              swatchColor={CONFIDENCE_COLOR[confidence]}
              swatchKind="bar"
              label={CONFIDENCE_LABEL[confidence]}
              count={confidenceCounts[confidence] ?? 0}
            />
          ))}
        </FilterSection>

        <FilterSection title="Relationships" summary={`${edges.length} kinds`}>
          {edges.map((kind) => (
            <FilterRow
              key={kind}
              checked={!hiddenEdgeKinds.has(kind)}
              onChange={() => toggle(hiddenEdgeKinds, kind, setHiddenEdgeKinds)}
              swatchColor={EDGE_COLOR[kind] ?? "#666"}
              swatchKind="bar"
              label={kind}
              count={edgeCounts[kind]}
            />
          ))}
        </FilterSection>
      </div>
    </aside>
  );
}

function SectionLabel({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`mb-2 text-[9px] font-semibold tracking-[0.15em] text-(--color-text-dim) uppercase ${className}`}
    >
      {children}
    </div>
  );
}

function ModeButton({
  active,
  disabled = false,
  icon,
  title,
  description,
  onClick,
}: {
  active: boolean;
  disabled?: boolean;
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className={`rounded-md border p-2.5 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
        active
          ? "border-(--color-accent) bg-(--color-accent-soft)"
          : "border-(--color-border-subtle) bg-(--color-void-deep) hover:border-(--color-border-strong) hover:bg-(--color-elevated)"
      }`}
    >
      <span className={active ? "text-(--color-accent)" : "text-(--color-text-muted)"}>{icon}</span>
      <span className="mt-2 block text-[11px] font-medium">{title}</span>
      <span className="block truncate text-[9px] text-(--color-text-dim)">{description}</span>
    </button>
  );
}

function InsightButton({
  active,
  icon,
  title,
  description,
  tone,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  title: string;
  description: string;
  tone: "hot" | "cyan" | "warn" | "purple";
  onClick: () => void;
}) {
  const activeTone =
    tone === "hot"
      ? "border-(--color-accent)/30 bg-(--color-accent-soft) text-(--color-accent)"
      : tone === "cyan"
        ? "border-[#167A86]/30 bg-[#167A86]/8 text-[#167A86]"
        : tone === "purple"
          ? "border-[#6B4CA8]/30 bg-[#6B4CA8]/8 text-[#6B4CA8]"
          : "border-[#8A6D28]/30 bg-[#8A6D28]/8 text-[#8A6D28]";
  return (
    <button
      onClick={onClick}
      className={`flex w-full items-center gap-2.5 rounded-md border px-2.5 py-2 text-left transition-colors ${
        active
          ? activeTone
          : "border-(--color-border-subtle) bg-(--color-void-deep) text-(--color-text-muted) hover:border-(--color-border-strong) hover:bg-(--color-elevated) hover:text-(--color-text-primary)"
      }`}
    >
      <span className="shrink-0">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[11px] font-medium">{title}</span>
        <span className="block truncate text-[9px] opacity-65">{description}</span>
      </span>
      <span
        className={`size-1.5 rounded-full ${active ? "bg-current" : "bg-(--color-border-strong)"}`}
      />
    </button>
  );
}

function FilterSection({
  title,
  summary,
  children,
}: {
  title: string;
  summary: string;
  children: React.ReactNode;
}) {
  return (
    <details className="group border-b border-(--color-border-subtle)/70 py-1.5">
      <summary className="flex cursor-pointer list-none items-center justify-between rounded-md px-1 py-1.5 text-[11px] text-(--color-text-muted) hover:text-(--color-text-primary)">
        <span>{title}</span>
        <span className="font-mono text-[9px] text-(--color-text-dim)">{summary}</span>
      </summary>
      <div className="space-y-0.5 pb-2 pt-1">{children}</div>
    </details>
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
    <label className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 hover:bg-(--color-elevated)">
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
        className="size-3 accent-(--color-accent)"
      />
      <span
        className={swatchKind === "dot" ? "size-2 rounded-full" : "h-0.5 w-3 rounded"}
        style={{ background: swatchColor }}
      />
      <span className="flex-1 truncate text-[10px] capitalize text-(--color-text-muted)">
        {label}
      </span>
      <span className="font-mono text-[9px] text-(--color-text-dim)">{count}</span>
    </label>
  );
}
