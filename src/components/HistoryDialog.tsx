import { useEffect, useState, useRef } from "react";
import type { ScanRecord, DiffEntry, DirEntry } from "../types";
import { listScans, saveScan, loadScan, deleteScan, compareScans } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import gsap from "gsap";
import { History, Save, Database, ArrowRight, ArrowDown, ArrowUp, XCircle, Search, Trash2, GitCompare } from "lucide-react";
import { cn } from "../lib/utils";

interface Props {
  drive: string | null;
  onClose: () => void;
  onLoad: (rootPath: string, children: DirEntry[]) => void;
}

export function HistoryDialog({ drive, onClose, onLoad }: Props) {
  const [scans, setScans] = useState<ScanRecord[]>([]);
  const [idA, setIdA] = useState<number | null>(null);
  const [idB, setIdB] = useState<number | null>(null);
  const [diffs, setDiffs] = useState<DiffEntry[]>([]);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (dialogRef.current) {
      gsap.fromTo(dialogRef.current,
        { opacity: 0, y: 30, scale: 0.95 },
        { opacity: 1, y: 0, scale: 1, duration: 0.4, ease: "power3.out" }
      );
    }
  }, []);

  const closeDialog = () => {
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: onClose
      });
    } else {
      onClose();
    }
  };

  const refresh = () => listScans().then(setScans).catch((e) => setMsg(String(e)));

  useEffect(() => { refresh(); }, []);

  const handleSave = async () => {
    if (!drive) return;
    setBusy(true);
    setMsg("正在保存...");
    try {
      const id = await saveScan(drive);
      setMsg(`已保存为 #[${id}]`);
      refresh();
    } catch (e) {
      setMsg(`保存失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleLoad = async (id: number) => {
    setBusy(true);
    setMsg("正在恢复...");
    try {
      const result = await loadScan(id);
      if (dialogRef.current) {
        gsap.to(dialogRef.current, {
          opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: () => onLoad(result.root_path, result.children)
        });
      } else {
        onLoad(result.root_path, result.children);
      }
    } catch (e) {
      setMsg(`恢复失败: ${e}`);
      setBusy(false);
    }
  };

  const handleDelete = async (id: number) => {
    setBusy(true);
    setMsg("正在删除...");
    try {
      await deleteScan(id);
      setMsg("已删除");
      if (idA === id) setIdA(null);
      if (idB === id) setIdB(null);
      refresh();
    } catch (e) {
      setMsg(`删除失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const handleCompare = async () => {
    if (idA == null || idB == null || !drive) return;
    setMsg("正在分析差异...");
    try {
      const res = await compareScans(idA, idB, drive);
      setDiffs(res);
      setMsg(`发现 ${res.length} 处差异`);
    } catch (e) {
      setMsg(`分析失败: ${e}`);
    }
  };

  return (
    <div className="fixed inset-0 bg-[#0D0D12]/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-4xl rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)] max-h-[85vh]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-5 border-b border-[#2A2A35] flex items-center justify-between bg-[#13131A] shrink-0">
          <div className="flex items-center gap-3">
            <History className="w-5 h-5 text-[#C9A84C]" />
            <h2 className="text-lg font-semibold text-[#FAF8F5]">历史记录</h2>
          </div>
          <button className="text-[#888899] hover:text-[#FAF8F5] transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="flex flex-col flex-1 min-h-0 bg-[#0A0A12]">
          <div className="p-6 border-b border-[#2A2A35] flex items-center justify-between bg-[#13131A]/50 shrink-0">
            <div className="flex flex-col">
              <span className="text-xs font-mono text-[#888899] uppercase tracking-widest">当前磁盘</span>
              <span className="text-[#FAF8F5] font-mono">{drive || "无"}</span>
            </div>
            {drive && (
              <button
                className="magnetic-btn flex items-center gap-2 px-6 py-2 rounded-full bg-[#C9A84C] text-[#0D0D12] font-semibold text-sm hover:bg-[#D4B55C] disabled:opacity-50"
                onClick={handleSave}
                disabled={busy}
              >
                <Save className="w-4 h-4" /> 保存当前状态
              </button>
            )}
          </div>

          <div className="flex flex-1 min-h-0">
            {/* Left Col: Scans */}
            <div className="w-1/2 flex flex-col border-r border-[#2A2A35] bg-[#0A0A12]">
              <div className="px-6 py-3 border-b border-[#2A2A35]/50 bg-[#13131A] text-xs font-mono text-[#888899] uppercase tracking-widest flex items-center gap-2 shrink-0">
                <Database className="w-3 h-3" /> 已保存记录
              </div>
              <div className="flex-1 overflow-y-auto custom-scrollbar p-4 flex flex-col gap-3">
                {scans.length === 0 && <div className="text-center py-8 font-mono text-sm text-[#888899]">暂无记录</div>}
                {scans.map((s) => (
                  <div key={s.id} className="border border-[#2A2A35] rounded-xl p-4 bg-[#13131A] hover:border-[#C9A84C]/50 transition-colors group">
                    <div className="flex items-center justify-between mb-3 border-b border-[#2A2A35]/50 pb-2">
                      <div className="flex items-center gap-2 font-mono text-sm text-[#FAF8F5]">
                        <span className="text-[#C9A84C]">#[{s.id}]</span> {s.drive}
                      </div>
                      <div className="text-xs text-[#888899] font-mono">{s.created_at}</div>
                    </div>

                    <div className="flex items-center gap-4 mb-4 text-xs font-mono text-[#888899]">
                      <span><span className="text-[#FAF8F5]">{formatNumber(s.entry_count)}</span> 条目</span>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-3 bg-[#0A0A12] p-1 rounded-lg border border-[#2A2A35]">
                        <label className="flex items-center gap-1.5 cursor-pointer px-2 py-1 rounded hover:bg-[#2A2A35]/50 text-xs font-mono">
                          <input type="radio" className="focus:ring-0 text-[#C9A84C] bg-[#13131A] border-[#2A2A35]" name="scanA" checked={idA === s.id} onChange={() => setIdA(s.id)} />
                          <span className={idA === s.id ? "text-[#C9A84C]" : "text-[#FAF8F5]"}>A</span>
                        </label>
                        <label className="flex items-center gap-1.5 cursor-pointer px-2 py-1 rounded hover:bg-[#2A2A35]/50 text-xs font-mono">
                          <input type="radio" className="focus:ring-0 text-[#C9A84C] bg-[#13131A] border-[#2A2A35]" name="scanB" checked={idB === s.id} onChange={() => setIdB(s.id)} />
                          <span className={idB === s.id ? "text-[#C9A84C]" : "text-[#FAF8F5]"}>B</span>
                        </label>
                      </div>

                      <div className="flex gap-2">
                        {s.has_tree && (
                          <button className="p-2 rounded bg-[#2A2A35]/50 text-[#FAF8F5] hover:bg-[#C9A84C] hover:text-[#0D0D12] transition-colors" disabled={busy} onClick={() => handleLoad(s.id)} title="恢复">
                            <ArrowRight className="w-4 h-4" />
                          </button>
                        )}
                        <button className="p-2 rounded bg-[#2A2A35]/50 text-[#888899] hover:bg-[#E74C3C] hover:text-[#0D0D12] transition-colors" disabled={busy} onClick={() => handleDelete(s.id)} title="删除">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Right Col: Compare */}
            <div className="w-1/2 flex flex-col bg-[#0D0D12]">
              <div className="px-6 py-3 border-b border-[#2A2A35]/50 bg-[#13131A] text-xs font-mono text-[#888899] uppercase tracking-widest flex items-center justify-between shrink-0">
                <div className="flex items-center gap-2">
                  <GitCompare className="w-3 h-3" /> 差异对比
                </div>
                {(idA != null && idB != null) && (
                  <button
                    className="magnetic-btn flex items-center gap-2 text-[#C9A84C] hover:text-[#FAF8F5] bg-[#C9A84C]/10 hover:bg-[#C9A84C]/20 px-3 py-1 rounded transition-colors"
                    onClick={handleCompare}
                    disabled={busy}
                  >
                    <Search className="w-3 h-3" /> 开始对比
                  </button>
                )}
              </div>

              <div className="flex-1 overflow-y-auto custom-scrollbar p-4 flex flex-col gap-2 relative">
                {idA == null || idB == null ? (
                  <div className="absolute inset-0 flex flex-col items-center justify-center text-[#888899] font-mono text-sm">
                    <GitCompare className="w-8 h-8 opacity-20 mb-4" />
                    选择 A 和 B 开始对比
                  </div>
                ) : diffs.length === 0 ? (
                  <div className="absolute inset-0 flex items-center justify-center text-[#888899] font-mono text-sm">
                    无差异
                  </div>
                ) : (
                  diffs.map((d) => (
                    <div key={d.path} className={cn(
                      "flex flex-col p-3 rounded-xl border bg-[#13131A]",
                      d.diff > 0 ? "border-[#E74C3C]/30" : "border-[#4CAF50]/30"
                    )}>
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-[#FAF8F5] text-sm truncate pr-4">{d.name}</span>
                        <div className={cn("flex items-center gap-1 font-mono text-xs font-bold shrink-0", d.diff > 0 ? "text-[#E74C3C]" : "text-[#4CAF50]")}>
                          {d.diff > 0 ? <ArrowUp className="w-3 h-3" /> : <ArrowDown className="w-3 h-3" />}
                          {formatSize(Math.abs(d.diff))}
                        </div>
                      </div>
                      <div className="flex items-center gap-2 text-xs font-mono text-[#888899]">
                        <span className="truncate flex-1" title={d.path}>{d.path}</span>
                        <span className="shrink-0 flex items-center gap-2">
                          {formatSize(Math.abs(d.old_size))} <ArrowRight className="w-3 h-3 opacity-50" /> {formatSize(Math.abs(d.new_size))}
                        </span>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </div>
          </div>

          {msg && (
            <div className="px-6 py-2 border-t border-[#2A2A35] bg-[#13131A] text-xs font-mono shrink-0 flex items-center gap-2 text-[#C9A84C]">
              <span className="w-2 h-2 rounded-full bg-[#C9A84C] animate-pulse" />
              {msg}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
