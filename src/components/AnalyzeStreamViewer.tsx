import { useEffect, useRef } from "react";
import { X, BrainCircuit } from "lucide-react";

interface Props {
  log: string;
  analyzing: boolean;
  onClose: () => void;
}

export function AnalyzeStreamViewer({ log, analyzing, onClose }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log]);

  return (
    <div
      className="fixed bottom-10 right-4 w-[420px] max-h-[340px] z-[200] flex flex-col rounded-2xl overflow-hidden shadow-[0_8px_40px_-8px_rgba(0,0,0,0.8)]"
      style={{ background: "#13131A", border: "1px solid #2A2A35" }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-4 py-2.5 shrink-0"
        style={{ borderBottom: "1px solid #2A2A35", background: "rgba(255,255,255,0.03)" }}
      >
        <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest" style={{ color: "#888899" }}>
          <BrainCircuit className="w-3.5 h-3.5" style={{ color: "#C9A84C" }} />
          <span>AI 分析实时输出</span>
          {analyzing && (
            <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: "#C9A84C" }} />
          )}
        </div>
        <button
          onClick={onClose}
          className="transition-colors hover:opacity-80"
          style={{ color: "#888899" }}
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Stream content */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-4 font-mono text-xs leading-relaxed whitespace-pre-wrap break-all"
        style={{ color: "#FAF8F5", minHeight: 80 }}
      >
        {log || (
          <span style={{ color: "#888899" }} className="italic">等待 AI 返回...</span>
        )}
        {analyzing && log && (
          <span
            className="inline-block w-1.5 h-3.5 ml-0.5 animate-pulse align-middle"
            style={{ background: "rgba(201,168,76,0.8)" }}
          />
        )}
      </div>
    </div>
  );
}
