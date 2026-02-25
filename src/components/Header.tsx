import type { ScanSession } from "../hooks/useScanSession";
import { formatSize } from "../lib/format";

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
    <header className="app-header">
      <h1 className="app-title">Disky</h1>

      <div className="header-controls">
        <select
          className="drive-select"
          value={scan.selectedDrive ?? ""}
          onChange={(e) => scan.setSelectedDrive(e.target.value || null)}
          disabled={scanning}
        >
          <option value="">-- 选择磁盘 --</option>
          {scan.drives.map((d) => (
            <option key={d.mount_point} value={d.mount_point}>
              {d.mount_point} (可用 {formatSize(d.available_space)} / 共 {formatSize(d.total_space)})
            </option>
          ))}
        </select>

        <button
          className="btn btn-primary"
          disabled={!scan.selectedDrive || scanning}
          onClick={scan.startScan}
        >
          {scanning ? "扫描中..." : "扫描"}
        </button>
      </div>

      <div className="header-actions">
        <button
          className="btn"
          disabled={!scan.rootPath}
          onClick={onHistory}
        >
          历史记录
        </button>
        <button
          className="btn"
          disabled={!canClean}
          onClick={onClean}
        >
          清理{cleanCount > 0 ? ` (${cleanCount})` : ""}
        </button>
        <button
          className={`btn${chatOpen ? " btn-primary" : ""}`}
          onClick={onToggleChat}
        >
          AI 助手
        </button>
      </div>
    </header>
  );
}
