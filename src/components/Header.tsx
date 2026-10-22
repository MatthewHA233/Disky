import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ScanSession } from "../hooks/useScanSession";
import { formatSize } from "../lib/format";
import { HardDrive, History, Trash2, Play, Tag, X, ChevronDown, Loader2 } from "lucide-react";
import { AiIcon } from "./ui/AiIcon";
import { cn } from "../lib/utils";

interface Props {
  scan: ScanSession;
  cleanedBytes?: number;
  onHistory: () => void;
  onClean: () => void;
  cleanCount: number;
  onClearSelected: () => void;
  onToggleChat: () => void;
  chatOpen: boolean;
  viewMode: "tree" | "tags" | "archive";
  /** 档案馆里有别的盘的数据：没有当前扫描也能切到档案馆 */
  hasArchive: boolean;
  onToggleViewMode: () => void;
  onSelectView: (view: "tree" | "archive") => void;
  analyzing?: boolean;
  onToggleAnalyzeLog?: () => void;
  analyzeLogOpen?: boolean;
}

/** 命令栏右侧的图标按钮（Fluent 32×32 subtle button） */
function IconButton({ title, active, disabled, danger, onClick, children }: {
  title: string;
  active?: boolean;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "relative w-8 h-8 grid place-items-center rounded transition-colors",
        "disabled:opacity-40 disabled:pointer-events-none",
        active ? "bg-hover text-photon" : "text-text-dim hover:bg-hover hover:text-textMain active:bg-press",
        danger && !active && "hover:text-danger"
      )}
    >
      {children}
    </button>
  );
}

const VIEWS: { id: "archive" | "tree"; label: string }[] = [
  { id: "archive", label: "档案馆" },
  { id: "tree", label: "目录" },
];

