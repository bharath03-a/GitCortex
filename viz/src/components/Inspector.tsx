import { useEffect, useMemo, useState } from "react";
import { ChevronRight, ExternalLink, X } from "lucide-react";
import type { DeepCallersResult, FileHotspot, GraphData, RawNode } from "../api";
import { fetchDeepCallers } from "../api";
import { KIND_COLOR, KIND_LABEL } from "../theme/colors";

type Tab = "local" | "deep";

interface Props {
  node: RawNode;
  data: GraphData | null;
  onClose: () => void;
  onSelect: (n: RawNode) => void;
  depth: number;
  onDepthChange: (d: number) => void;
  branch: string;
  hotspot: FileHotspot | null;
}

const RISK_TONE: Record<string, string> = {
  LOW: "text-(--color-good) bg-emerald-500/15",
  MEDIUM: "text-(--color-warn) bg-amber-500/15",
  HIGH: "text-(--color-warn) bg-amber-500/20",
  CRITICAL: "text-(--color-bad) bg-red-500/20",
};

export function Inspector({
  node,
  data,
  onClose,
  onSelect,
  depth,
  onDepthChange,
  branch,
  hotspot,
}: Props) {
  const [tab, setTab] = useState<Tab>("local");
  const { callers, callees, uses } = useMemo(() => {
    const callers: RawNode[] = [];
    const callees: RawNode[] = [];
    const uses: RawNode[] = [];
    if (!data) return { callers, callees, uses };
    const byId = new Map(data.nodes.map((n) => [n.id, n]));
    for (const e of data.edges) {
      if (e.kind === "calls") {
        if (e.dst === node.id) {
          const n = byId.get(e.src);
          if (n) callers.push(n);
        } else if (e.src === node.id) {
          const n = byId.get(e.dst);
          if (n) callees.push(n);
        }
      } else if (e.kind === "uses" && e.src === node.id) {
        const n = byId.get(e.dst);
        if (n) uses.push(n);
      }
    }
    return { callers, callees, uses };
  }, [node, data]);

  const color = KIND_COLOR[node.kind] ?? "#89b4fa";
  const fileRel = node.file.split("/").slice(-4).join("/");

  return (
    <aside className="animate-fade-in flex w-[360px] flex-col border-l border-(--color-border-subtle) bg-(--color-void-deep)">
      <div className="flex items-start justify-between border-b border-(--color-border-subtle) p-4">
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2">
            <span className="size-2.5 rounded-full" style={{ background: color }} />
            <span className="text-[10px] tracking-widest text-(--color-text-dim) uppercase">
              {KIND_LABEL[node.kind] ?? node.kind}
            </span>
          </div>
          <h2 className="truncate font-mono text-[14px] font-medium">{node.name}</h2>
          {node.qualified_name && node.qualified_name !== node.name && (
            <div className="mt-1 truncate font-mono text-[11px] text-(--color-text-muted)">
              {node.qualified_name}
            </div>
          )}
        </div>
        <button
          onClick={onClose}
          className="rounded-md p-1 text-(--color-text-muted) hover:text-(--color-text-primary)"
        >
          <X className="size-4" />
        </button>
      </div>

      <div className="border-b border-(--color-border-subtle) px-4 py-3">
        <a
          href={`vscode://file/${encodeURI(node.file)}:${node.start_line}`}
          title="Open in VS Code / Cursor"
          className="mb-2 flex items-center gap-1 font-mono text-[11px] text-(--color-text-muted) hover:text-(--color-accent)"
        >
          <span className="truncate">
            {fileRel}:{node.start_line}–{node.end_line}
          </span>
          <ExternalLink className="size-3 shrink-0" />
        </a>
        <div className="flex flex-wrap gap-1.5">
          <Badge>{node.visibility}</Badge>
          <Badge>{node.loc} LOC</Badge>
          {node.is_async && <Badge tone="accent">async</Badge>}
          {node.is_unsafe && <Badge tone="warn">unsafe</Badge>}
          {hotspot && (
            <Badge tone="hot">
              {hotspot.touches} changes · +{hotspot.additions}/−{hotspot.deletions}
            </Badge>
          )}
        </div>
        <div className="mt-3 flex items-center gap-2">
          <span
            id="neighborhood-label"
            className="text-[10px] tracking-widest text-(--color-text-dim) uppercase"
          >
            neighborhood
          </span>
          <div role="radiogroup" aria-labelledby="neighborhood-label" className="flex gap-2">
            {[1, 2, 3].map((d) => (
              <button
                key={d}
                role="radio"
                aria-checked={depth === d}
                onClick={() => onDepthChange(d)}
                className={`rounded px-1.5 py-0.5 font-mono text-[10px] transition-colors ${
                  depth === d
                    ? "bg-(--color-accent-soft) text-(--color-accent)"
                    : "text-(--color-text-muted) hover:text-(--color-text-primary)"
                }`}
              >
                {d}-hop
              </button>
            ))}
          </div>
        </div>
      </div>

      <div
        role="tablist"
        aria-label="Inspector tabs"
        className="flex border-b border-(--color-border-subtle) px-2"
      >
        <TabBtn
          id="tab-local"
          panelId="inspector-panel"
          active={tab === "local"}
          onClick={() => setTab("local")}
        >
          Local
        </TabBtn>
        <TabBtn
          id="tab-deep"
          panelId="inspector-panel"
          active={tab === "deep"}
          onClick={() => setTab("deep")}
        >
          Deep Callers
        </TabBtn>
      </div>

      <div
        id="inspector-panel"
        role="tabpanel"
        aria-labelledby={tab === "local" ? "tab-local" : "tab-deep"}
        className="flex-1 overflow-y-auto p-3"
      >
        {tab === "local" && (
          <>
            <NodeList title="Callers" nodes={callers} onSelect={onSelect} />
            <NodeList title="Callees" nodes={callees} onSelect={onSelect} />
            <NodeList title="Uses" nodes={uses} onSelect={onSelect} />
            {callers.length === 0 && callees.length === 0 && uses.length === 0 && <EmptyHint />}
          </>
        )}
        {tab === "deep" && <DeepCallersPanel node={node} branch={branch} onSelect={onSelect} />}
      </div>
    </aside>
  );
}

