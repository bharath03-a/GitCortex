import { Keyboard, X } from "lucide-react";

interface Props {
  onClose: () => void;
}

const SHORTCUTS: { keys: string[]; desc: string }[] = [
  { keys: ["⌘", "K"], desc: "Open search palette" },
  { keys: ["/"], desc: "Open search palette" },
  { keys: ["?"], desc: "Show this help" },
  { keys: ["["], desc: "Toggle left filter rail" },
  { keys: ["]"], desc: "Close inspector" },
  { keys: ["1"], desc: "Density: Focused" },
  { keys: ["2"], desc: "Density: Public API" },
  { keys: ["3"], desc: "Density: Full graph" },
  { keys: ["U"], desc: "Toggle dead-code (unused) overlay" },
  { keys: ["G"], desc: "Toggle hub-node (high fan-in) overlay" },
  { keys: ["C"], desc: "Toggle most-changed file overlay" },
  { keys: ["F"], desc: "Fit graph to view" },
  { keys: ["Space"], desc: "Pause / resume layout" },
  { keys: ["Esc"], desc: "Close modal / clear selection" },
];

export function KeyboardHelp({ onClose }: Props) {
  return (
    <div
      className="animate-fade-in fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="w-[480px] max-w-[90vw] overflow-hidden rounded-xl border border-(--color-border-subtle) bg-(--color-elevated) shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-(--color-border-subtle) px-4 py-3">
          <div className="flex items-center gap-2">
            <Keyboard className="size-4 text-(--color-accent)" />
            <span className="font-medium">Keyboard shortcuts</span>
          </div>
          <button
            onClick={onClose}
            className="rounded p-1 text-(--color-text-muted) hover:text-(--color-text-primary)"
          >
            <X className="size-4" />
          </button>
        </div>
        <ul className="grid grid-cols-1 gap-1 p-3">
          {SHORTCUTS.map((s) => (
            <li
              key={s.desc}
              className="flex items-center justify-between rounded px-2 py-1.5 hover:bg-(--color-void)"
            >
              <span className="text-[12px] text-(--color-text-muted)">{s.desc}</span>
              <span className="flex items-center gap-1">
                {s.keys.map((k) => (
                  <kbd
                    key={k}
                    className="rounded border border-(--color-border-subtle) bg-(--color-void) px-1.5 py-0.5 font-mono text-[10px] text-(--color-text-primary)"
                  >
                    {k}
                  </kbd>
                ))}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
