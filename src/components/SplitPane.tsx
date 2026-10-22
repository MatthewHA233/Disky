import { useCallback, useRef, useState, type ReactNode } from "react";

interface Props {
  top: ReactNode;
  bottom: ReactNode;
  initialRatio?: number;
}

const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

export function SplitPane({ top, bottom, initialRatio = 0.55 }: Props) {
  const [ratio, setRatio] = useState(initialRatio);
  const containerRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;

    const onMouseMove = (ev: MouseEvent) => {
      if (!dragging.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const y = ev.clientY - rect.top;
      const newRatio = Math.min(MAX_RATIO, Math.max(MIN_RATIO, y / rect.height));
      setRatio(newRatio);
    };

    const onMouseUp = () => {
      dragging.current = false;
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, []);

  return (
    <div className="flex flex-col w-full h-full overflow-hidden" ref={containerRef}>
      <div className="relative overflow-hidden min-h-0" style={{ flex: `0 0 ${ratio * 100}%` }}>
        {top}
      </div>
      <div
        className="group relative shrink-0 h-2 cursor-row-resize z-20"
        onMouseDown={onMouseDown}
        role="separator"
        aria-orientation="horizontal"
        title="拖动调整上下比例"
      >
        <span className="absolute left-1/2 top-[3px] w-12 h-0.5 -ml-6 rounded-full bg-slate transition-colors group-hover:bg-photon" />
      </div>
      <div className="relative overflow-hidden min-h-0" style={{ flex: 1 }}>
        {bottom}
      </div>
    </div>
  );
}
