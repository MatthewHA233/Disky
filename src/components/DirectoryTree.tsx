import { useEffect, useRef, useState, useMemo, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { AiAnalysis, DirEntry, FileTag } from "../types";
import { getChildrenPage } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import {
  PAGE_SIZE,
  appendPage,
  expandWithPages,
  fetchExpandPages,
  findNode,
  flattenTree,
  liveRows,
  makeRoot,
  pagePaths,
  updateNode,
  type EntryRow,
  type GroupPos,
  type MoreRow,
  type TreeNode,
  type TreeRow,
} from "../lib/dirTree";
import { StarRating } from "./StarRating";
import { ChevronRight, ChevronDown, FolderOpen, File, Loader2, Sparkles } from "lucide-react";
import gsap from "gsap";
import { cn } from "../lib/utils";

interface Props {
  rootPath: string | null;
  liveChildren: DirEntry[];
  scanning: boolean;
  onSelect?: (path: string, size: number) => void;
  selected?: Map<string, number>;
  analyses?: Map<string, AiAnalysis>;
  onAnalyzeSelected?: () => void;
  analyzing?: boolean;
  navPath: string[];
  onNavigate: (path: string) => void;
  onContextMenu?: (entry: DirEntry, x: number, y: number) => void;
  pathTags?: Map<string, FileTag[]>;
  onRemoveTag?: (path: string, tagId: number, isDir: boolean) => void;
  onPathsLoaded?: (paths: string[]) => void;
  activePath?: string | null;
}

/** 单行行高估计；AI 注释换行等多行情况由 measureElement 实测修正 */
const ROW_ESTIMATE = 22;
const LIST_PADDING = 8;

const fetchFirstPage = (path: string) => getChildrenPage(path, 0, PAGE_SIZE);

const GROUP_CLASS: Record<GroupPos, string> = {
  none: "",
  first: "bg-photon/[0.04] border-x border-t border-[#FF6D00]/25 rounded-t-xl pt-0.5",
  middle: "bg-photon/[0.04] border-x border-[#FF6D00]/25",
  last: "bg-photon/[0.04] border-x border-b border-[#FF6D00]/25 rounded-b-xl pb-0.5",
};

export function DirectoryTree({
  rootPath, liveChildren, scanning,
  onSelect, selected, analyses, onAnalyzeSelected, analyzing,
  navPath, onNavigate, onContextMenu,
  pathTags, onRemoveTag, onPathsLoaded, activePath,
}: Props) {
  const [root, setRoot] = useState<TreeNode | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [failedMore, setFailedMore] = useState<ReadonlySet<string>>(new Set());
  const [pendingRemoveTag, setPendingRemoveTag] = useState<{ path: string; tagId: number; isDir: boolean; tagName: string; tagColor: string } | null>(null);
  const selfNav = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);
  const scrollPendingRef = useRef<string | null>(null);
  const loadingMoreRef = useRef<Set<string>>(new Set());

  // 异步回调里读取最新树（避免闭包拿到旧状态）
  const rootRef = useRef(root);
  useEffect(() => { rootRef.current = root; }, [root]);

  // Capture latest navPath for use in initial-load closure
  const navPathRef = useRef(navPath);
  useEffect(() => { navPathRef.current = navPath; }, [navPath]);

  const rows = useMemo<TreeRow[]>(
    () => (scanning ? liveRows(liveChildren) : root ? flattenTree(root, activePath ?? null) : []),
    [scanning, liveChildren, root, activePath]
  );

  // 引用必须稳定：getItemKey 变化会让虚拟列表重算全部行的测量（滚动时每帧都会触发）
  const getItemKey = useCallback((index: number) => rows[index].key, [rows]);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => ROW_ESTIMATE,
    getItemKey,
    overscan: 12,
    paddingStart: LIST_PADDING,
    paddingEnd: LIST_PADDING,
    scrollPaddingStart: LIST_PADDING,
    useFlushSync: false,
  });
  const virtualItems = virtualizer.getVirtualItems();

  // GSAP entrance
  useEffect(() => {
    if (!scanning && (root?.children?.length ?? 0) > 0 && listRef.current) {
      const ctx = gsap.context(() => {
        gsap.from(".tree-row-anim", {
          y: 20, opacity: 0, duration: 0.4, stagger: 0.02, ease: "power3.out", clearProps: "all",
        });
      }, listRef);
      return () => ctx.revert();
    }
  }, [scanning, rootPath]); // eslint-disable-line react-hooks/exhaustive-deps

  // Queue scroll when activePath changes
  useEffect(() => {
    if (activePath) scrollPendingRef.current = activePath;
  }, [activePath]);

  // 目标行出现在列表里（展开完成）后滚到顶部；虚拟列表中该行可能尚未挂载，按下标滚动
  useEffect(() => {
    const target = scrollPendingRef.current;
    if (!target) return;
    const index = rows.findIndex((r) => r.kind === "entry" && r.node.path === target);
    if (index < 0) return;
    scrollPendingRef.current = null;
    virtualizer.scrollToIndex(index, { align: "start" });
  }, [rows]); // eslint-disable-line react-hooks/exhaustive-deps

  // Initial load — also applies current navPath expansion immediately (handles remount after TagBoard)
  useEffect(() => {
    if (scanning || !rootPath) {
      setRoot(null);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    setFailedMore(new Set());
    (async () => {
      try {
        const page = await fetchFirstPage(rootPath);
        const initial = makeRoot(rootPath, page);
        const targets = navPathRef.current.slice(1);
        const { pages, error: expandError } = await fetchExpandPages(initial, targets, fetchFirstPage);
        if (cancelled) return;
        // 某级展开失败时仍显示根目录和已展开的部分
        setRoot(expandWithPages(initial, targets, pages));
        if (expandError) setError(`展开目录失败：${String(expandError)}`);
        const paths = [...page.entries.map((e) => e.path), ...pagePaths(pages.values())];
        if (paths.length > 0) onPathsLoaded?.(paths);
      } catch (e) {
        if (!cancelled) {
          setRoot(null);
          setError(`加载目录失败：${String(e)}`);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [scanning, rootPath]); // eslint-disable-line react-hooks/exhaustive-deps

  // Toggle a node open/closed on user click
  const toggleNode = useCallback(async (path: string) => {
    const node = rootRef.current && findNode(rootRef.current, path);
    if (!node) return;
    if (!node.is_dir || node.loaded) {
      setRoot((prev) => prev && updateNode(prev, path, (n) => ({ ...n, expanded: !n.expanded })));
      return;
    }
    try {
      const page = await fetchFirstPage(path);
      setRoot((prev) => prev && expandWithPages(prev, [path], new Map([[path, page]])));
      if (page.entries.length > 0) onPathsLoaded?.(page.entries.map((e) => e.path));
    } catch (e) {
      setError(`展开目录失败：${String(e)}`);
    }
  }, [onPathsLoaded]);

  const handleDirClick = (path: string) => {
    selfNav.current = true;
    toggleNode(path);
    onNavigate(path);
  };

  // Expand tree to match navPath changes (user navigating via TreeMap / breadcrumb)
  useEffect(() => {
    if (selfNav.current) { selfNav.current = false; return; }
    const current = rootRef.current;
    if (scanning || navPath.length <= 1 || !current) return;
    const targets = navPath.slice(1);
    fetchExpandPages(current, targets, fetchFirstPage).then(({ pages, error: expandError }) => {
      setRoot((prev) => prev && expandWithPages(prev, targets, pages));
      if (expandError) setError(`展开目录失败：${String(expandError)}`);
      const paths = pagePaths(pages.values());
      if (paths.length > 0) onPathsLoaded?.(paths);
    });
  }, [navPath]); // eslint-disable-line react-hooks/exhaustive-deps

  // 分页：取 path 的下一页并追加
  const loadMore = useCallback(async (path: string) => {
    if (loadingMoreRef.current.has(path)) return;
    const node = rootRef.current && findNode(rootRef.current, path);
    if (!node?.children) return;
    const offset = node.children.length;
    loadingMoreRef.current.add(path);
    try {
      const page = await getChildrenPage(path, offset, PAGE_SIZE);
      setRoot((prev) => prev && appendPage(prev, path, offset, page));
      if (page.entries.length > 0) onPathsLoaded?.(page.entries.map((e) => e.path));
    } catch {
      // 标记失败，占位行改为「点击重试」，避免滚动时反复请求
      setFailedMore((prev) => new Set(prev).add(path));
    } finally {
      loadingMoreRef.current.delete(path);
    }
  }, [onPathsLoaded]);

  // 分页占位行进入可视区即自动续取
  useEffect(() => {
    for (const item of virtualItems) {
      const row = rows[item.index];
      if (row?.kind === "more" && !failedMore.has(row.parentPath)) void loadMore(row.parentPath);
    }
  }, [virtualItems, rows, failedMore, loadMore]);

  const retryMore = (path: string) => {
    setFailedMore((prev) => new Set([...prev].filter((p) => p !== path)));
  };

  const renderEntry = ({ node, depth, parentSize }: EntryRow) => {
    const item = node;
    const pct = parentSize > 0 ? (item.logical_size / parentSize) * 100 : 0;
    const isScanning = scanning && item.is_dir && item.logical_size === 0;
    const isSelected = selected?.has(item.path) ?? false;
    const hasAnalysis = analyses?.has(item.path);
    const isActive = item.path === activePath;

    return (
      <div
        className={cn(
          "tree-row-anim group flex items-center gap-1.5 px-1 py-0.5 cursor-pointer border-l-2 transition-all hover:bg-[#FFFFFF]/5",
          isActive
            ? "border-photon bg-photon/10 text-textMain"
            : isSelected
              ? "border-photon bg-photon/5"
              : "border-transparent text-text-dim",
          !isActive && hasAnalysis && !isSelected && "border-[#7B61FF]/30"
        )}
        style={{ paddingLeft: `${depth * 1.25 + 0.25}rem` }}
        onClick={() => !scanning && item.is_dir && handleDirClick(item.path)}
        onContextMenu={(e) => {
          if (onContextMenu && !scanning) {
            e.preventDefault();
            onContextMenu(item, e.clientX, e.clientY);
          }
        }}
      >
        {onSelect && !scanning && (
          <div
            className={cn(
              "w-4 h-4 rounded border flex items-center justify-center shrink-0 transition-colors",
              isSelected ? "bg-photon border-photon" : "border-slate group-hover:border-photon"
            )}
            onClick={(e) => { e.stopPropagation(); onSelect(item.path, item.logical_size); }}
          >
            {isSelected && (
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <polyline points="1.5,5 4,7.5 8.5,2" stroke="#ffffff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )}
          </div>
        )}

        <span className="w-4 flex justify-center shrink-0">
          {item.is_dir ? (
            scanning ? (
              isScanning
                ? <Loader2 className="w-3 h-3 animate-spin text-photon" />
                : <ChevronRight className="w-3.5 h-3.5" />
            ) : (
              node.expanded
                ? <ChevronDown className="w-3.5 h-3.5 text-photon" />
                : <ChevronRight className="w-3.5 h-3.5" />
            )
          ) : (
            <span className="w-1 h-1 rounded-full bg-[#EAE6DF]/30" />
          )}
        </span>

        <div className="flex items-center gap-1.5 w-[15%] min-w-0 shrink-0">
          {item.is_dir ? (
            <FolderOpen className={cn("w-3.5 h-3.5 shrink-0", node.expanded ? "text-photon" : "text-photon/70")} />
          ) : (
            <File className="w-3.5 h-3.5 shrink-0 text-border/70" />
          )}
          <span className={cn(
            "break-all font-medium transition-colors group-hover:text-textMain text-[11px] leading-snug",
            isActive ? "text-textMain font-semibold" : item.is_dir ? "text-photon" : "text-border",
            isSelected && !isActive && "text-photon font-semibold"
          )}>
            {item.name}
          </span>
          {pathTags?.get(item.path)?.map((ft) => (
            <span
              key={ft.tag_id}
              className="group/tag inline-flex items-center gap-0.5 px-1.5 py-0 rounded-full shrink-0 text-[9px] font-medium leading-tight"
              style={{ backgroundColor: `${ft.tag_color}25`, color: ft.tag_color }}
            >
              {ft.tag_name}
              {onRemoveTag && (
                <button
                  className="opacity-0 group-hover/tag:opacity-100 hover:text-danger transition-opacity -mr-0.5"
                  onClick={(e) => { e.stopPropagation(); setPendingRemoveTag({ path: item.path, tagId: ft.tag_id, isDir: item.is_dir, tagName: ft.tag_name, tagColor: ft.tag_color }); }}
                  title={`移除「${ft.tag_name}」`}
                >
                  ×
                </button>
              )}
            </span>
          ))}
        </div>

        <span className="w-16 text-right shrink-0 font-mono text-[10px]">
          {isScanning ? (
            <span className="text-photon animate-pulse">扫描中...</span>
          ) : (
            <span className="text-textMain">{formatSize(item.logical_size)}</span>
          )}
        </span>

        <span className="w-14 text-right shrink-0 font-mono text-[11px] text-text-dim">
          {!isScanning && item.is_dir ? formatNumber(item.files) : "-"}
        </span>

        <span className="w-14 text-right shrink-0 font-mono text-[11px] text-text-dim">
          {!isScanning && item.is_dir ? formatNumber(item.subdirs) : "-"}
        </span>

        <span className="w-14 text-right shrink-0 font-mono text-[11px] text-text-dim">
          {!isScanning && `${pct.toFixed(1)}%`}
        </span>

        <span className="w-28 shrink-0 flex items-center justify-center">
          {analyses?.get(item.path) && (
            <StarRating priority={analyses.get(item.path)!.priority} />
          )}
        </span>

        <span className="flex-1 min-w-0 text-xs text-text-dim group-hover:text-textMain transition-colors whitespace-pre-wrap break-words leading-snug">
          {analyses?.get(item.path)?.description ?? ""}
        </span>
      </div>
    );
  };

  const renderMore = ({ parentPath, depth, remaining }: MoreRow) => (
    <div
      className="flex items-center gap-1.5 px-1 py-0.5 font-mono text-[11px] text-text-dim"
      style={{ paddingLeft: `${depth * 1.25 + 0.25}rem` }}
    >
      <span className="w-4 shrink-0" />
      {failedMore.has(parentPath) ? (
        <button className="text-danger hover:underline" onClick={() => retryMore(parentPath)}>
          加载失败，点击重试（还有 {formatNumber(remaining)} 项）
        </button>
      ) : (
        <>
          <Loader2 className="w-3 h-3 animate-spin text-photon" />
          正在加载剩余 {formatNumber(remaining)} 项...
        </>
      )}
    </div>
  );

  if (!scanning && loading) {
    return (
      <div className="h-full flex items-center justify-center text-photon font-mono text-sm gap-2 uppercase tracking-widest">
        <Loader2 className="w-4 h-4 animate-spin" /> 加载中...
      </div>
    );
  }

  if (!scanning && !rootPath) {
    return (
      <div className="h-full flex items-center justify-center text-text-dim font-mono text-sm uppercase tracking-widest">
        选择硬盘并开始扫描
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-fiber rounded-2xl border border-slate overflow-hidden shadow-2xl relative">
      <div className="flex items-center gap-1.5 px-1 py-2 bg-abyss border-b border-slate shrink-0 font-mono text-[10px] uppercase tracking-[0.1em] text-text-dim">
        {onSelect && !scanning && <span className="w-4 shrink-0" />}
        <span className="w-4 shrink-0" />
        <span className="w-[15%] min-w-0 shrink-0">名称</span>
        <span className="w-16 text-right shrink-0">大小</span>
        <span className="w-14 text-right shrink-0">文件</span>
        <span className="w-14 text-right shrink-0">目录</span>
        <span className="w-14 text-right shrink-0">占比</span>
        <span className="w-28 shrink-0 flex items-center justify-center">
          {onAnalyzeSelected && selected && selected.size > 0 ? (
            <button
              className="px-2 py-0.5 rounded bg-[#7B61FF]/20 text-[#7B61FF] border border-[#7B61FF]/40 hover:bg-[#7B61FF]/30 transition-colors flex items-center gap-1"
              disabled={analyzing}
              onClick={onAnalyzeSelected}
            >
              {analyzing ? <Loader2 className="w-3 h-3 animate-spin" /> : <Sparkles className="w-3 h-3" />}
              {analyzing ? "分析中" : `分析 (${selected.size})`}
            </button>
          ) : (
            "AI 分析"
          )}
        </span>
        <span className="flex-1 min-w-0">AI 注释</span>
      </div>
      {error && (
        <div className="px-3 py-1 border-b border-slate shrink-0 font-mono text-[11px] text-danger flex items-center gap-2">
          <span className="flex-1 min-w-0 truncate" title={error}>{error}</span>
          <button className="text-text-dim hover:text-textMain" onClick={() => setError(null)}>×</button>
        </div>
      )}
      <div ref={listRef} className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
          {virtualItems.map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            return (
              <div
                key={item.key}
                data-index={item.index}
                ref={virtualizer.measureElement}
                className={cn("absolute left-0 top-0 w-full", GROUP_CLASS[row.group])}
                style={{ transform: `translateY(${item.start}px)` }}
              >
                {row.kind === "entry" ? renderEntry(row) : renderMore(row)}
              </div>
            );
          })}
        </div>
      </div>

      {pendingRemoveTag && onRemoveTag && (
        <div className="absolute inset-0 bg-fiber/70 backdrop-blur-sm flex items-center justify-center z-10">
          <div className="bg-abyss border border-slate rounded-xl shadow-2xl p-5 min-w-[260px] max-w-[340px] font-mono text-xs">
            <div className="text-textMain font-semibold mb-2">移除标签</div>
            <div className="text-text-dim mb-4 leading-relaxed">
              确定要移除标签{" "}
              <span
                className="inline-flex items-center px-1.5 py-0.5 rounded-full text-[9px] font-medium"
                style={{ backgroundColor: `${pendingRemoveTag.tagColor}25`, color: pendingRemoveTag.tagColor }}
              >
                {pendingRemoveTag.tagName}
              </span>
              {" "}吗？
            </div>
            <div className="flex gap-2 justify-end">
              <button
                className="px-3 py-1.5 rounded-lg bg-slate text-text-dim hover:bg-[#333340] transition-colors"
                onClick={() => setPendingRemoveTag(null)}
              >
                取消
              </button>
              <button
                className="px-3 py-1.5 rounded-lg bg-[#E74C3C] text-white hover:bg-[#C0392B] transition-colors"
                onClick={() => {
                  onRemoveTag(pendingRemoveTag.path, pendingRemoveTag.tagId, pendingRemoveTag.isDir);
                  setPendingRemoveTag(null);
                }}
              >
                确认移除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
