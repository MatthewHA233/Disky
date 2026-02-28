import { useEffect, useRef, useState, useCallback } from "react";
import {
  ExternalLink, Copy, FolderOpen, Terminal,
  RefreshCw, Info, FolderMinus, Move,
  Trash2, XCircle, Tag as TagIcon,
} from "lucide-react";
import gsap from "gsap";
import type { DirEntry, Tag, FileTag } from "../types";
import {
  openPath, showInExplorer, openInTerminal,
  showProperties, emptyFolder, pickFolderAndMove,
  refreshScanNode, deleteItems,
} from "../lib/invoke";
import { cn } from "../lib/utils";
import { TagPicker } from "./TagPicker";

interface Props {
  entry: DirEntry;
  x: number;
  y: number;
  onClose: () => void;
  onDataChanged: () => void;
  tags?: Tag[];
  pathTags?: Map<string, FileTag[]>;
  onToggleTag?: (path: string, tagId: number, isDir: boolean) => Promise<boolean>;
  onCreateTag?: (name: string, color: string) => Promise<Tag>;
  onDeleteTag?: (id: number) => Promise<void>;
}

type ConfirmAction = "empty-trash" | "empty-perm" | "delete-trash" | "delete-perm" | "move" | null;

interface MenuItem {
  id: string;
  label: string;
  shortcut?: string;
  icon: React.ReactNode;
  danger?: boolean;
  disabled?: boolean;
  action: () => void;
}

