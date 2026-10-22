import type { DriveInfo, SnapshotInfo } from "../types";
import { formatSize, timeAgo } from "../lib/format";
import { normalizePath } from "../lib/path";
import { FolderOpen, HardDrive, History, Play, RotateCw } from "lucide-react";
import { cn } from "../lib/utils";

interface Props {
  drives: DriveInfo[];
  /** 各盘存下来的上次扫描 */
  snapshots: SnapshotInfo[];
  onScan: (mount: string) => void;
  onOpen: (mount: string) => void;
  onHistory: () => void;
}

/** 剩余空间低于该比例时用警示色 */
const LOW_SPACE = 0.1;

/** 还没打开扫描结果时的首页：每块磁盘一张卡片。扫过的盘直接打开上次的结果，没扫过的点了就扫 */
export function WelcomePanel({ drives, snapshots, onScan, onOpen, onHistory }: Props) {
  const snapshotOf = (mount: string) => snapshots.find((s) => normalizePath(s.root).toLowerCase() === normalizePath(mount).toLowerCase());
  return (
    <div className="h-full overflow-auto grid place-items-center p-6">
      <div className="w-full max-w-[760px]">
        <h1 className="text-xl font-semibold">选择要分析的磁盘</h1>
        <p className="mt-1 text-[13px] text-text-dim">
          扫描会读出整块磁盘里每个文件夹的大小，找出空间都被什么占用了。只读取文件信息，不会改动任何文件。扫过的盘会记住最近一次的结果，下次直接打开。
        </p>

        {drives.length === 0 ? (
          <div className="mt-6 p-6 rounded-lg bg-abyss border border-slate text-[13px] text-text-dim">
            没有找到可以扫描的磁盘。
          </div>
        ) : (
          <div className="mt-5 grid gap-3 grid-cols-[repeat(auto-fill,minmax(230px,1fr))]">
            {drives.map((d) => {
              const used = d.total_space - d.available_space;
              const ratio = d.total_space > 0 ? used / d.total_space : 0;
              const low = d.total_space > 0 && d.available_space / d.total_space < LOW_SPACE;
              const snap = snapshotOf(d.mount_point);
              return (
                <div key={d.mount_point} className="group relative rounded-lg bg-abyss border border-slate hover:bg-hover transition-colors">
                  <button
                    type="button"
                    className="w-full text-left p-4 rounded-lg active:bg-press"
                    title={snap ? "打开上次存下的扫描结果" : "扫描这块盘"}
                    onClick={() => (snap ? onOpen(d.mount_point) : onScan(d.mount_point))}
                  >
                    <div className="flex items-center gap-2">
                      <HardDrive className="w-5 h-5 text-text-dim" />
                      <span className="text-sm font-semibold">{d.mount_point}</span>
                      <span className="ml-auto flex items-center gap-1 text-xs text-photon opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
                        {snap ? <><FolderOpen className="w-3 h-3" />打开</> : <><Play className="w-3 h-3" />扫描</>}
                      </span>
                    </div>
                    <div className="mt-3 h-1.5 rounded-full bg-slate overflow-hidden">
                      <div className={cn("h-full rounded-full", low ? "bg-danger" : "bg-photon")} style={{ width: `${ratio * 100}%` }} />
                    </div>
                    <div className="mt-2 flex justify-between text-xs text-text-dim">
                      <span>已用 {formatSize(used)} / {formatSize(d.total_space)}</span>
                      <span className={cn(low && "text-danger")}>剩余 {formatSize(d.available_space)}</span>
                    </div>
                    <div className="mt-2 text-xs text-text-faint">{snap ? `${timeAgo(snap.scanned_at)}扫过` : "还没扫过"}</div>
                  </button>
                  {snap && (
                    <button
                      type="button"
                      className="absolute right-3 bottom-3 inline-flex items-center gap-1 h-6 px-1.5 rounded text-xs text-text-dim hover:text-textMain hover:bg-hover"
                      title="重新扫描这块盘，更新存下的结果"
                      onClick={() => onScan(d.mount_point)}
                    >
                      <RotateCw className="w-3 h-3" />重新扫描
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}

        <button type="button" className="mt-5 inline-flex items-center gap-1.5 text-[13px] text-photon hover:underline" onClick={onHistory}>
          <History className="w-4 h-4" />查看以前的清理记录
        </button>
      </div>
    </div>
  );
}
