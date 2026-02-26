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
        <span className="truncate">SYSTEM ERROR: {scan.errorMsg}</span>
      </footer>
    );
  }

  if (scan.status === "scanning") {
    const p = scan.progress;
    return (
      <footer className="h-6 px-4 flex items-center gap-3 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#C9A84C] font-mono shrink-0">
        <Loader2 className="w-3 h-3 animate-spin" />
        <span className="truncate flex-1">
          ANALYZING::{scan.currentDir ? scan.currentDir : "INITIALIZING"}
        </span>
        {p && (
          <span className="text-[#888899] flex gap-4 shrink-0">
            <span>FILES <span className="text-[#FAF8F5]">{formatNumber(p.files_scanned)}</span></span>
            <span>DIRS <span className="text-[#FAF8F5]">{formatNumber(p.dirs_scanned)}</span></span>
            <span>VOL <span className="text-[#FAF8F5]">{formatSize(p.total_size)}</span></span>
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
        <span className="flex-1">ANALYSIS COMPLETE</span>
        <span className="text-[#888899] flex gap-4 shrink-0">
          <span>FILES <span className="text-[#FAF8F5]">{formatNumber(p.files_scanned)}</span></span>
          <span>DIRS <span className="text-[#FAF8F5]">{formatNumber(p.dirs_scanned)}</span></span>
          <span>VOL <span className="text-[#FAF8F5]">{formatSize(p.total_size)}</span></span>
        </span>
      </footer>
    );
  }

  return (
    <footer className="h-6 px-4 flex items-center gap-2 bg-[#0D0D12] border-t border-[#2A2A35] text-[10px] text-[#888899] font-mono shrink-0">
      <Activity className="w-3 h-3" />
      <span>SYSTEM STANDBY</span>
    </footer>
  );
}
