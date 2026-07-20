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
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    fetchBranches()
      .then((b) => setBranches(b.branches))
      .catch(() => {});
  }, [open]);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  return (
    <div ref={wrapRef} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 rounded-md border border-(--color-border-subtle) bg-(--color-elevated) px-2.5 py-1.5 text-(--color-text-muted) hover:text-(--color-text-primary)"
      >
        <GitBranch className="size-3.5" />
        <span className="font-mono text-[11px]">{active ?? "—"}</span>
        {diffHead && (
          <span className="font-mono text-[11px] text-(--color-accent)">↔ {diffHead}</span>
        )}
        <ChevronDown className="size-3" />
      </button>
      {open && (
        <div className="animate-fade-in absolute top-full right-0 z-40 mt-1 w-[260px] overflow-hidden rounded-lg border border-(--color-border-subtle) bg-(--color-elevated) shadow-2xl">
          <div className="border-b border-(--color-border-subtle) px-3 py-2 text-[10px] tracking-widest text-(--color-text-dim) uppercase">
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
                    branch === active ? "text-(--color-accent)" : "text-(--color-text-primary)"
                  }`}
                >
                  <span className="font-mono text-[12px]">{branch}</span>
                  {branch === active && <Check className="size-3.5" />}
                </button>
              </li>
            ))}
          </ul>
          <div className="flex items-center gap-1.5 border-y border-(--color-border-subtle) px-3 py-2 text-[10px] tracking-widest text-(--color-text-dim) uppercase">
            <GitCompare className="size-3" /> Compare {active ?? "—"} with
          </div>
          <ul className="max-h-[24vh] overflow-y-auto py-1">
            {diffHead && (
              <li>
                <button
                  onClick={() => {
                    onSetDiffHead(null);
                    setOpen(false);
                  }}
                  className="flex w-full items-center px-3 py-1.5 text-left text-[12px] text-(--color-text-muted) hover:bg-(--color-accent-soft) hover:text-(--color-text-primary)"
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
                      diffHead === branch ? "text-(--color-accent)" : "text-(--color-text-primary)"
                    }`}
                  >
                    <span className="font-mono text-[12px]">{branch}</span>
                    {diffHead === branch && <Check className="size-3.5" />}
                  </button>
                </li>
              ))}
            {branches.length === 0 && (
              <li className="px-3 py-2 text-[11px] text-(--color-text-dim)">Loading branches…</li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
