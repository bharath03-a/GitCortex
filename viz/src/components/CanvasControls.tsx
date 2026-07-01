import { useEffect, useState, type RefObject } from "react";
import { Focus, Maximize2, Pause, Play, RotateCcw, ZoomIn, ZoomOut } from "lucide-react";
import type { CosmographRef } from "@cosmograph/react";

interface Props {
  cosmoRef: RefObject<CosmographRef | null>;
}

export function CanvasControls({ cosmoRef }: Props) {
  const [playing, setPlaying] = useState(true);
  const c = () => cosmoRef.current;

  // F = fit graph, Space = pause/resume — matches KeyboardHelp.tsx documentation.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const isInput =
        e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (isInput) return;
      if (e.key === "f" || e.key === "F") {
        e.preventDefault();
        cosmoRef.current?.fitView(400);
      } else if (e.key === " ") {
        e.preventDefault();
        const inst = cosmoRef.current;
        if (!inst) return;
        if (playing) {
          inst.pause();
        } else {
          inst.unpause();
        }
        setPlaying((p) => !p);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [playing, cosmoRef]);

  const togglePlay = () => {
    const inst = c();
    if (!inst) return;
    if (playing) {
      inst.pause();
    } else {
      inst.unpause();
    }
    setPlaying(!playing);
  };

  return (
    <div className="absolute right-3 bottom-3 z-20 flex items-center gap-1 rounded-lg border border-(--color-border-subtle) bg-(--color-elevated)/90 p-1 backdrop-blur-sm">
      <Btn
        title="Zoom in"
        onClick={() => {
          const inst = c();
          if (!inst) return;
          inst.setZoomLevel((inst.getZoomLevel() ?? 1) * 1.4, 200);
        }}
      >
        <ZoomIn className="size-4" />
      </Btn>
      <Btn
        title="Zoom out"
        onClick={() => {
          const inst = c();
          if (!inst) return;
          inst.setZoomLevel((inst.getZoomLevel() ?? 1) / 1.4, 200);
        }}
      >
        <ZoomOut className="size-4" />
      </Btn>
      <Btn title="Fit graph" onClick={() => c()?.fitView(400)}>
        <Maximize2 className="size-4" />
      </Btn>
      <Btn
        title="Focus selected"
        onClick={() => {
          const inst = c();
          if (!inst) return;
          const idx = inst.focusedPointIndex;
          if (idx != null) inst.zoomToPoint(idx, 400, 5, true);
          else inst.fitView(400);
        }}
      >
        <Focus className="size-4" />
      </Btn>
      <Btn title={playing ? "Pause layout" : "Resume layout"} onClick={togglePlay}>
        {playing ? <Pause className="size-4" /> : <Play className="size-4" />}
      </Btn>
      <Btn title="Restart layout" onClick={() => c()?.start(1)}>
        <RotateCcw className="size-4" />
      </Btn>
    </div>
  );
}

function Btn({
  children,
  onClick,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      title={title}
      aria-label={title}
      onClick={onClick}
      className="rounded-md p-1.5 text-(--color-text-muted) hover:bg-(--color-elevated-hi) hover:text-(--color-text-primary)"
    >
      {children}
    </button>
  );
}
