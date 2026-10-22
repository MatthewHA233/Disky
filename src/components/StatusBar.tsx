import type { ScanSession } from "../hooks/useScanSession";
import { formatSize, formatNumber, timeAgo } from "../lib/format";
import { normalizePath } from "../lib/path";
import { CheckCircle2, XCircle, Loader2, Activity, History, RotateCw } from "lucide-react";

interface Props {
  scan: ScanSession;
}

export function StatusBar({ scan }: Props) {
  if (scan.status === "error") {
    return (
      <footer className="h-7 px-4 flex items-center gap-2 bg-fiber border-t border-slate text-xs text-danger shrink-0">
        <XCircle className="w-3 h-3" />
        <span className="truncate">错误: {scan.errorMsg}</span>
      </footer>
    );
  }

  if (scan.status === "scanning") {
    const p = scan.progress;
    const drive = scan.drives.find(d =>
      scan.selectedDrive != null &&
      normalizePath(d.mount_point) === normalizePath(scan.selectedDrive)
    );
    const usedSpace = drive ? drive.total_space - drive.available_space : 0;
    const pct = p && usedSpace > 0
      ? Math.min(100, (p.total_size / usedSpace) * 100)
      : 0;

    return (
      <footer className="h-7 px-4 flex items-center gap-3 bg-fiber border-t border-slate text-xs text-photon shrink-0">
        <Loader2 className="w-3 h-3 animate-spin shrink-0" />
        <span className="truncate shrink-0 max-w-[180px]">
          扫描中
        </span>

        <div className="flex-1 h-1.5 bg-slate rounded-full overflow-hidden min-w-[60px] max-w-[320px] relative">
          {pct > 0 ? (
            <div
              className="h-full rounded bg-photon transition-[width] duration-500 ease-out relative overflow-hidden"
              style={{ width: `${pct}%` }}
            >
              <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent animate-[shimmer_1.5s_infinite]" />
            </div>
          ) : (
            <div className="h-full w-full bg-gradient-to-r from-transparent via-photon/40 to-transparent animate-[shimmer_1.5s_infinite]" />
          )}
        </div>

        <span className="text-photon shrink-0 min-w-[36px] text-right">
          {pct > 0 ? `${pct.toFixed(0)}%` : ""}
        </span>

        {p && (
          <span className="text-text-dim flex gap-4 shrink-0">
            <span>文件 <span className="text-textMain">{formatNumber(p.files_scanned)}</span></span>
            <span>目录 <span className="text-textMain">{formatNumber(p.dirs_scanned)}</span></span>
            <span>总量 <span className="text-textMain">{formatSize(p.total_size)}</span></span>
          </span>
        )}
      </footer>
    );
  }

  // 打开的是存下来的扫描：说清楚是什么时候的，随时能重扫
  if (scan.status === "done" && scan.restoredAt != null) {
    return (
      <footer className="h-7 px-4 flex items-center gap-2 bg-fiber border-t border-slate text-xs text-text-dim shrink-0">
        <History className="w-3 h-3" />
        <span className="flex-1 truncate">打开的是 {timeAgo(scan.restoredAt)}存下的扫描结果，之后的变化要重新扫描才看得到</span>
        <button type="button" className="shrink-0 inline-flex items-center gap-1 text-photon hover:underline" onClick={scan.startScan}>
          <RotateCw className="w-3 h-3" />重新扫描
        </button>
      </footer>
    );
  }

  if (scan.status === "done" && scan.progress) {
    const p = scan.progress;
    return (
      <footer className="h-7 px-4 flex items-center gap-2 bg-fiber border-t border-slate text-xs text-success shrink-0">
        <CheckCircle2 className="w-3 h-3" />
        <span className="flex-1">扫描完成</span>
        <span className="text-text-dim flex gap-4 shrink-0">
          <span>文件 <span className="text-textMain">{formatNumber(p.files_scanned)}</span></span>
          <span>目录 <span className="text-textMain">{formatNumber(p.dirs_scanned)}</span></span>
          <span>总量 <span className="text-textMain">{formatSize(p.total_size)}</span></span>
        </span>
      </footer>
    );
  }

  return (
    <footer className="h-7 px-4 flex items-center gap-2 bg-fiber border-t border-slate text-xs text-text-dim shrink-0">
      <Activity className="w-3 h-3" />
      <span>就绪</span>
    </footer>
  );
}
