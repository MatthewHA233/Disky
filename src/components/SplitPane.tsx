import { useCallback, useRef, useState, type ReactNode } from "react";

interface Props {
  top: ReactNode;
  bottom: ReactNode;
  initialRatio?: number;
}

const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

export function SplitPane({ top, bottom, initialRatio = 0.5 }: Props) {
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
    <div className="split-pane" ref={containerRef}>
      <div className="split-top" style={{ flex: `0 0 ${ratio * 100}%` }}>
        {top}
      </div>
      <div className="split-handle" onMouseDown={onMouseDown}>
        <div className="split-handle-bar" />
      </div>
      <div className="split-bottom" style={{ flex: 1 }}>
        {bottom}
      </div>
    </div>
  );
}
