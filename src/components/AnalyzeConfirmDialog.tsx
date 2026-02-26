import { useState, useEffect, useRef } from "react";
import type { AiAnalysis } from "../types";
import { StarRating } from "./StarRating";
import gsap from "gsap";
import { AlertCircle, XCircle, FileType, CheckCircle2, FastForward } from "lucide-react";

interface Props {
  duplicates: AiAnalysis[];
  onConfirm: (pathsToReplace: string[]) => void;
  onClose: () => void;
}

export function AnalyzeConfirmDialog({ duplicates, onConfirm, onClose }: Props) {
  const [currentIndex, setCurrentIndex] = useState(0);
  const [pathsToReplace, setPathsToReplace] = useState<string[]>([]);
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

  if (duplicates.length === 0) {
    onConfirm([]);
    return null;
  }

  const current = duplicates[currentIndex];
  const remaining = duplicates.length - currentIndex;

  const advance = (replace: boolean) => {
    const next = replace ? [...pathsToReplace, current.path] : pathsToReplace;
    if (currentIndex + 1 >= duplicates.length) {
      if (dialogRef.current) {
        gsap.to(dialogRef.current, {
          opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: () => onConfirm(next)
        });
      } else {
        onConfirm(next);
      }
    } else {
      setPathsToReplace(next);
      setCurrentIndex(currentIndex + 1);
    }
  };

  const handleReplaceAll = () => {
    const remainingPaths = duplicates.slice(currentIndex).map((d) => d.path);
    const next = [...pathsToReplace, ...remainingPaths];
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: () => onConfirm(next)
      });
    } else {
      onConfirm(next);
    }
  };

  const handleSkipAll = () => {
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: () => onConfirm(pathsToReplace)
      });
    } else {
      onConfirm(pathsToReplace);
    }
  };

  return (
    <div className="fixed inset-0 bg-[#0D0D12]/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-lg rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-6 border-b border-[#2A2A35] flex items-center justify-between bg-[#13131A]">
          <div className="flex items-center gap-3">
            <AlertCircle className="w-5 h-5 text-[#C9A84C]" />
            <h2 className="text-lg font-semibold text-[#FAF8F5]">PRECEDING ANALYSIS DETECTED</h2>
          </div>
          <button className="text-[#888899] hover:text-[#FAF8F5] transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="p-8 flex flex-col gap-6">
          <div className="flex items-center justify-between text-xs font-mono text-[#888899] uppercase tracking-widest border-b border-[#2A2A35]/50 pb-2">
            <span>Conflict Query</span>
            <span className="text-[#C9A84C] font-bold text-sm">{currentIndex + 1} / {duplicates.length}</span>
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex flex-col p-4 bg-[#0A0A12] border border-[#2A2A35] rounded-xl relative overflow-hidden group">
              <FileType className="absolute -right-4 -top-4 w-24 h-24 text-[#C9A84C] opacity-5 group-hover:opacity-10 transition-opacity" />
              <div className="text-xs font-mono text-[#888899] mb-1">TARGET ENTITY</div>
              <div className="text-sm font-mono text-[#C9A84C] break-all">{current.path}</div>

              <div className="mt-4 pt-4 border-t border-[#2A2A35]/50">
                <div className="text-xs font-mono text-[#888899] mb-2 uppercase">Current Designation</div>
                <div className="text-sm text-[#FAF8F5] mb-4">{current.description}</div>

                <div className="flex items-center justify-between">
                  <div className="text-xs font-mono text-[#888899] uppercase">Assessed Priority</div>
                  <div className="flex items-center gap-2">
                    <StarRating priority={current.priority} />
                    <span className="font-mono text-[#C9A84C] font-bold">{current.priority.toFixed(1)}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="text-center text-sm font-medium text-[#FAF8F5] mt-2">
            Overwrite existing structural analysis data?
          </div>

          <div className="flex flex-col gap-3 mt-4 pt-6 border-t border-[#2A2A35]/50">
            <div className="flex justify-between gap-4">
              <button
                className="magnetic-btn flex-1 py-3 bg-[#13131A] border border-[#2A2A35] text-[#888899] hover:text-[#FAF8F5] hover:border-[#888899] rounded-xl font-medium transition-colors"
                onClick={() => advance(false)}
              >
                BYPASS
              </button>
              <button
                className="magnetic-btn flex-1 py-3 bg-[#C9A84C] text-[#0D0D12] rounded-xl font-bold hover:bg-[#D4B55C] flex items-center justify-center gap-2 shadow-[0_0_15px_rgba(201,168,76,0.3)] transition-all"
                onClick={() => advance(true)}
              >
                OVERWRITE <CheckCircle2 className="w-4 h-4" />
              </button>
            </div>

            {remaining > 1 && (
              <div className="flex justify-between gap-4 mt-2">
                <button
                  className="magnetic-btn text-xs font-mono text-[#888899] hover:text-[#FAF8F5] transition-colors py-2 flex items-center gap-1 justify-center flex-1"
                  onClick={handleSkipAll}
                >
                  <FastForward className="w-3 h-3" /> BYPASS ALL ({remaining})
                </button>
                <button
                  className="magnetic-btn text-xs font-mono text-[#C9A84C] hover:text-[#D4B55C] border border-[#C9A84C]/30 hover:bg-[#C9A84C]/10 transition-colors py-2 rounded-lg flex items-center gap-1 justify-center flex-1"
                  onClick={handleReplaceAll}
                >
                  <CheckCircle2 className="w-3 h-3" /> OVERWRITE ALL ({remaining})
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
