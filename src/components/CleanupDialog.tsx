import { useState, useEffect, useRef } from "react";
import type { DeleteResult } from "../types";
import { deleteItems } from "../lib/invoke";
import { formatSize } from "../lib/format";
import gsap from "gsap";
import { Trash2, AlertTriangle, CheckCircle2, XCircle, ArrowRight } from "lucide-react";

interface Props {
  selected: Map<string, number>;
  onDone: () => void;
  onClose: () => void;
}

type Step = "review" | "confirm" | "done";

export function CleanupDialog({ selected, onDone, onClose }: Props) {
  const [step, setStep] = useState<Step>("review");
  const [toTrash, setToTrash] = useState(true);
  const [results, setResults] = useState<DeleteResult[]>([]);
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
    const res = await deleteItems(paths, toTrash);
    setResults(res);
    setStep("done");
  };

  return (
    <div className="fixed inset-0 bg-[#0D0D12]/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-2xl rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-6 border-b border-[#2A2A35] flex items-center justify-between bg-[#13131A]">
          <div className="flex items-center gap-3">
            <Trash2 className="w-5 h-5 text-[#E74C3C]" />
            <h2 className="text-lg font-semibold text-[#FAF8F5]">STRUCTURAL ERADICATION</h2>
          </div>
          <button className="text-[#888899] hover:text-[#FAF8F5] transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="p-8 flex flex-col gap-6 max-h-[70vh] overflow-y-auto custom-scrollbar">
          {paths.length === 0 ? (
            <div className="text-[#888899] font-mono text-center py-8">No specific targets engaged.</div>
          ) : (
            <>
              {step === "review" && (
                <div className="flex flex-col gap-6 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="flex items-end justify-between border-b mx-2 pb-4 border-[#2A2A35]/50">
                    <span className="text-[#888899] font-mono uppercase tracking-widest text-xs">Targets Acquired</span>
                    <div className="text-right">
                      <div className="text-2xl font-mono text-[#E74C3C]">{paths.length} <span className="text-sm text-[#FAF8F5]">ENTITIES</span></div>
                      <div className="text-sm font-mono text-[#C9A84C]">{formatSize(totalSize)} ALLOCATED</div>
                    </div>
                  </div>

                  <div className="bg-[#000000]/30 rounded-xl border border-[#2A2A35] p-4 max-h-[30vh] overflow-y-auto custom-scrollbar font-mono text-xs">
                    {paths.map((p) => (
                      <div key={p} className="flex justify-between py-1.5 border-b border-[#2A2A35]/30 last:border-0 hover:bg-[#FFFFFF]/5 px-2 rounded -mx-2">
                        <span className="text-[#FAF8F5] truncate pr-4">{p}</span>
                        <span className="text-[#888899] shrink-0">{formatSize(selected.get(p) ?? 0)}</span>
                      </div>
                    ))}
                  </div>

                  <label className="flex items-center gap-3 p-4 rounded-xl border border-[#2A2A35] cursor-pointer hover:bg-[#2A2A35]/30 transition-colors group">
                    <input
                      type="checkbox"
                      className="w-4 h-4 rounded border-[#2A2A35] bg-[#000] text-[#E74C3C] focus:ring-[#E74C3C] focus:ring-offset-0"
                      checked={toTrash}
                      onChange={(e) => setToTrash(e.target.checked)}
                    />
                    <div className="flex flex-col">
                      <span className="text-sm font-medium text-[#FAF8F5] group-hover:text-[#C9A84C] transition-colors">Move to Recycle Bin</span>
                      <span className="text-xs text-[#888899]">Recommended safety protocol. Uncheck for permanent deletion.</span>
                    </div>
                  </label>

                  <div className="flex justify-end gap-4 mt-2">
                    <button className="magnetic-btn px-6 py-2 rounded-full border border-[#2A2A35] text-[#888899] hover:text-[#FAF8F5] hover:bg-[#2A2A35]/50" onClick={closeDialog}>CANCEL</button>
                    <button className="magnetic-btn px-6 py-2 rounded-full bg-[#E74C3C] text-[#0D0D12] font-semibold flex items-center gap-2 hover:bg-[#c0392b]" onClick={() => setStep("confirm")}>
                      PROCEED <ArrowRight className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              )}

              {step === "confirm" && (
                <div className="flex flex-col gap-6 items-center text-center py-4 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="w-16 h-16 rounded-full bg-[#E74C3C]/10 flex items-center justify-center mb-2">
                    <AlertTriangle className="w-8 h-8 text-[#E74C3C]" />
                  </div>
                  <div className="text-xl font-medium text-[#FAF8F5]">Confirm Eradication</div>
                  <div className="text-[#888899] text-sm max-w-md">
                    You are about to eliminate {paths.length} entities freeing {formatSize(totalSize)} of space.
                    <br /><br />
                    {toTrash ? (
                      <span className="text-[#C9A84C]">Entities will be transferred to the Recycle Bin.</span>
                    ) : (
                      <span className="text-[#E74C3C] font-semibold">Entities will be permanently purged from the physical drive. They cannot be recovered!</span>
                    )}
                  </div>

                  <div className="flex justify-center gap-4 mt-4 w-full">
                    <button className="magnetic-btn flex-1 py-3 rounded-full border border-[#2A2A35] text-[#888899] hover:text-[#FAF8F5] hover:bg-[#2A2A35]/50" onClick={() => setStep("review")}>ABORT</button>
                    <button className="magnetic-btn flex-1 py-3 rounded-full bg-[#E74C3C] text-[#0D0D12] font-bold hover:bg-[#c0392b] hover:shadow-[0_0_20px_rgba(231,76,60,0.4)] transition-all" onClick={handleDelete}>CONFIRM DELETION</button>
                  </div>
                </div>
              )}

              {step === "done" && (
                <div className="flex flex-col gap-6 animate-in fade-in slide-in-from-right-4 duration-300">
                  <div className="flex items-center justify-between border-b pb-4 border-[#2A2A35]/50">
                    <div className="flex items-center gap-2">
                      <CheckCircle2 className="w-6 h-6 text-[#4CAF50]" />
                      <span className="text-[#FAF8F5] font-semibold">OPERATION COMPLETE</span>
                    </div>
                    <div className="font-mono text-sm text-[#888899]">
                      <span className="text-[#FAF8F5]">{results.filter((r) => r.success).length}</span> / {results.length} SUCCESS
                    </div>
                  </div>

                  <div className="bg-[#000000]/30 rounded-xl border border-[#2A2A35] p-4 max-h-[30vh] overflow-y-auto custom-scrollbar font-mono text-xs">
                    {results.map((r) => (
                      <div key={r.path} className="flex flex-col py-2 border-b border-[#2A2A35]/30 last:border-0 hover:bg-[#FFFFFF]/5 px-2 rounded -mx-2">
                        <div className="flex justify-between items-start">
                          <span className={r.success ? "text-[#888899] line-through" : "text-[#E74C3C]"}>{r.path}</span>
                          <span className="shrink-0 ml-4 font-bold">{r.success ? <span className="text-[#4CAF50]">OK</span> : <span className="text-[#E74C3C]">FAIL</span>}</span>
                        </div>
                        {r.error && <span className="text-[#E74C3C] mt-1 opacity-80">{r.error}</span>}
                      </div>
                    ))}
                  </div>

                  <div className="flex justify-end mt-2">
                    <button className="magnetic-btn px-8 py-2 rounded-full bg-[#C9A84C] text-[#0D0D12] font-bold hover:bg-[#D4B55C]" onClick={finishDialog}>FINISH</button>
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
