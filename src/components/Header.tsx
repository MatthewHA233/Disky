import type { ScanSession } from "../hooks/useScanSession";
import { formatSize } from "../lib/format";
import { HardDrive, History, Trash2, Bot, Play } from "lucide-react";

interface Props {
  scan: ScanSession;
  onHistory: () => void;
  onClean: () => void;
  cleanCount: number;
  onToggleChat: () => void;
  chatOpen: boolean;
}

export function Header({ scan, onHistory, onClean, cleanCount, onToggleChat, chatOpen }: Props) {
  const scanning = scan.status === "scanning";
  const canClean = scan.status === "done" && cleanCount > 0;

  return (
    <div className="fixed top-2 left-1/2 -translate-x-1/2 z-50">
      <header className="glass-panel rounded-full px-4 py-1.5 flex items-center gap-4 min-w-[600px]">
        <h1 className="text-lg font-bold text-[#C9A84C] font-serif tracking-wide italic">Disky.</h1>

        <div className="flex items-center gap-4 flex-1">
          <div className="relative flex-1 max-w-xs">
            <HardDrive className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[#888899]" />
            <select
              className="w-full bg-[#0D0D12]/50 border border-[#2A2A35] text-[#FAF8F5] text-sm rounded-full pl-9 pr-8 py-2 appearance-none focus:outline-none focus:border-[#C9A84C] transition-colors cursor-pointer disabled:opacity-50"
              value={scan.selectedDrive ?? ""}
              onChange={(e) => scan.setSelectedDrive(e.target.value || null)}
              disabled={scanning}
            >
              <option value="">选择节点...</option>
              {scan.drives.map((d) => (
                <option key={d.mount_point} value={d.mount_point}>
                  {d.mount_point} ({formatSize(d.available_space)} 可用)
                </option>
              ))}
            </select>
          </div>

          <button
            className="magnetic-btn flex items-center gap-2 bg-[#C9A84C] hover:bg-[#D4B55C] text-[#0D0D12] px-6 py-2 rounded-full font-medium text-sm shadow-[0_0_15px_rgba(201,168,76,0.3)] disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:scale-100 disabled:shadow-none"
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
                <Play className="w-4 h-4" />
                分析
              </>
            )}
          </button>
        </div>

        <div className="flex items-center gap-2 border-l border-[#2A2A35] pl-6">
          <button
            className="magnetic-btn p-2 text-[#888899] hover:text-[#FAF8F5] transition-colors disabled:opacity-30 disabled:hover:text-[#888899] disabled:hover:scale-100"
            disabled={!scan.rootPath}
            onClick={onHistory}
            title="历史记录"
          >
            <History className="w-5 h-5" />
          </button>

          <button
            className="magnetic-btn p-2 text-[#888899] hover:text-[#E74C3C] transition-colors disabled:opacity-30 disabled:hover:text-[#888899] disabled:hover:scale-100 relative"
            disabled={!canClean}
            onClick={onClean}
            title="清理"
          >
            <Trash2 className="w-5 h-5" />
            {cleanCount > 0 && (
              <span className="absolute top-0 right-0 w-4 h-4 bg-[#E74C3C] text-white text-[10px] flex items-center justify-center rounded-full font-mono translate-x-1 -translate-y-1">
                {cleanCount}
              </span>
            )}
          </button>

          <button
            className={`magnetic-btn p-2 transition-colors ${chatOpen ? 'text-[#C9A84C]' : 'text-[#888899] hover:text-[#C9A84C]'}`}
            onClick={onToggleChat}
            title="AI 助手"
          >
            <Bot className="w-5 h-5" />
          </button>
        </div>
      </header>
    </div>
  );
}
