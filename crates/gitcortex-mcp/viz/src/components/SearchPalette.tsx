import { useEffect, useMemo, useRef, useState } from "react";
import { Search } from "lucide-react";
import type { GraphData, RawNode } from "../api";
import { KIND_COLOR, KIND_LABEL } from "../theme/colors";

interface Props {
  data: GraphData | null;
  onClose: () => void;
  onSelect: (n: RawNode) => void;
}

export function SearchPalette({ data, onClose, onSelect }: Props) {
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const results = useMemo(() => {
    if (!data) return [];
    const q = query.trim().toLowerCase();
    if (!q) return data.nodes.slice(0, 50);
    const scored: { n: RawNode; score: number }[] = [];
    for (const n of data.nodes) {
      const name = n.name.toLowerCase();
      const qn = n.qualified_name.toLowerCase();
      let score = 0;
      if (name === q) score = 1000;
      else if (name.startsWith(q)) score = 800;
      else if (name.includes(q)) score = 500;
      else if (qn.includes(q)) score = 200;
      if (score > 0) scored.push({ n, score });
    }
    scored.sort((a, b) => b.score - a.score);
    return scored.slice(0, 50).map((s) => s.n);
  }, [query, data]);

  useEffect(() => setCursor(0), [query]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const pick = results[cursor];
      if (pick) {
        onSelect(pick);
        onClose();
      }
    }
  };

  return (
    <div
      className="animate-fade-in fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-[15vh]"
      onClick={onClose}
    >
      <div
        className="w-[640px] max-w-[90vw] overflow-hidden rounded-xl border border-(--color-border-subtle) bg-(--color-elevated) shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-(--color-border-subtle) px-3 py-2.5">
          <Search className="size-4 text-(--color-text-muted)" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder="Search functions, structs, files…"
            className="flex-1 bg-transparent text-(--color-text-primary) placeholder:text-(--color-text-dim) focus:outline-none"
          />
          <kbd className="rounded bg-(--color-void) px-1.5 py-0.5 font-mono text-[10px] text-(--color-text-dim)">
            esc
          </kbd>
        </div>
        <ul className="max-h-[50vh] overflow-y-auto py-1">
          {results.length === 0 && (
            <li className="px-4 py-6 text-center text-[12px] text-(--color-text-dim)">
              No matches
            </li>
          )}
          {results.map((n, i) => (
            <li key={n.id}>
              <button
                onMouseEnter={() => setCursor(i)}
                onClick={() => {
                  onSelect(n);
                  onClose();
                }}
                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left ${
                  i === cursor ? "bg-(--color-accent-soft)" : ""
                }`}
              >
                <span
                  className="size-2 shrink-0 rounded-full"
                  style={{ background: KIND_COLOR[n.kind] ?? "#888" }}
                />
                <span className="flex-1 truncate font-mono text-[12px]">
                  {n.name}
                </span>
                <span className="shrink-0 text-[10px] tracking-widest text-(--color-text-dim) uppercase">
                  {KIND_LABEL[n.kind] ?? n.kind}
                </span>
                <span className="shrink-0 truncate font-mono text-[10px] text-(--color-text-dim)">
                  {n.file.split("/").slice(-2).join("/")}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
