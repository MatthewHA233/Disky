import type { ScanSession } from "../hooks/useScanSession";
import { formatSize } from "../lib/format";
import { HardDrive, History, Trash2, Bot, Play, Tag } from "lucide-react";

interface Props {
  scan: ScanSession;
  onHistory: () => void;
  onClean: () => void;
  cleanCount: number;
  onToggleChat: () => void;
  chatOpen: boolean;
  viewMode: "tree" | "tags";
  onToggleViewMode: () => void;
}

export function Header({ scan, onHistory, onClean, cleanCount, onToggleChat, chatOpen, viewMode, onToggleViewMode }: Props) {
  const scanning = scan.status === "scanning";
  const canClean = scan.status === "done" && cleanCount > 0;

  return (
    <header className="shrink-0 w-full bg-[#131318] border-b border-[#2A2A35] px-4 py-1.5 flex items-center gap-4 z-50 select-none" data-tauri-drag-region>
      {/* Left: Icon + Brand */}
      <div className="flex items-center gap-2 shrink-0" data-tauri-drag-region>
        <img src="/icon.png" alt="Disky" className="w-7 h-7 pointer-events-none" draggable={false} />
        <span className="text-base font-bold text-[#C9A84C] font-serif tracking-wide italic pointer-events-none">Disky</span>
      </div>

      {/* Center: Drive selector + Scan button */}
      <div className="flex items-center gap-3 flex-1">
        <div className="relative max-w-[220px] w-full">
          <HardDrive className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[#888899] pointer-events-none" />
          <select
            className="w-full bg-[#0D0D12]/60 border border-[#2A2A35] text-[#FAF8F5] text-sm rounded-md pl-9 pr-6 py-1.5 appearance-none focus:outline-none focus:border-[#C9A84C] transition-colors cursor-pointer disabled:opacity-50"
            value={scan.selectedDrive ?? ""}
            onChange={(e) => scan.setSelectedDrive(e.target.value || null)}
            disabled={scanning}
          >
            <option value="">选择硬盘...</option>
            {scan.drives.map((d) => (
              <option key={d.mount_point} value={d.mount_point}>
                {d.mount_point} ({formatSize(d.available_space)} 可用)
              </option>
            ))}
          </select>
        </div>

        <button
          className="flex items-center gap-1.5 bg-[#C9A84C] hover:bg-[#D4B55C] text-[#0D0D12] px-4 py-1.5 rounded-md font-medium text-sm disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          disabled={!scan.selectedDrive || scanning}
          onClick={scan.startScan}
        >
          {scanning ? (
            <>
              <div className="w-3 h-3 rounded-full bg-[#0D0D12] animate-pulse" />
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

      {/* Right: Action buttons */}
      <div className="flex items-center gap-1 shrink-0">
        <button
          className="p-1.5 text-[#888899] hover:text-[#FAF8F5] rounded-md hover:bg-[#1E1E28] transition-colors disabled:opacity-30 disabled:hover:text-[#888899] disabled:hover:bg-transparent"
          disabled={!scan.rootPath}
          onClick={onHistory}
          title="历史记录"
        >
          <History className="w-4.5 h-4.5" />
        </button>

        <button
          className={`p-1.5 rounded-md transition-colors ${viewMode === 'tags' ? 'text-[#C9A84C] bg-[#1E1E28]' : 'text-[#888899] hover:text-[#C9A84C] hover:bg-[#1E1E28]'}`}
          onClick={onToggleViewMode}
          title="标签看板"
        >
          <Tag className="w-4.5 h-4.5" />
        </button>

        <button
          className="p-1.5 text-[#888899] hover:text-[#E74C3C] rounded-md hover:bg-[#1E1E28] transition-colors disabled:opacity-30 disabled:hover:text-[#888899] disabled:hover:bg-transparent relative"
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

        <button
          className={`p-1.5 rounded-md transition-colors ${chatOpen ? 'text-[#C9A84C] bg-[#1E1E28]' : 'text-[#888899] hover:text-[#C9A84C] hover:bg-[#1E1E28]'}`}
          onClick={onToggleChat}
          title="AI 助手"
        >
          <Bot className="w-4.5 h-4.5" />
        </button>
      </div>
    </header>
  );
}