function DeepCallersPanel({
  node,
  branch,
  onSelect,
}: {
  node: RawNode;
  branch: string;
  onSelect: (n: RawNode) => void;
}) {
  const [result, setResult] = useState<DeepCallersResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const depth = 3;

  useEffect(() => {
    setLoading(true);
    setError(null);
    setResult(null);
    fetchDeepCallers(node.id, branch, depth)
      .then(setResult)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [node.id, branch, depth]);

  if (loading)
    return (
      <div className="px-2 py-4 text-[12px] text-(--color-text-muted)">
        Tracing callers up to depth {depth}…
      </div>
    );
  if (error) return <div className="px-2 py-4 text-[12px] text-(--color-bad)">{error}</div>;
  if (!result) return null;

  const total = result.hops.reduce((acc, h) => acc + h.nodes.length, 0);
  const tone = RISK_TONE[result.risk_level] ?? "text-(--color-text-muted)";

  return (
    <div>
      <div className="mb-3 flex items-center justify-between">
        <div className="text-[10px] tracking-widest text-(--color-text-dim) uppercase">
          {total} affected · depth {result.depth}
        </div>
        <span className={`rounded px-2 py-0.5 font-mono text-[10px] ${tone}`}>
          {result.risk_level}
        </span>
      </div>
      {result.hops.map((h) => (
        <NodeList key={h.hop} title={`Hop ${h.hop}`} nodes={h.nodes} onSelect={onSelect} />
      ))}
      {result.truncated && (
        <div className="rounded bg-amber-500/10 px-2 py-1.5 text-[11px] text-(--color-warn)">
          Showing the first 500 affected symbols.
        </div>
      )}
      {total === 0 && <EmptyHint label="No callers found" />}
    </div>
  );
}

function EmptyHint({ label = "No connections in current view" }: { label?: string }) {
  return <div className="px-2 py-6 text-center text-[12px] text-(--color-text-dim)">{label}</div>;
}

function TabBtn({
  children,
  active,
  onClick,
  id,
  panelId,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
  id: string;
  panelId: string;
}) {
  return (
    <button
      role="tab"
      id={id}
      aria-selected={active}
      aria-controls={panelId}
      onClick={onClick}
      className={`px-3 py-2 text-[12px] transition-colors ${
        active
          ? "border-b-2 border-(--color-accent) text-(--color-text-primary)"
          : "text-(--color-text-muted) hover:text-(--color-text-primary)"
      }`}
    >
      {children}
    </button>
  );
}

function Badge({
  children,
  tone = "default",
}: {
  children: React.ReactNode;
  tone?: "default" | "accent" | "warn" | "hot";
}) {
  const cls =
    tone === "accent"
      ? "bg-(--color-accent-soft) text-(--color-accent)"
      : tone === "warn"
        ? "bg-amber-500/15 text-(--color-warn)"
        : tone === "hot"
          ? "bg-red-500/15 text-red-300"
          : "bg-(--color-elevated) text-(--color-text-muted)";
  return <span className={`rounded px-1.5 py-0.5 font-mono text-[10px] ${cls}`}>{children}</span>;
}

function NodeList({
  title,
  nodes,
  onSelect,
}: {
  title: string;
  nodes: RawNode[];
  onSelect: (n: RawNode) => void;
}) {
  if (nodes.length === 0) return null;
  return (
    <section className="mb-4">
      <h3 className="mb-1.5 flex items-center justify-between text-[10px] font-semibold tracking-widest text-(--color-text-dim) uppercase">
        <span>{title}</span>
        <span className="font-mono text-(--color-text-dim)">{nodes.length}</span>
      </h3>
      <ul className="space-y-0.5">
        {nodes.slice(0, 30).map((n) => (
          <li key={n.id}>
            <button
              onClick={() => onSelect(n)}
              className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-(--color-elevated)"
            >
              <span
                className="size-1.5 shrink-0 rounded-full"
                style={{ background: KIND_COLOR[n.kind] ?? "#888" }}
              />
              <span className="flex-1 truncate font-mono text-[11px]">{n.name}</span>
              <ChevronRight className="size-3 text-(--color-text-dim)" />
            </button>
          </li>
        ))}
        {nodes.length > 30 && (
          <li className="px-2 py-1 text-[11px] text-(--color-text-dim)">
            +{nodes.length - 30} more
          </li>
        )}
      </ul>
    </section>
  );
}
