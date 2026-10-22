import { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
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
  refreshScanNode, removeScanNode, deleteItems,
} from "../lib/invoke";
import { cn } from "../lib/utils";
import { IS_WINDOWS, TRASH_NAME } from "../lib/path";
import { TagPicker } from "./TagPicker";

interface Props {
  entry: DirEntry;
  x: number;
  y: number;
  onClose: () => void;
  onDataChanged: () => void;
  onCleanupDone?: (bytes: number, path: string, action: "delete" | "empty") => void;
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

export function ContextMenu({ entry, x, y, onClose, onDataChanged, onCleanupDone, tags, pathTags, onToggleTag, onCreateTag, onDeleteTag }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [confirm, setConfirm] = useState<ConfirmAction>(null);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [showTagPicker, setShowTagPicker] = useState(false);

  // 贴边时往回收，保证整个菜单都在窗口内。用 offset 尺寸（不受入场缩放动画影响），
  // 并在菜单内容变化（标签加载、切到确认 / 结果视图）后重新定位
  const [pos, setPos] = useState({ x, y });
  useLayoutEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const place = () => {
      const w = el.offsetWidth, h = el.offsetHeight;
      const nx = x + w > window.innerWidth - 8 ? window.innerWidth - w - 8 : x;
      const ny = y + h > window.innerHeight - 8 ? window.innerHeight - h - 8 : y;
      setPos({ x: Math.max(4, nx), y: Math.max(4, ny) });
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(el);
    return () => observer.disconnect();
  }, [x, y, confirm, result, showTagPicker]);

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

  // 删除/移走：从内存树手术式摘除节点（祖先大小自动回减），不重扫。
  // 之前刷新父目录的做法对根级条目意味着整盘重扫（分钟级"卡死"）。
  const removeNodeAndClose = useCallback(async () => {
    try {
      await removeScanNode(entry.path);
    } catch { /* 节点不在树中（如已被刷新移除）时忽略 */ }
    onDataChanged();
    onClose();
  }, [entry.path, onDataChanged, onClose]);

  // 清空文件夹：重扫该文件夹自身（清空后近乎瞬时），差量会传播给祖先
  const refreshSelfAndClose = useCallback(async () => {
    try {
      await refreshScanNode(entry.path);
    } catch { /* ignore */ }
    onDataChanged();
    onClose();
  }, [entry.path, onDataChanged, onClose]);

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
          if (r.bytes_freed > 0 && onCleanupDone) onCleanupDone(r.bytes_freed, entry.path, "empty");
          if (r.errors.length > 0) {
            setResult(`已删除 ${r.deleted} 项，${r.errors.length} 项失败`);
            setLoading(false);
            setTimeout(refreshSelfAndClose, 1200);
            return;
          }
          await refreshSelfAndClose();
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
          if (onCleanupDone) onCleanupDone(entry.logical_size, entry.path, "delete");
          await removeNodeAndClose();
          return;
        }
        case "move": {
          try {
            await pickFolderAndMove(entry.path);
            await removeNodeAndClose();
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
  }, [confirm, entry.path, entry.logical_size, onCleanupDone, removeNodeAndClose, refreshSelfAndClose]);

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
    IS_WINDOWS
      ? [
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
        ]
      : [
          {
            id: "explorer", label: "在 Finder 中显示",
            icon: <FolderOpen className="w-3.5 h-3.5" />,
            action: () => runAction(() => showInExplorer(entry.path)),
          },
          {
            id: "terminal", label: "在终端中打开",
            icon: <Terminal className="w-3.5 h-3.5" />,
            action: () => runAction(() => openInTerminal(entry.path, "terminal")),
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
        id: "props", label: IS_WINDOWS ? "属性" : "显示简介", shortcut: "Alt+Enter",
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
        id: "trash", label: `删除至${TRASH_NAME}`, shortcut: "Del",
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
        className="fixed z-[9999] bg-abyss border border-slate rounded-lg shadow-[var(--fl-shadow)] text-xs"
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
      "empty-trash": { title: "清空文件夹", desc: `将 ${entry.name} 的所有内容移至${TRASH_NAME}`, btn: "确认清空", danger: false },
      "empty-perm": { title: "永久清空", desc: `永久删除 ${entry.name} 内所有文件`, btn: "永久清空", danger: true },
      "delete-trash": { title: `删除至${TRASH_NAME}`, desc: `将 ${entry.name} 移至${TRASH_NAME}`, btn: "确认删除", danger: false },
      "delete-perm": { title: "永久删除", desc: `永久删除 ${entry.name}，不可恢复`, btn: "永久删除", danger: true },
      "move": { title: "移动到...", desc: `选择目标文件夹移动 ${entry.name}`, btn: "选择文件夹", danger: false },
    };
    const cfg = labels[confirm];

    return (
      <div
        ref={menuRef}
        className="fixed z-[9999] bg-abyss border border-slate rounded-lg shadow-[var(--fl-shadow)] p-4 min-w-[260px] text-xs"
        style={{ left: pos.x, top: pos.y }}
      >
        <div className="text-textMain font-semibold mb-1">{cfg.title}</div>
        <div className="text-text-dim mb-3 leading-relaxed">{cfg.desc}</div>
        {result && (
          <div className="text-danger mb-2 text-[11px]">{result}</div>
        )}
        <div className="flex gap-2">
          <button
            className="px-3 py-1.5 rounded-lg bg-slate text-text-dim hover:bg-hover transition-colors"
            disabled={loading}
            onClick={() => { setConfirm(null); setResult(null); }}
          >
            取消
          </button>
          {confirm === "empty-trash" && (
            <button
              className="px-3 py-1.5 rounded-lg bg-danger/20 text-danger border border-danger/30 hover:bg-danger/30 transition-colors"
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
                ? "bg-critical text-white hover:bg-critical/90"
                : "bg-photon text-on-accent hover:bg-photon/90"
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
      className="fixed z-[9999] bg-abyss border border-slate rounded-lg shadow-[var(--fl-shadow)] py-1.5 min-w-[240px] text-xs"
      style={{ left: pos.x, top: pos.y }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {groups.map((group, gi) => (
        <div key={gi}>
          {gi > 0 && <div className="my-1 border-t border-slate" />}
          {group.map((item) => {
            if (item === "sep") return null;
            const mi = item as MenuItem;
            return (
              <button
                key={mi.id}
                className={cn(
                  "w-full flex items-center gap-3 px-3 py-1.5 text-left transition-colors",
                  mi.disabled
                    ? "text-text-faint cursor-not-allowed"
                    : mi.danger
                      ? "text-danger hover:bg-danger/10"
                      : "text-border hover:bg-hover"
                )}
                disabled={mi.disabled}
                onClick={() => !mi.disabled && mi.action()}
              >
                <span className="w-4 flex justify-center shrink-0">{mi.icon}</span>
                <span className="flex-1">{mi.label}</span>
                {mi.shortcut && (
                  <span className="text-[10px] text-text-faint shrink-0">{mi.shortcut}</span>
                )}
              </button>
            );
          })}
        </div>
      ))}
      {result && (
        <div className="mx-3 my-1.5 p-2 rounded-lg bg-danger/10 text-danger text-[10px] break-all">
          {result}
        </div>
      )}
    </div>
  );
}
