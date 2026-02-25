import type { ScanSession } from "../hooks/useScanSession";
import { formatSize, formatNumber } from "../lib/format";

interface Props {
  scan: ScanSession;
}

export function StatusBar({ scan }: Props) {
  if (scan.status === "error") {
    return (
      <footer className="status-bar status-error">
        <span className="status-indicator error" />
        <span>错误: {scan.errorMsg}</span>
      </footer>
    );
  }

  if (scan.status === "scanning") {
    const p = scan.progress;
    return (
      <footer className="status-bar">
        <span className="status-indicator scanning" />
        <span>
          {scan.currentDir
            ? `正在扫描 "${scan.currentDir}"...`
            : "正在扫描..."}
        </span>
        {p && (
          <span className="status-stats">
            {formatNumber(p.files_scanned)} 个文件 | {formatNumber(p.dirs_scanned)} 个目录 | {formatSize(p.total_size)}
          </span>
        )}
      </footer>
    );
  }

  if (scan.status === "done" && scan.progress) {
    const p = scan.progress;
    return (
      <footer className="status-bar">
        <span className="status-indicator done" />
        <span>扫描完成</span>
        <span className="status-stats">
          {formatNumber(p.files_scanned)} 个文件 | {formatNumber(p.dirs_scanned)} 个目录 | {formatSize(p.total_size)}
        </span>
      </footer>
    );
  }

  return (
    <footer className="status-bar">
      <span className="status-indicator idle" />
      <span>就绪</span>
    </footer>
  );
}
