import type { ScanSession } from "../hooks/useScanSession";
import { formatSize, formatNumber } from "../lib/format";
import { CheckCircle2, XCircle, Loader2, Activity } from "lucide-react";

interface Props {
  scan: ScanSession;
}

export function StatusBar({ scan }: Props) {
  if (scan.status === "error") {
    return (
      <footer className="h-6 px-4 flex items-center gap-2 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#E74C3C] font-mono shrink-0">
        <XCircle className="w-3 h-3" />
        <span className="truncate">错误: {scan.errorMsg}</span>
      </footer>
    );
  }

  if (scan.status === "scanning") {
    const p = scan.progress;
    const drive = scan.drives.find(d =>
      d.mount_point.replace(/\\$/, "") === scan.selectedDrive?.replace(/\\$/, "")
    );
    const usedSpace = drive ? drive.total_space - drive.available_space : 0;
    const pct = p && usedSpace > 0
      ? Math.min(100, (p.total_size / usedSpace) * 100)
      : 0;

    return (
      <footer className="h-6 px-4 flex items-center gap-3 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#C9A84C] font-mono shrink-0">
        <Loader2 className="w-3 h-3 animate-spin shrink-0" />
        <span className="truncate shrink-0 max-w-[180px]">
          扫描中
        </span>

        <div className="flex-1 h-1.5 bg-[#2A2A35] rounded-full overflow-hidden min-w-[60px] relative">
          {pct > 0 ? (
            <div
              className="h-full rounded-full bg-[#C9A84C] transition-[width] duration-500 ease-out relative overflow-hidden"
              style={{ width: `${pct}%` }}
            >
              <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent animate-[shimmer_1.5s_infinite]" />
            </div>
          ) : (
            <div className="h-full w-full bg-gradient-to-r from-transparent via-[#C9A84C]/40 to-transparent animate-[shimmer_1.5s_infinite]" />
          )}
        </div>

        <span className="text-[#C9A84C] shrink-0 min-w-[36px] text-right">
          {pct > 0 ? `${pct.toFixed(0)}%` : ""}
        </span>

        {p && (
          <span className="text-[#888899] flex gap-4 shrink-0">
            <span>文件 <span className="text-[#FAF8F5]">{formatNumber(p.files_scanned)}</span></span>
            <span>目录 <span className="text-[#FAF8F5]">{formatNumber(p.dirs_scanned)}</span></span>
            <span>总量 <span className="text-[#FAF8F5]">{formatSize(p.total_size)}</span></span>
          </span>
        )}
      </footer>
    );
  }

  if (scan.status === "done" && scan.progress) {
    const p = scan.progress;
    return (
      <footer className="h-6 px-4 flex items-center gap-2 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#4CAF50] font-mono shrink-0">
        <CheckCircle2 className="w-3 h-3" />
        <span className="flex-1">扫描完成</span>
        <span className="text-[#888899] flex gap-4 shrink-0">
          <span>文件 <span className="text-[#FAF8F5]">{formatNumber(p.files_scanned)}</span></span>
          <span>目录 <span className="text-[#FAF8F5]">{formatNumber(p.dirs_scanned)}</span></span>
          <span>总量 <span className="text-[#FAF8F5]">{formatSize(p.total_size)}</span></span>
        </span>
      </footer>
    );
  }

  return (
    <footer className="h-6 px-4 flex items-center gap-2 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#888899] font-mono shrink-0">
      <Activity className="w-3 h-3" />
      <span>就绪</span>
    </footer>
  );
}
