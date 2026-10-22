import { useEffect, useState } from "react";
import type { CleanupSession } from "../types";
import { listCleanupLogs } from "../lib/invoke";
import { formatSize } from "../lib/format";
import { basename, normalizePath, parentPath } from "../lib/path";
import { ChevronDown, ChevronLeft, ChevronRight, FolderMinus, Loader2, Trash2 } from "lucide-react";
import { Dialog } from "./ui/Dialog";
import { Button } from "./ui/Button";
import { cn } from "../lib/utils";

const PAGE_SIZE = 8;

interface Props {
  onClose: () => void;
}

function formatSessionTime(ts: string): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime())
    ? ts
    : d.toLocaleString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

/** 清理记录：每次清理一条，展开看具体删了什么 */
export function HistoryDialog({ onClose }: Props) {
  const [sessions, setSessions] = useState<CleanupSession[]>([]);
  const [totalSessions, setTotalSessions] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  const totalPages = Math.max(1, Math.ceil(totalSessions / PAGE_SIZE));
  const freedOnPage = sessions.reduce((s, x) => s + x.total_freed, 0);

  const fetchPage = (p: number) => {
    setLoading(true);
    setError(null);
    listCleanupLogs(p, PAGE_SIZE)
      .then((res) => {
        setSessions(res.sessions);
        setTotalSessions(res.total_sessions);
        setPage(p);
      })
      .catch((e) => setError(`读取清理记录失败：${String(e)}`))
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchPage(1); }, []);

  const toggleExpand = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key); else next.add(key);
      return next;
    });

  const footer = totalPages > 1 ? (
    <>
      <span className="mr-auto text-xs text-text-dim">第 {page} / {totalPages} 页，共 {totalSessions} 次清理</span>
      <Button variant="subtle" className="px-2" aria-label="上一页" disabled={page <= 1 || loading} onClick={() => fetchPage(page - 1)}>
        <ChevronLeft className="w-4 h-4" />
      </Button>
      <Button variant="subtle" className="px-2" aria-label="下一页" disabled={page >= totalPages || loading} onClick={() => fetchPage(page + 1)}>
        <ChevronRight className="w-4 h-4" />
      </Button>
    </>
  ) : undefined;

  return (
    <Dialog
      title="清理记录"
      subtitle={sessions.length > 0 ? <>这一页的 {sessions.length} 次清理共腾出 <b className="font-semibold text-success">{formatSize(freedOnPage)}</b></> : undefined}
      onClose={onClose}
      width="max-w-3xl"
      footer={footer}
    >
      {error ? (
        <div className="py-10 text-center">
          <p className="text-danger">{error}</p>
          <Button className="mt-3" onClick={() => fetchPage(page)}>重试</Button>
        </div>
      ) : loading && sessions.length === 0 ? (
        <div className="py-10 flex items-center justify-center gap-2 text-text-dim"><Loader2 className="w-4 h-4 animate-spin" />正在读取</div>
      ) : sessions.length === 0 ? (
        <div className="py-10 text-center text-text-dim">还没有清理过。在档案馆勾选可以腾出的空间，点右上角的清理按钮。</div>
      ) : (
        <div className="rounded border border-slate bg-abyss overflow-hidden">
          {sessions.map((s) => {
            const key = `${s.session_ts}_${s.drive}`;
            const isOpen = expanded.has(key);
            return (
              <div key={key} className="border-b border-slate last:border-b-0">
                <button
                  type="button"
                  aria-expanded={isOpen}
                  className="w-full flex items-center gap-3 h-11 px-3 text-left hover:bg-hover transition-colors"
                  onClick={() => toggleExpand(key)}
                >
                  <ChevronDown className={cn("w-4 h-4 shrink-0 text-text-dim transition-transform", !isOpen && "-rotate-90")} />
                  <span className="font-semibold shrink-0">{normalizePath(s.drive)}</span>
                  <span className="text-xs text-text-dim">{formatSessionTime(s.session_ts)}</span>
                  <span className="text-xs text-text-dim">{s.actions.length} 项</span>
                  <span className="flex-1" />
                  <span className="shrink-0 font-semibold text-success">{formatSize(s.total_freed)}</span>
                </button>

                {isOpen && (
                  <div className="pb-1">
                    {s.actions.map((a) => {
                      const isEmpty = a.action_type === "empty";
                      const Icon = isEmpty ? FolderMinus : Trash2;
                      return (
                        <div key={a.id} className="flex items-start gap-3 pl-10 pr-3 py-1.5 hover:bg-hover">
                          <Icon className="w-3.5 h-3.5 mt-0.5 shrink-0 text-text-dim" aria-label={isEmpty ? "清空" : "删除"} />
                          <span className="flex-1 min-w-0">
                            <span className="block truncate" title={a.path}>{basename(a.path)}</span>
                            <span className="block truncate text-[11px] text-text-faint">{isEmpty ? "清空了" : "删除了"} · {parentPath(a.path) ?? a.path}</span>
                            {a.description && <span className="block truncate text-[11px] text-text-dim" title={a.description}>{a.description}</span>}
                          </span>
                          <span className="shrink-0 text-xs text-text-dim">{formatSize(a.bytes_freed)}</span>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </Dialog>
  );
}
