import { useEffect, useState, useRef } from "react";
import type { CleanupSession } from "../types";
import { listCleanupLogs } from "../lib/invoke";
import { formatSize } from "../lib/format";
import { normalizePath } from "../lib/path";
import gsap from "gsap";
import { History, XCircle, ChevronLeft, ChevronRight, ChevronDown, ChevronUp, FolderMinus, Trash2 } from "lucide-react";

const PAGE_SIZE = 8;

interface Props {
  onClose: () => void;
}

export function HistoryDialog({ onClose }: Props) {
  const [sessions, setSessions] = useState<CleanupSession[]>([]);
  const [totalSessions, setTotalSessions] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const dialogRef = useRef<HTMLDivElement>(null);

  const totalPages = Math.max(1, Math.ceil(totalSessions / PAGE_SIZE));

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

  const fetchPage = (p: number) => {
    setLoading(true);
    listCleanupLogs(p, PAGE_SIZE)
      .then((res) => {
        setSessions(res.sessions);
        setTotalSessions(res.total_sessions);
        setPage(p);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchPage(1); }, []);

  const toggleExpand = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const formatSessionTime = (ts: string) => {
    try {
      const d = new Date(ts);
      return d.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
    } catch {
      return ts;
    }
  };

  return (
    <div className="fixed inset-0 bg-fiber/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-4xl rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)] max-h-[85vh]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-8 py-5 border-b border-slate flex items-center justify-between bg-abyss shrink-0">
          <div className="flex items-center gap-3">
            <History className="w-5 h-5 text-photon" />
            <h2 className="text-lg font-semibold text-textMain">清理记录</h2>
          </div>
          <button className="text-text-dim hover:text-textMain transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        {/* Sessions list */}
        <div className="flex-1 overflow-y-auto custom-scrollbar p-4 flex flex-col gap-3 min-h-0">
          {loading && sessions.length === 0 ? (
            <div className="text-center py-12 font-mono text-sm text-text-dim">加载中...</div>
          ) : sessions.length === 0 ? (
            <div className="text-center py-12 font-mono text-sm text-text-dim">暂无清理记录</div>
          ) : (
            sessions.map((s) => {
              const key = `${s.session_ts}_${s.drive}`;
              const isOpen = expanded.has(key);
              return (
                <div key={key} className="rounded-xl border border-slate bg-abyss overflow-hidden">
                  {/* Session header (parent) */}
                  <button
                    className="w-full flex items-center gap-3 px-5 py-3 hover:bg-[#1A1A24] transition-colors text-left"
                    onClick={() => toggleExpand(key)}
                  >
                    {isOpen
                      ? <ChevronUp className="w-4 h-4 text-text-dim shrink-0" />
                      : <ChevronDown className="w-4 h-4 text-text-dim shrink-0" />
                    }
                    <span className="text-sm font-mono text-textMain font-semibold shrink-0">
                      {normalizePath(s.drive)}
                    </span>
                    <span className="text-xs font-mono text-text-dim">
                      {formatSessionTime(s.session_ts)}
                    </span>
                    <span className="text-xs font-mono text-text-dim">
                      {s.actions.length} 项操作
                    </span>
                    <span className="flex-1" />
                    <span className="text-sm font-mono font-bold text-success shrink-0">
                      +{formatSize(s.total_freed)}
                    </span>
                  </button>

                  {/* Expanded action list (children) */}
                  {isOpen && (
                    <div className="border-t border-slate/50 bg-fiber/50">
                      {s.actions.map((a) => {
                        const isEmpty = a.action_type === "empty";
                        return (
                          <div
                            key={a.id}
                            className="flex flex-col gap-0.5 px-5 py-2 border-b border-slate/30 last:border-0 hover:bg-[#FFFFFF]/[0.02]"
                          >
                            <div className="flex items-center gap-3">
                              <span className="w-4 flex justify-center shrink-0">
                                {isEmpty
                                  ? <FolderMinus className="w-3.5 h-3.5 text-photon" />
                                  : <Trash2 className="w-3.5 h-3.5 text-danger" />
                                }
                              </span>
                              <span className="text-xs font-mono text-text-dim shrink-0 w-10">
                                {isEmpty ? "清空" : "删除"}
                              </span>
                              <span className="text-xs font-mono text-border truncate flex-1" title={a.path}>
                                {a.path}
                              </span>
                              <span className="text-xs font-mono font-bold text-success shrink-0">
                                +{formatSize(a.bytes_freed)}
                              </span>
                            </div>
                            {a.description && (
                              <div className="ml-[calc(1rem+0.75rem+2.5rem)] text-[11px] text-text-dim leading-snug truncate" title={a.description}>
                                {a.description}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>

        {/* Pagination */}
        {totalPages > 1 && (
          <div className="px-8 py-3 border-t border-slate bg-abyss flex items-center justify-center gap-3 shrink-0">
            <button
              className="p-1.5 rounded-md text-text-dim hover:text-textMain hover:bg-slate transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
              disabled={page <= 1 || loading}
              onClick={() => fetchPage(page - 1)}
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            <span className="text-xs font-mono text-text-dim">
              <span className="text-textMain">{page}</span> / {totalPages}
            </span>
            <button
              className="p-1.5 rounded-md text-text-dim hover:text-textMain hover:bg-slate transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
              disabled={page >= totalPages || loading}
              onClick={() => fetchPage(page + 1)}
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