export function Header({ scan, cleanedBytes = 0, onHistory, onClean, cleanCount, onClearSelected, onToggleChat, chatOpen, viewMode, hasArchive, onToggleViewMode, onSelectView, analyzing = false, onToggleAnalyzeLog, analyzeLogOpen = false }: Props) {
  const scanning = scan.status === "scanning";
  const canClean = scan.status === "done" && cleanCount > 0;
  const [driveOpen, setDriveOpen] = useState(false);
  const driveRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!driveOpen) return;
    const handler = (e: MouseEvent) => {
      if (driveRef.current && !driveRef.current.contains(e.target as Node)) setDriveOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setDriveOpen(false); };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", onKey);
    };
  }, [driveOpen]);

  const driveLabel = (mount: string, withCleaned: boolean) => {
    const d = scan.drives.find((x) => x.mount_point === mount);
    if (!d) return { name: mount, free: "" };
    const cleaned = withCleaned && cleanedBytes > 0 ? ` +${formatSize(cleanedBytes)}` : "";
    return { name: d.mount_point, free: `${formatSize(d.available_space)}${cleaned} 可用` };
  };
  const current = scan.selectedDrive ? driveLabel(scan.selectedDrive, true) : null;

  return (
    <header className="shrink-0 h-12 w-full px-3 flex items-center gap-3 z-50 select-none" data-tauri-drag-region>
      <div className="flex items-center gap-2 pl-1 pr-2 shrink-0" data-tauri-drag-region>
        <img src="/icon.png" alt="" className="w-5 h-5 pointer-events-none" draggable={false} />
        <span className="text-sm font-semibold pointer-events-none">Disky</span>
      </div>

      {/* 磁盘选择（Fluent ComboBox） */}
      <div ref={driveRef} className="relative w-[240px] shrink-0">
        <button
          type="button"
          className={cn(
            "w-full h-8 flex items-center gap-2 px-3 rounded text-[13px] text-left transition-colors",
            "bg-card border border-slate border-b-[color:var(--fl-stroke-strong)] hover:bg-hover",
            "disabled:opacity-50 disabled:pointer-events-none"
          )}
          disabled={scanning}
          aria-haspopup="listbox"
          aria-expanded={driveOpen}
          onClick={() => setDriveOpen((v) => !v)}
        >
          <HardDrive className="w-4 h-4 shrink-0 text-text-dim" />
          {current ? (
            <span className="flex-1 truncate">{current.name}<span className="text-text-dim ml-2">{current.free}</span></span>
          ) : (
            <span className="flex-1 truncate text-text-dim">选择磁盘</span>
          )}
          <ChevronDown className={cn("w-4 h-4 shrink-0 text-text-dim transition-transform", driveOpen && "rotate-180")} />
        </button>

        {driveOpen && (
          <div role="listbox" className="absolute top-full left-0 mt-1 w-full p-1 rounded-lg bg-card border border-slate z-[200]" style={{ boxShadow: "var(--fl-shadow)" }}>
            {scan.drives.length === 0 && <div className="px-3 py-2 text-[13px] text-text-dim">没有可用的磁盘</div>}
            {scan.drives.map((d) => {
              const isCurrent = d.mount_point === scan.selectedDrive;
              const label = driveLabel(d.mount_point, isCurrent);
              return (
                <button
                  key={d.mount_point}
                  type="button"
                  role="option"
                  aria-selected={isCurrent}
                  className={cn(
                    "relative w-full h-9 flex items-center gap-2 px-3 rounded text-[13px] text-left transition-colors hover:bg-hover",
                    isCurrent && "bg-hover"
                  )}
                  onClick={() => { scan.setSelectedDrive(d.mount_point); setDriveOpen(false); }}
                >
                  {isCurrent && <span className="absolute left-0 top-2.5 bottom-2.5 w-[3px] rounded-full bg-photon" />}
                  <HardDrive className="w-4 h-4 shrink-0 text-text-dim" />
                  <span className="truncate">{label.name}<span className="text-text-dim ml-2">{label.free}</span></span>
                </button>
              );
            })}
          </div>
        )}
      </div>

      <button
        type="button"
        className="h-8 flex items-center gap-1.5 px-4 rounded text-[13px] font-medium bg-photon text-on-accent hover:bg-photon/90 active:bg-photon/80 disabled:opacity-40 disabled:pointer-events-none transition-colors"
        disabled={!scan.selectedDrive || scanning}
        onClick={scan.startScan}
      >
        {scanning ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
        {scanning ? "扫描中" : "扫描"}
      </button>

      {/* 视图切换（Fluent SelectorBar：选中项下方一条强调色短线） */}
      {(scan.rootPath || hasArchive) && !scanning && (
        <nav className="flex items-center gap-0.5 ml-2" role="tablist" aria-label="视图">
          {VIEWS.map((v) => {
            const active = viewMode === v.id;
            return (
              <button
                key={v.id}
                type="button"
                role="tab"
                aria-selected={active}
                className={cn(
                  "relative h-8 px-3 rounded text-[13px] transition-colors hover:bg-hover",
                  active ? "text-textMain font-semibold" : "text-text-dim hover:text-textMain"
                )}
                onClick={() => onSelectView(v.id)}
              >
                {v.label}
                {active && <span className="absolute left-1/2 bottom-0.5 w-4 h-[3px] -ml-2 rounded-full bg-photon" />}
              </button>
            );
          })}
        </nav>
      )}

      <div className="flex-1 h-full" data-tauri-drag-region />

      {cleanCount > 0 && (
        <div className="flex items-center gap-1 h-7 pl-3 pr-1 rounded-full bg-photon/15 text-photon text-xs shrink-0">
          <span>已选 <b className="font-semibold">{cleanCount}</b> 项</span>
          <button type="button" className="w-5 h-5 grid place-items-center rounded-full hover:bg-photon/20" onClick={onClearSelected} title="取消全部选择" aria-label="取消全部选择">
            <X className="w-3 h-3" />
          </button>
        </div>
      )}

      <div className="flex items-center gap-0.5 shrink-0">
        <IconButton title="清理记录" onClick={onHistory}><History className="w-4 h-4" /></IconButton>
        <IconButton title="标签看板" active={viewMode === "tags"} disabled={scanning || !scan.rootPath} onClick={onToggleViewMode}>
          <Tag className="w-4 h-4" />
        </IconButton>
        <IconButton title="清理选中的项目" danger disabled={!canClean} onClick={onClean}>
          <Trash2 className="w-4 h-4" />
          {cleanCount > 0 && (
            <span className="absolute top-0.5 right-0.5 min-w-3.5 h-3.5 px-0.5 rounded-full bg-danger text-on-accent text-[9px] leading-[14px] text-center">{cleanCount}</span>
          )}
        </IconButton>
        {(analyzing || analyzeLogOpen) && onToggleAnalyzeLog && (
          <IconButton title="AI 分析实时输出" active={analyzeLogOpen} onClick={onToggleAnalyzeLog}>
            <AiIcon className="w-4 h-4" />
            {analyzing && <span className="absolute top-1.5 right-1.5 w-1.5 h-1.5 rounded-full bg-photon animate-pulse" />}
          </IconButton>
        )}
        <IconButton title="AI 助手" active={chatOpen} onClick={onToggleChat}><AiIcon className="w-4 h-4" /></IconButton>
      </div>
    </header>
  );
}
