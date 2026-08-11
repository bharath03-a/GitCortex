import { CircleHelp, Moon, Search, Sun } from "lucide-react";
import type { ProductTheme } from "../theme/colors";
import { BranchPicker } from "./BranchPicker";

interface Props {
  onSearch: () => void;
  onShowHelp: () => void;
  repoName: string | null;
  activeBranch: string | null;
  onSetActiveBranch: (branch: string) => void;
  diffHead: string | null;
  onSetDiffHead: (branch: string | null) => void;
  theme: ProductTheme;
  onToggleTheme: () => void;
}

export function Header({
  onSearch,
  onShowHelp,
  repoName,
  activeBranch,
  onSetActiveBranch,
  diffHead,
  onSetDiffHead,
  theme,
  onToggleTheme,
}: Props) {
  return (
    <header className="relative z-50 flex h-[60px] shrink-0 items-center gap-4 border-b border-(--color-border-subtle) bg-(--color-void-deep)/95 px-5 backdrop-blur-xl">
      <div className="flex min-w-[220px] shrink-0 items-center gap-3">
        <BrandMark />
        <div className="leading-none">
          <div className="text-[14px] font-bold tracking-[-0.025em]">GitCortex</div>
          <div className="mt-1.5 font-mono text-[8px] font-semibold tracking-[0.16em] text-(--color-text-dim) uppercase">
            {repoName ? `${repoName} · local code atlas` : "Local code atlas"}
          </div>
        </div>
        <span className="ml-1 rounded-full bg-(--color-accent-soft) px-2 py-1 font-mono text-[8px] font-semibold tracking-[0.08em] text-(--color-accent) uppercase">
          live
        </span>
      </div>

      <button
        onClick={onSearch}
        className="group mx-auto flex h-9 w-full max-w-[620px] items-center gap-2.5 rounded-md border border-(--color-border-subtle) bg-(--color-elevated)/70 px-3 text-(--color-text-muted) shadow-[0_1px_2px_rgba(20,20,26,0.03)] transition-colors hover:border-(--color-border-strong) hover:bg-(--color-void-deep) hover:text-(--color-text-primary)"
      >
        <Search className="size-3.5 shrink-0 text-(--color-accent)" />
        <span className="flex-1 text-left text-[12px]">Find a symbol, file, or qualified name</span>
        <kbd className="rounded border border-(--color-border-subtle) bg-(--color-void-deep) px-1.5 py-0.5 font-mono text-[9px] text-(--color-text-dim)">
          ⌘ K
        </kbd>
      </button>

      <div className="flex min-w-[280px] shrink-0 items-center justify-end gap-2">
        <BranchPicker
          active={activeBranch}
          onSetActive={onSetActiveBranch}
          diffHead={diffHead}
          onSetDiffHead={onSetDiffHead}
        />
        <button
          onClick={onToggleTheme}
          title={`Use ${theme === "light" ? "dark" : "light"} theme`}
          aria-label={`Use ${theme === "light" ? "dark" : "light"} theme`}
          className="rounded-md border border-transparent p-2 text-(--color-text-muted) transition-colors hover:border-(--color-border-subtle) hover:bg-(--color-elevated) hover:text-(--color-accent)"
        >
          {theme === "light" ? <Moon className="size-4" /> : <Sun className="size-4" />}
        </button>
        <button
          onClick={onShowHelp}
          title="Keyboard shortcuts"
          aria-label="Keyboard shortcuts"
          className="rounded-md border border-transparent p-2 text-(--color-text-muted) transition-colors hover:border-(--color-border-subtle) hover:bg-(--color-elevated) hover:text-(--color-text-primary)"
        >
          <CircleHelp className="size-4" />
        </button>
      </div>
    </header>
  );
}

function BrandMark() {
  return (
    <svg className="size-7 shrink-0" viewBox="0 0 28 28" fill="none" aria-hidden="true">
      <path
        d="M6.5 6.5 21.2 13.7M6.5 6.5l7.2 15M21.2 13.7l-7.5 7.8"
        className="stroke-(--color-accent)"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <circle cx="6.5" cy="6.5" r="3.6" className="fill-(--color-accent)" />
      <circle cx="21.2" cy="13.7" r="2.9" className="fill-(--color-accent-deep)" />
      <circle cx="13.7" cy="21.5" r="2.9" className="fill-(--color-accent-deep)" />
    </svg>
  );
}