export function ContextMenu({ entry, x, y, onClose, onDataChanged, tags, pathTags, onToggleTag, onCreateTag, onDeleteTag }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [confirm, setConfirm] = useState<ConfirmAction>(null);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [showTagPicker, setShowTagPicker] = useState(false);

  // Edge-detect positioning
  const [pos, setPos] = useState({ x, y });
  useEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const nx = x + rect.width > window.innerWidth ? window.innerWidth - rect.width - 8 : x;
    const ny = y + rect.height > window.innerHeight ? window.innerHeight - rect.height - 8 : y;
    setPos({ x: Math.max(4, nx), y: Math.max(4, ny) });
  }, [x, y]);

  // GSAP entrance
  useEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    gsap.fromTo(el,
      { opacity: 0, scale: 0.92, y: -6 },
      { opacity: 1, scale: 1, y: 0, duration: 0.18, ease: "power2.out" }
    );
  }, []);

  // Close on Escape / click-outside / scroll
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose();
    };
    const onScroll = () => onClose();
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onClick);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onClick);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [onClose]);

  const parentPath = entry.path.replace(/\//g, "\\").replace(/\\[^\\]+$/, "");

  const doRefreshAndClose = useCallback(async () => {
    try {
      await refreshScanNode(parentPath);
    } catch { /* parent may not exist in tree if root-level */ }
    onDataChanged();
    onClose();
  }, [parentPath, onDataChanged, onClose]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(entry.path);
    onClose();
  }, [entry.path, onClose]);

  const handleConfirmAction = useCallback(async () => {
    setLoading(true);
    setResult(null);
    try {
      switch (confirm) {
        case "empty-trash":
        case "empty-perm": {
          const r = await emptyFolder(entry.path, confirm === "empty-trash");
          if (r.errors.length > 0) {
            setResult(`已删除 ${r.deleted} 项，${r.errors.length} 项失败`);
            setLoading(false);
            setTimeout(doRefreshAndClose, 1200);
            return;
          }
          await doRefreshAndClose();
          return;
        }
        case "delete-trash":
        case "delete-perm": {
          const results = await deleteItems([entry.path], confirm === "delete-trash");
          const failed = results.filter(r => !r.success);
          if (failed.length > 0) {
            setResult(failed[0].error ?? "删除失败");
            setLoading(false);
            return;
          }
          await doRefreshAndClose();
          return;
        }
        case "move": {
          try {
            await pickFolderAndMove(entry.path);
            await doRefreshAndClose();
          } catch (e) {
            const msg = String(e);
            if (msg.includes("cancelled")) {
              setConfirm(null);
            } else {
              setResult(msg);
            }
            setLoading(false);
          }
          return;
        }
      }
    } catch (e) {
      setResult(String(e));
      setLoading(false);
    }
  }, [confirm, entry.path, doRefreshAndClose]);

  const runAction = useCallback(async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      onClose();
    } catch (e) {
      setResult(String(e));
    }
  }, [onClose]);

  const isDir = entry.is_dir;

  const groups: (MenuItem | "sep")[][] = [
    [
      {
        id: "open", label: "打开", shortcut: "Enter",
        icon: <ExternalLink className="w-3.5 h-3.5" />,
        action: () => runAction(() => openPath(entry.path)),
      },
      {
        id: "copy", label: "复制路径", shortcut: "Ctrl+C",
        icon: <Copy className="w-3.5 h-3.5" />,
        action: handleCopy,
      },
    ],
    [
      {
        id: "explorer", label: "在资源管理器中打开",
        icon: <FolderOpen className="w-3.5 h-3.5" />,
        action: () => runAction(() => showInExplorer(entry.path)),
      },
      {
        id: "cmd", label: "在命令提示符中打开",
        icon: <Terminal className="w-3.5 h-3.5" />,
        action: () => runAction(() => openInTerminal(entry.path, "cmd")),
      },
      {
        id: "ps", label: "在 PowerShell 中打开",
        icon: <Terminal className="w-3.5 h-3.5" />,
        action: () => runAction(() => openInTerminal(entry.path, "powershell")),
      },
    ],
    [
      {
        id: "tag", label: "标记标签",
        icon: <TagIcon className="w-3.5 h-3.5" />,
        disabled: !tags || tags.length === 0,
        action: () => setShowTagPicker(true),
      },
    ],
    [
      {
        id: "refresh", label: "刷新", shortcut: "F5",
        icon: <RefreshCw className="w-3.5 h-3.5" />,
        disabled: !isDir,
        action: async () => {
          try {
            await refreshScanNode(entry.path);
            onDataChanged();
          } catch { /* ignore */ }
          onClose();
        },
      },
      {
        id: "props", label: "属性", shortcut: "Alt+Enter",
        icon: <Info className="w-3.5 h-3.5" />,
        action: () => runAction(() => showProperties(entry.path)),
      },
    ],
    [
      {
        id: "empty", label: "清空文件夹",
        icon: <FolderMinus className="w-3.5 h-3.5" />,
        disabled: !isDir,
        danger: true,
        action: () => setConfirm("empty-trash"),
      },
      {
        id: "move", label: "移动到...", shortcut: "Ctrl+M",
        icon: <Move className="w-3.5 h-3.5" />,
        action: () => setConfirm("move"),
      },
    ],
    [
      {
        id: "trash", label: "删除至回收站", shortcut: "Del",
        icon: <Trash2 className="w-3.5 h-3.5" />,
        danger: true,
        action: () => setConfirm("delete-trash"),
      },
      {
        id: "perm-del", label: "永久删除", shortcut: "Shift+Del",
        icon: <XCircle className="w-3.5 h-3.5" />,
        danger: true,
        action: () => setConfirm("delete-perm"),
      },
    ],
  ];

  // Tag picker panel
  if (showTagPicker && tags && onToggleTag && onCreateTag) {
    const fileTags = pathTags?.get(entry.path) ?? [];
    return (
      <div
        ref={menuRef}
        className="fixed z-[9999] bg-[#13131A] border border-[#2A2A35] rounded-xl shadow-2xl font-mono text-xs"
        style={{ left: pos.x, top: pos.y }}
      >
        <TagPicker
          tags={tags}
          fileTags={fileTags}
          onToggle={(tagId) => onToggleTag(entry.path, tagId, entry.is_dir)}
          onCreate={(name, color) => onCreateTag(name, color)}
          onDeleteTag={onDeleteTag ? (id) => onDeleteTag(id) : undefined}
          onClose={onClose}
        />
      </div>
    );
  }

  // Inline confirmation panel
  if (confirm) {
    const labels: Record<NonNullable<ConfirmAction>, { title: string; desc: string; btn: string; danger: boolean }> = {
      "empty-trash": { title: "清空文件夹", desc: `将 ${entry.name} 的所有内容移至回收站`, btn: "确认清空", danger: false },
      "empty-perm": { title: "永久清空", desc: `永久删除 ${entry.name} 内所有文件`, btn: "永久清空", danger: true },
      "delete-trash": { title: "删除至回收站", desc: `将 ${entry.name} 移至回收站`, btn: "确认删除", danger: false },
      "delete-perm": { title: "永久删除", desc: `永久删除 ${entry.name}，不可恢复`, btn: "永久删除", danger: true },
      "move": { title: "移动到...", desc: `选择目标文件夹移动 ${entry.name}`, btn: "选择文件夹", danger: false },
    };
    const cfg = labels[confirm];

    return (
      <div
        ref={menuRef}
        className="fixed z-[9999] bg-[#13131A] border border-[#2A2A35] rounded-xl shadow-2xl p-4 min-w-[260px] font-mono text-xs"
        style={{ left: pos.x, top: pos.y }}
      >
        <div className="text-[#FAF8F5] font-semibold mb-1">{cfg.title}</div>
        <div className="text-[#888899] mb-3 leading-relaxed">{cfg.desc}</div>
        {result && (
          <div className="text-[#E74C3C] mb-2 text-[11px]">{result}</div>
        )}
        <div className="flex gap-2">
          <button
            className="px-3 py-1.5 rounded-lg bg-[#2A2A35] text-[#888899] hover:bg-[#333340] transition-colors"
            disabled={loading}
            onClick={() => { setConfirm(null); setResult(null); }}
          >
            取消
          </button>
          {confirm === "empty-trash" && (
            <button
              className="px-3 py-1.5 rounded-lg bg-[#E74C3C]/20 text-[#E74C3C] border border-[#E74C3C]/30 hover:bg-[#E74C3C]/30 transition-colors"
              disabled={loading}
              onClick={() => setConfirm("empty-perm")}
            >
              永久清空
            </button>
          )}
          <button
            className={cn(
              "px-3 py-1.5 rounded-lg transition-colors flex-1",
              cfg.danger
                ? "bg-[#E74C3C] text-white hover:bg-[#C0392B]"
                : "bg-[#C9A84C] text-[#0D0D12] hover:bg-[#B89A3E]"
            )}
            disabled={loading}
            onClick={handleConfirmAction}
          >
            {loading ? "处理中..." : cfg.btn}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={menuRef}
      className="fixed z-[9999] bg-[#13131A] border border-[#2A2A35] rounded-xl shadow-2xl py-1.5 min-w-[240px] font-mono text-xs"
      style={{ left: pos.x, top: pos.y }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {groups.map((group, gi) => (
        <div key={gi}>
          {gi > 0 && <div className="my-1 border-t border-[#2A2A35]" />}
          {group.map((item) => {
            if (item === "sep") return null;
            const mi = item as MenuItem;
            return (
              <button
                key={mi.id}
                className={cn(
                  "w-full flex items-center gap-3 px-3 py-1.5 text-left transition-colors",
                  mi.disabled
                    ? "text-[#555] cursor-not-allowed"
                    : mi.danger
                      ? "text-[#E74C3C] hover:bg-[#E74C3C]/10"
                      : "text-[#EAE6DF] hover:bg-[#FFFFFF]/5"
                )}
                disabled={mi.disabled}
                onClick={() => !mi.disabled && mi.action()}
              >
                <span className="w-4 flex justify-center shrink-0">{mi.icon}</span>
                <span className="flex-1">{mi.label}</span>
                {mi.shortcut && (
                  <span className="text-[10px] text-[#555] shrink-0">{mi.shortcut}</span>
                )}
              </button>
            );
          })}
        </div>
      ))}
      {result && (
        <div className="mx-3 my-1.5 p-2 rounded-lg bg-[#E74C3C]/10 text-[#E74C3C] text-[10px] break-all">
          {result}
        </div>
      )}
    </div>
  );
}
