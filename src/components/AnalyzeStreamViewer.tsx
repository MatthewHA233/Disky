import { useEffect, useRef } from "react";
import { X } from "lucide-react";
import { AiIcon } from "./ui/AiIcon";

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
      className="fixed bottom-10 right-4 w-[420px] max-h-[340px] z-[200] flex flex-col rounded-lg overflow-hidden border border-slate shadow-[var(--fl-shadow)]"
      style={{ background: "var(--fl-card)", border: "1px solid var(--fl-stroke)" }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-4 py-2.5 shrink-0"
        style={{ borderBottom: "1px solid var(--fl-stroke)", background: "var(--fl-hover)" }}
      >
        <div className="flex items-center gap-2 text-xs" style={{ color: "var(--fl-text-2)" }}>
          <AiIcon className="w-3.5 h-3.5" style={{ color: "var(--fl-accent)" }} />
          <span>AI 分析实时输出</span>
          {analyzing && (
            <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: "var(--fl-accent)" }} />
          )}
        </div>
        <button
          onClick={onClose}
          className="transition-colors hover:opacity-80"
          style={{ color: "var(--fl-text-2)" }}
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Stream content */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-4 font-code text-xs leading-relaxed whitespace-pre-wrap break-all"
        style={{ color: "var(--fl-text)", minHeight: 80 }}
      >
        {log || (
          <span style={{ color: "var(--fl-text-2)" }} className="italic">等待 AI 返回...</span>
        )}
        {analyzing && log && (
          <span
            className="inline-block w-1.5 h-3.5 ml-0.5 animate-pulse align-middle"
            style={{ background: "var(--fl-accent)" }}
          />
        )}
      </div>
    </div>
  );
}
