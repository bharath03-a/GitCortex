import { Keyboard, Network, Search } from "lucide-react";
import { BranchPicker } from "./BranchPicker";

interface Props {
  onSearch: () => void;
  onShowHelp: () => void;
  activeBranch: string | null;
  onSetActiveBranch: (branch: string) => void;
  diffHead: string | null;
  onSetDiffHead: (branch: string | null) => void;
}

export function Header({
  onSearch,
  onShowHelp,
  activeBranch,
  onSetActiveBranch,
  diffHead,
  onSetDiffHead,
}: Props) {
  return (
    <header className="flex h-14 items-center gap-4 border-b border-(--color-border-subtle) bg-(--color-void-deep)/95 px-4 backdrop-blur-xl">
      <div className="flex shrink-0 items-center gap-2.5">
        <div className="grid size-7 place-items-center rounded-lg bg-(--color-accent-soft)">
          <Network className="size-4 text-(--color-accent)" />
        </div>
        <div>
          <div className="text-[13px] font-semibold tracking-tight">GitCortex</div>
          <div className="text-[9px] tracking-[0.16em] text-(--color-text-dim) uppercase">
            Code graph
          </div>
        </div>
      </div>

      <div className="h-6 w-px shrink-0 bg-(--color-border-subtle)" />
      <BranchPicker
        active={activeBranch}
        onSetActive={onSetActiveBranch}
        diffHead={diffHead}
        onSetDiffHead={onSetDiffHead}
      />

      <button
        onClick={onSearch}
        className="mx-auto flex h-9 w-full max-w-[540px] items-center gap-2 rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/70 px-3 text-(--color-text-muted) shadow-sm transition-colors hover:border-(--color-border-strong) hover:bg-(--color-elevated) hover:text-(--color-text-primary)"
      >
        <Search className="size-3.5 shrink-0" />
        <span className="flex-1 text-left text-[12px]">Find a symbol, file, or type…</span>
        <kbd className="rounded border border-(--color-border-subtle) bg-(--color-void) px-1.5 py-0.5 font-mono text-[9px] text-(--color-text-dim)">
          ⌘K
        </kbd>
      </button>

      <button
        onClick={onShowHelp}
        title="Keyboard shortcuts"
        aria-label="Keyboard shortcuts"
        className="shrink-0 rounded-lg border border-transparent p-2 text-(--color-text-muted) transition-colors hover:border-(--color-border-subtle) hover:bg-(--color-elevated) hover:text-(--color-text-primary)"
      >
        <Keyboard className="size-4" />
      </button>
    </header>
  );
}
