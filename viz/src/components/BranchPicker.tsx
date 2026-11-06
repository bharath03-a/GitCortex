import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, GitBranch, GitCompare } from "lucide-react";
import { fetchBranches } from "../api";

interface Props {
  active: string | null;
  onSetActive: (branch: string) => void;
  diffHead: string | null;
  onSetDiffHead: (head: string | null) => void;
}

export function BranchPicker({ active, onSetActive, diffHead, onSetDiffHead }: Props) {
  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const controller = new AbortController();
    fetchBranches(controller.signal)
      .then((result) => {
        const available = new Set(result.branches);
        if (active) available.add(active);
        setBranches(
          [...available].sort((left, right) => {
            if (left === active) return -1;
            if (right === active) return 1;
            return left.localeCompare(right);
          }),
        );
      })
      .catch((error) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setLoadError(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [open, active]);

  useEffect(() => {
    const onDoc = (event: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, []);

  return (
    <div ref={wrapRef} className="relative">
      <button
        onClick={() => {
          if (!open) {
            setLoading(true);
            setLoadError(false);
          }
          setOpen((current) => !current);
        }}
        aria-haspopup="menu"
        aria-expanded={open}
        className="flex h-9 min-w-[168px] max-w-[240px] items-center gap-1.5 rounded-md border border-(--color-border-subtle) bg-(--color-void-deep) px-2.5 text-(--color-text-muted) transition-colors hover:border-(--color-border-strong) hover:text-(--color-text-primary)"
      >
        <GitBranch className="size-3.5 shrink-0" />
        <span
          title={active ?? undefined}
          className="font-mono min-w-0 flex-1 truncate text-left text-ui-xs font-medium"
        >
          {active ?? "—"}
        </span>
        {diffHead && (
          <span
            title={diffHead}
            className="font-mono max-w-[86px] truncate text-ui-xs text-(--color-accent-text)"
          >
            vs {diffHead}
          </span>
        )}
        <ChevronDown
          className={`size-3 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="animate-fade-in absolute top-full right-0 z-40 mt-1.5 w-[280px] overflow-hidden rounded-md border border-(--color-border-subtle) bg-(--color-void-deep) shadow-[var(--shadow-float)]">
          <div className="border-b border-(--color-border-subtle) bg-(--color-elevated)/65 px-3 py-2 text-ui-base font-heading text-(--color-text-dim)">
            View branch
          </div>
          <ul className="max-h-[24vh] overflow-y-auto py-1">
            {branches.map((branch) => (
              <li key={`view-${branch}`}>
                <button
                  onClick={() => {
                    if (branch !== active) onSetActive(branch);
                    setOpen(false);
                  }}
                  className={`flex w-full items-center justify-between px-3 py-1.5 text-left hover:bg-(--color-accent-soft) ${
                    branch === active ? "text-(--color-accent-text)" : "text-(--color-text-primary)"
                  }`}
                >
                  <span className="min-w-0 break-all font-mono text-ui-xs">{branch}</span>
                  {branch === active && <Check className="size-3.5" />}
                </button>
              </li>
            ))}
            {loading && branches.length === 0 && (
              <li className="px-3 py-2 text-ui-base text-(--color-text-dim)">Loading branches…</li>
            )}
            {loadError && (
              <li className="px-3 py-2 text-ui-base text-(--color-bad)">
                Branches could not be refreshed. Close and retry.
              </li>
            )}
          </ul>
          <div className="flex items-center gap-1.5 border-y border-(--color-border-subtle) bg-(--color-elevated)/65 px-3 py-2 text-ui-base font-heading text-(--color-text-dim)">
            <GitCompare className="size-3" /> Compare against
          </div>
          <ul className="max-h-[24vh] overflow-y-auto py-1">
            {diffHead && (
              <li>
                <button
                  onClick={() => {
                    onSetDiffHead(null);
                    setOpen(false);
                  }}
                  className="flex w-full items-center px-3 py-1.5 text-left text-ui-xs text-(--color-text-muted) hover:bg-(--color-accent-soft) hover:text-(--color-text-primary)"
                >
                  Clear comparison
                </button>
              </li>
            )}
            {branches
              .filter((branch) => branch !== active)
              .map((branch) => (
                <li key={`compare-${branch}`}>
                  <button
                    onClick={() => {
                      onSetDiffHead(branch);
                      setOpen(false);
                    }}
                    className={`flex w-full items-center justify-between px-3 py-1.5 text-left hover:bg-(--color-accent-soft) ${
                      diffHead === branch
                        ? "text-(--color-accent-text)"
                        : "text-(--color-text-primary)"
                    }`}
                  >
                    <span className="min-w-0 break-all font-mono text-ui-xs">{branch}</span>
                    {diffHead === branch && <Check className="size-3.5" />}
                  </button>
                </li>
              ))}
            {!loading && branches.length <= 1 && (
              <li className="px-3 py-2 text-ui-base text-(--color-text-dim)">
                No other local branches
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
