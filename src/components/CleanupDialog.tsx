import { useState, useEffect, useRef } from "react";
import type { DeleteResult } from "../types";
import { deleteItems } from "../lib/invoke";
import { formatSize } from "../lib/format";
import { TRASH_NAME } from "../lib/path";
import gsap from "gsap";
import { Trash2, AlertTriangle, CheckCircle2, XCircle, ArrowRight, Loader2 } from "lucide-react";

interface Props {
  selected: Map<string, number>;
  onDone: () => void;
  onClose: () => void;
  onCleanupDone: (bytes: number, path: string, action: "delete" | "empty") => void;
}

type Step = "review" | "confirm" | "done";

export function CleanupDialog({ selected, onDone, onClose, onCleanupDone }: Props) {
  const [step, setStep] = useState<Step>("review");
  const [toTrash, setToTrash] = useState(true);
  const [results, setResults] = useState<DeleteResult[]>([]);
  const [deleting, setDeleting] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);

  const paths = [...selected.keys()];
  const totalSize = [...selected.values()].reduce((a, b) => a + b, 0);

  useEffect(() => {
    if (dialogRef.current) {
      gsap.fromTo(dialogRef.current,
        { opacity: 0, y: 30, scale: 0.95 },
        { opacity: 1, y: 0, scale: 1, duration: 0.4, ease: "power3.out" }
      );
    }
  }, []);

  const closeDialog = () => {
    if (deleting) return; // 删除进行中禁止关闭，避免误触
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: onClose
      });
    } else {
      onClose();
    }
  };

  const finishDialog = () => {
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: onDone
      });
    } else {
      onDone();
    }
  };

  const handleDelete = async () => {
    if (deleting) return;
    setDeleting(true);
    try {
      const res = await deleteItems(paths, toTrash);
      setResults(res);
      for (const r of res) {
        if (r.success) {
          const size = selected.get(r.path) ?? 0;
          if (size > 0) onCleanupDone(size, r.path, "delete");
        }
      }
      setStep("done");
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-fiber/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-2xl rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-6 border-b border-slate flex items-center justify-between bg-abyss">
          <div className="flex items-center gap-3">
            <Trash2 className="w-5 h-5 text-danger" />
            <h2 className="text-lg font-semibold text-textMain">清理删除</h2>
          </div>
          <button className="text-text-dim hover:text-textMain transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="p-8 flex flex-col gap-6 max-h-[70vh] overflow-y-auto custom-scrollbar">
          {paths.length === 0 ? (
            <div className="text-text-dim font-mono text-center py-8">未选择任何项目</div>
          ) : (
            <>
              {step === "review" && (
                <div className="flex flex-col gap-6 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="flex items-end justify-between border-b mx-2 pb-4 border-slate/50">
                    <span className="text-text-dim font-mono uppercase tracking-widest text-xs">已选目标</span>
                    <div className="text-right">
                      <div className="text-2xl font-mono text-danger">{paths.length} <span className="text-sm text-textMain">个项目</span></div>
                      <div className="text-sm font-mono text-photon">{formatSize(totalSize)} 占用</div>
                    </div>
                  </div>

                  <div className="bg-[#000000]/30 rounded-xl border border-slate p-4 max-h-[30vh] overflow-y-auto custom-scrollbar font-mono text-xs">
                    {paths.map((p) => (
                      <div key={p} className="flex justify-between py-1.5 border-b border-slate/30 last:border-0 hover:bg-[#FFFFFF]/5 px-2 rounded -mx-2">
                        <span className="text-textMain truncate pr-4">{p}</span>
                        <span className="text-text-dim shrink-0">{formatSize(selected.get(p) ?? 0)}</span>
                      </div>
                    ))}
                  </div>

                  <label className="flex items-center gap-3 p-4 rounded-xl border border-slate cursor-pointer hover:bg-slate/30 transition-colors group">
                    <input
                      type="checkbox"
                      className="w-4 h-4 rounded border-slate bg-[#000] text-danger focus:ring-[#E74C3C] focus:ring-offset-0"
                      checked={toTrash}
                      onChange={(e) => setToTrash(e.target.checked)}
                    />
                    <div className="flex flex-col">
                      <span className="text-sm font-medium text-textMain group-hover:text-photon transition-colors">移至{TRASH_NAME}</span>
                      <span className="text-xs text-text-dim">推荐。取消勾选将永久删除，无法恢复。</span>
                    </div>
                  </label>

                  <div className="flex justify-end gap-4 mt-2">
                    <button className="magnetic-btn px-6 py-2 rounded-full border border-slate text-text-dim hover:text-textMain hover:bg-slate/50" onClick={closeDialog}>取消</button>
                    <button className="magnetic-btn px-6 py-2 rounded-full bg-[#E74C3C] text-fiber font-semibold flex items-center gap-2 hover:bg-[#c0392b]" onClick={() => setStep("confirm")}>
                      下一步 <ArrowRight className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              )}

              {step === "confirm" && (
                <div className="flex flex-col gap-6 items-center text-center py-4 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="w-16 h-16 rounded-full bg-danger/10 flex items-center justify-center mb-2">
                    <AlertTriangle className="w-8 h-8 text-danger" />
                  </div>
                  <div className="text-xl font-medium text-textMain">确认删除</div>
                  <div className="text-text-dim text-sm max-w-md">
                    即将删除 {paths.length} 个项目，释放 {formatSize(totalSize)} 空间。
                    <br /><br />
                    {toTrash ? (
                      <span className="text-photon">文件将移至{TRASH_NAME}，可恢复。</span>
                    ) : (
                      <span className="text-danger font-semibold">文件将被永久删除，无法恢复！</span>
                    )}
                  </div>

                  <div className="flex justify-center gap-4 mt-4 w-full">
                    <button
                      className="magnetic-btn flex-1 py-3 rounded-full border border-slate text-text-dim hover:text-textMain hover:bg-slate/50 disabled:opacity-40 disabled:hover:bg-transparent"
                      onClick={() => setStep("review")}
                      disabled={deleting}
                    >
                      返回
                    </button>
                    <button
                      className="magnetic-btn flex-1 py-3 rounded-full bg-[#E74C3C] text-fiber font-bold hover:bg-[#c0392b] hover:shadow-[0_0_20px_rgba(231,76,60,0.4)] transition-all disabled:opacity-70 disabled:hover:shadow-none flex items-center justify-center gap-2"
                      onClick={handleDelete}
                      disabled={deleting}
                    >
                      {deleting ? (
                        <><Loader2 className="w-4 h-4 animate-spin" /> 删除中…（大目录可能需要数分钟）</>
                      ) : (
                        "确认删除"
                      )}
                    </button>
                  </div>
                </div>
              )}

              {step === "done" && (
                <div className="flex flex-col gap-6 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="flex items-center justify-between border-b pb-4 border-slate/50">
                    <div className="flex items-center gap-2">
                      <CheckCircle2 className="w-6 h-6 text-success" />
                      <span className="text-textMain font-semibold">操作完成</span>
                    </div>
                    <div className="font-mono text-sm text-text-dim">
                      <span className="text-textMain">{results.filter((r) => r.success).length}</span> / {results.length} 成功
                    </div>
                  </div>

                  <div className="bg-[#000000]/30 rounded-xl border border-slate p-4 max-h-[30vh] overflow-y-auto custom-scrollbar font-mono text-xs">
                    {results.map((r) => (
                      <div key={r.path} className="flex flex-col py-2 border-b border-slate/30 last:border-0 hover:bg-[#FFFFFF]/5 px-2 rounded -mx-2">
                        <div className="flex justify-between items-start">
                          <span className={r.success ? "text-text-dim line-through" : "text-danger"}>{r.path}</span>
                          <span className="shrink-0 ml-4 font-bold">{r.success ? <span className="text-success">成功</span> : <span className="text-danger">失败</span>}</span>
                        </div>
                        {r.error && <span className="text-danger mt-1 opacity-80">{r.error}</span>}
                      </div>
                    ))}
                  </div>

                  <div className="flex justify-end mt-2">
                    <button className="magnetic-btn px-8 py-2 rounded-full bg-photon text-fiber font-bold hover:bg-[#D4B55C]" onClick={finishDialog}>完成</button>
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
