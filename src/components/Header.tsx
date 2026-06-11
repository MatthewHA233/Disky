import { useEffect, useRef, useState } from "react";
import type { ScanSession } from "../hooks/useScanSession";
import { formatSize } from "../lib/format";
import { HardDrive, History, Trash2, Bot, Play, Tag, X, BrainCircuit, ChevronDown } from "lucide-react";

interface Props {
  scan: ScanSession;
  cleanedBytes?: number;
  onHistory: () => void;
  onClean: () => void;
  cleanCount: number;
  onClearSelected: () => void;
  onToggleChat: () => void;
  chatOpen: boolean;
  viewMode: "tree" | "tags";
  onToggleViewMode: () => void;
  analyzing?: boolean;
  onToggleAnalyzeLog?: () => void;
  analyzeLogOpen?: boolean;
}

export function Header({ scan, cleanedBytes = 0, onHistory, onClean, cleanCount, onClearSelected, onToggleChat, chatOpen, viewMode, onToggleViewMode, analyzing = false, onToggleAnalyzeLog, analyzeLogOpen = false }: Props) {
  const scanning = scan.status === "scanning";
  const canClean = scan.status === "done" && cleanCount > 0;
  const [driveOpen, setDriveOpen] = useState(false);
  const driveRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!driveOpen) return;
    const handler = (e: MouseEvent) => {
      if (driveRef.current && !driveRef.current.contains(e.target as Node)) {
        setDriveOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [driveOpen]);

  const selectedLabel = scan.selectedDrive
    ? (() => {
        const d = scan.drives.find((d) => d.mount_point === scan.selectedDrive);
        const cleaned = d && cleanedBytes > 0 ? ` +${formatSize(cleanedBytes)}` : "";
        return d ? `${d.mount_point} (${formatSize(d.available_space)}${cleaned} 可用)` : scan.selectedDrive;
      })()
    : "选择硬盘...";

  return (
    <header className="shrink-0 w-full bg-[#131318] border-b border-slate px-4 py-1.5 flex items-center gap-4 z-50 select-none" data-tauri-drag-region>
      {/* Left: Icon + Brand */}
      <div className="flex items-center gap-2 shrink-0" data-tauri-drag-region>
        <img src="/icon.png" alt="Disky" className="w-7 h-7 pointer-events-none" draggable={false} />
        <span className="text-base font-bold text-photon font-serif tracking-wide italic pointer-events-none">Disky</span>
      </div>

      {/* Center: Drive selector + Scan button */}
      <div className="flex items-center gap-3 flex-1">
        {/* Custom drive dropdown */}
        <div ref={driveRef} className="relative max-w-[240px] w-full">
          <button
            className="w-full flex items-center gap-2 text-sm rounded-md px-3 py-1.5 transition-colors disabled:opacity-50 disabled:cursor-not-allowed text-left"
            style={{
              background: "rgba(255,255,255,0.05)",
              border: `1px solid ${driveOpen ? "#C9A84C" : "#2A2A35"}`,
              color: scan.selectedDrive ? "#FAF8F5" : "#888899",
            }}
            disabled={scanning}
            onClick={() => !scanning && setDriveOpen((v) => !v)}
          >
            <HardDrive className="w-4 h-4 shrink-0" style={{ color: "#888899" }} />
            <span className="flex-1 truncate">{selectedLabel}</span>
            <ChevronDown
              className="w-3.5 h-3.5 shrink-0 transition-transform"
              style={{ color: "#888899", transform: driveOpen ? "rotate(180deg)" : "rotate(0deg)" }}
            />
          </button>

          {driveOpen && (
            <div
              className="absolute top-full left-0 mt-1 w-full rounded-md overflow-hidden z-[200] shadow-[0_8px_24px_rgba(0,0,0,0.6)]"
              style={{ background: "#1A1A24", border: "1px solid #2A2A35" }}
            >
              {scan.drives.length === 0 && (
                <div className="px-3 py-2 text-sm" style={{ color: "#888899" }}>无可用磁盘</div>
              )}
              {scan.drives.map((d) => {
                const isCurrent = d.mount_point === scan.selectedDrive;
                const cleaned = isCurrent && cleanedBytes > 0 ? ` +${formatSize(cleanedBytes)}` : "";
                return (
                  <button
                    key={d.mount_point}
                    className="w-full flex items-center gap-2 px-3 py-2 text-sm text-left transition-colors"
                    style={{
                      color: isCurrent ? "#C9A84C" : "#FAF8F5",
                      background: isCurrent ? "rgba(201,168,76,0.1)" : "transparent",
                    }}
                    onMouseEnter={(e) => { if (!isCurrent) (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.05)"; }}
                    onMouseLeave={(e) => { if (!isCurrent) (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                    onClick={() => {
                      scan.setSelectedDrive(d.mount_point);
                      setDriveOpen(false);
                    }}
                  >
                    <HardDrive className="w-3.5 h-3.5 shrink-0" style={{ color: isCurrent ? "#C9A84C" : "#888899" }} />
                    <span className="truncate">
                      {d.mount_point} <span style={{ color: "#888899" }}>({formatSize(d.available_space)}{cleaned} 可用)</span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <button
          className="flex items-center gap-1.5 bg-photon hover:bg-[#D4B55C] text-fiber px-4 py-1.5 rounded-md font-medium text-sm disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          disabled={!scan.selectedDrive || scanning}
          onClick={scan.startScan}
        >
          {scanning ? (
            <>
              <div className="w-3 h-3 rounded-full bg-fiber animate-pulse" />
              扫描中
            </>
          ) : (
            <>
              <Play className="w-3.5 h-3.5" />
              扫描
            </>
          )}
        </button>
      </div>

      {/* Multi-select indicator pill */}
      {cleanCount > 0 && (
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-photon/10 border border-photon/30 text-photon font-mono text-xs shrink-0">
          <span className="font-semibold">{cleanCount}</span>
          <span className="text-photon/70">项已选</span>
          <button
            className="ml-0.5 hover:text-textMain transition-colors"
            onClick={onClearSelected}
            title="取消全部选择"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      )}

      {/* Right: Action buttons */}
      <div className="flex items-center gap-1 shrink-0">
        <button
          className="p-1.5 text-text-dim hover:text-textMain rounded-md hover:bg-[#1E1E28] transition-colors"
          onClick={onHistory}
          title="历史记录"
        >
          <History className="w-4.5 h-4.5" />
        </button>

        <button
          className={`p-1.5 rounded-md transition-colors disabled:opacity-30 disabled:hover:text-text-dim disabled:hover:bg-transparent ${viewMode === 'tags' ? 'text-photon bg-[#1E1E28]' : 'text-text-dim hover:text-photon hover:bg-[#1E1E28]'}`}
          onClick={onToggleViewMode}
          disabled={scanning || !scan.rootPath}
          title="标签看板"
        >
          <Tag className="w-4.5 h-4.5" />
        </button>

        <button
          className="p-1.5 text-text-dim hover:text-danger rounded-md hover:bg-[#1E1E28] transition-colors disabled:opacity-30 disabled:hover:text-text-dim disabled:hover:bg-transparent relative"
          disabled={!canClean}
          onClick={onClean}
          title="清理"
        >
          <Trash2 className="w-4.5 h-4.5" />
          {cleanCount > 0 && (
            <span className="absolute top-0 right-0 w-3.5 h-3.5 bg-[#E74C3C] text-white text-[9px] flex items-center justify-center rounded-full font-mono translate-x-0.5 -translate-y-0.5">
              {cleanCount}
            </span>
          )}
        </button>

        {(analyzing || analyzeLogOpen) && onToggleAnalyzeLog && (
          <button
            className={`relative p-1.5 rounded-md transition-colors ${analyzeLogOpen ? 'text-photon bg-[#1E1E28]' : 'text-text-dim hover:text-photon hover:bg-[#1E1E28]'}`}
            onClick={onToggleAnalyzeLog}
            title="AI 分析实时输出"
          >
            <BrainCircuit className="w-4.5 h-4.5" />
            {analyzing && (
              <span className="absolute top-0.5 right-0.5 w-1.5 h-1.5 rounded-full bg-photon animate-pulse" />
            )}
          </button>
        )}

        <button
          className={`p-1.5 rounded-md transition-colors ${chatOpen ? 'text-photon bg-[#1E1E28]' : 'text-text-dim hover:text-photon hover:bg-[#1E1E28]'}`}
          onClick={onToggleChat}
          title="AI 助手"
        >
          <Bot className="w-4.5 h-4.5" />
        </button>
      </div>
    </header>
  );
}
