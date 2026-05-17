import { useEffect, useRef, useState } from "react";
import { ChevronDown, GitBranch } from "lucide-react";
import { fetchBranches } from "../api";

interface Props {
  active: string | null;
  diffHead: string | null;
  onSetDiffHead: (head: string | null) => void;
}

export function BranchPicker({ active, diffHead, onSetDiffHead }: Props) {
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
          <span className="font-mono text-[11px] text-(--color-accent)">
            ↔ {diffHead}
          </span>
        )}
        <ChevronDown className="size-3" />
      </button>
      {open && (
        <div className="animate-fade-in absolute top-full right-0 z-40 mt-1 w-[260px] overflow-hidden rounded-lg border border-(--color-border-subtle) bg-(--color-elevated) shadow-2xl">
          <div className="border-b border-(--color-border-subtle) px-3 py-2 text-[10px] tracking-widest text-(--color-text-dim) uppercase">
            Diff vs current ({active ?? "—"})
          </div>
          <ul className="max-h-[40vh] overflow-y-auto py-1">
            {diffHead && (
              <li>
                <button
                  onClick={() => {
                    onSetDiffHead(null);
                    setOpen(false);
                  }}
                  className="flex w-full items-center justify-between px-3 py-1.5 text-left text-(--color-text-muted) hover:bg-(--color-accent-soft) hover:text-(--color-text-primary)"
                >
                  <span className="text-[12px]">Clear diff overlay</span>
                </button>
              </li>
            )}
            {branches
              .filter((b) => b !== active)
              .map((b) => (
                <li key={b}>
                  <button
                    onClick={() => {
                      onSetDiffHead(b);
                      setOpen(false);
                    }}
                    className={`flex w-full items-center justify-between px-3 py-1.5 text-left hover:bg-(--color-accent-soft) ${
                      diffHead === b
                        ? "text-(--color-accent)"
                        : "text-(--color-text-primary)"
                    }`}
                  >
                    <span className="font-mono text-[12px]">{b}</span>
                  </button>
                </li>
              ))}
            {branches.length === 0 && (
              <li className="px-3 py-2 text-[11px] text-(--color-text-dim)">
                Loading branches…
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
