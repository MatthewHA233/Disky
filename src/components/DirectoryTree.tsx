import { useEffect, useRef, useState, useMemo, useCallback } from "react";
import type { AiAnalysis, DirEntry, FileTag } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
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
  activePath?: string | null;
}

interface TreeNode extends DirEntry {
  children?: TreeNode[];
  expanded?: boolean;
  loaded?: boolean;
}

/** Ensure `target` node is expanded, loading children if needed */
async function expandNode(list: TreeNode[], target: string): Promise<TreeNode[]> {
  const result: TreeNode[] = [];
  for (const node of list) {
    if (node.path === target) {
      if (!node.loaded && node.is_dir) {
        const kids = await getChildren(node.path, 200);
        result.push({
          ...node, expanded: true, loaded: true,
          children: kids.map((k) => ({ ...k, expanded: false, loaded: false })),
        });
      } else {
        result.push({ ...node, expanded: true });
      }
    } else if (node.children) {
      result.push({ ...node, children: await expandNode(node.children, target) });
    } else {
      result.push(node);
    }
  }
  return result;
}

/** Expand every path in navPath[1..n] into the node list */
async function expandToNav(list: TreeNode[], navPath: string[]): Promise<TreeNode[]> {
  let updated = list;
  for (let i = 1; i < navPath.length; i++) {
    updated = await expandNode(updated, navPath[i]);
  }
  return updated;
}

export function DirectoryTree({
  rootPath, liveChildren, scanning,
  onSelect, selected, analyses, onAnalyzeSelected, analyzing,
  navPath, onNavigate, onContextMenu,
  pathTags, onRemoveTag, activePath,
}: Props) {
  const [nodes, setNodes] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);
  const selfNav = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const scrollPendingRef = useRef<string | null>(null);

  // Capture latest navPath for use in initial-load closure
  const navPathRef = useRef(navPath);
  useEffect(() => { navPathRef.current = navPath; }, [navPath]);

  // GSAP entrance
  useEffect(() => {
    if (!scanning && nodes.length > 0 && listRef.current) {
      const ctx = gsap.context(() => {
        gsap.from(".tree-row-anim", {
          y: 20, opacity: 0, duration: 0.4, stagger: 0.02, ease: "power3.out", clearProps: "all",
        });
      }, listRef);
      return () => ctx.revert();
    }
  }, [scanning, rootPath]);

  // Queue scroll when activePath changes
  useEffect(() => {
    if (activePath) scrollPendingRef.current = activePath;
  }, [activePath]);

  // After nodes update (expansion done), execute pending scroll-to-top
  useEffect(() => {
    const target = scrollPendingRef.current;
    if (!target || !listRef.current) return;
    const raf = requestAnimationFrame(() => {
      const el = rowRefs.current.get(target);
      const container = listRef.current;
      if (el && container) {
        const containerRect = container.getBoundingClientRect();
        const elRect = el.getBoundingClientRect();
        container.scrollTop += (elRect.top - containerRect.top) - 8;
        scrollPendingRef.current = null;
      }
    });
    return () => cancelAnimationFrame(raf);
  }, [nodes]);

  const sortedLive = useMemo(() => {
    return [...liveChildren].sort((a, b) => b.logical_size - a.logical_size);
  }, [liveChildren]);

  const displayItems = scanning ? sortedLive : nodes;
  const totalSize = useMemo(() => displayItems.reduce((s, i) => s + i.logical_size, 0), [displayItems]);

  // Initial load — also applies current navPath expansion immediately (handles remount after TagBoard)
  useEffect(() => {
    if (!scanning && rootPath) {
      setLoading(true);
      getChildren(rootPath, 200)
        .then(async (items) => {
          let initial: TreeNode[] = items.map((i) => ({ ...i, expanded: false, loaded: false }));
          const nav = navPathRef.current;
          if (nav.length > 1) {
            initial = await expandToNav(initial, nav);
          }
          setNodes(initial);
        })
        .finally(() => setLoading(false));
    } else {
      setNodes([]);
    }
  }, [scanning, rootPath]); // eslint-disable-line react-hooks/exhaustive-deps

  // Toggle a node open/closed on user click
  const toggleNode = useCallback(async (path: string) => {
    const toggle = async (list: TreeNode[]): Promise<TreeNode[]> => {
      const result: TreeNode[] = [];
      for (const node of list) {
        if (node.path === path) {
          if (!node.loaded && node.is_dir) {
            const kids = await getChildren(node.path, 200);
            result.push({
              ...node, expanded: true, loaded: true,
              children: kids.map((k) => ({ ...k, expanded: false, loaded: false })),
            });
          } else {
            result.push({ ...node, expanded: !node.expanded });
          }
        } else if (node.children) {
          result.push({ ...node, children: await toggle(node.children) });
        } else {
          result.push(node);
        }
      }
      return result;
    };
    setNodes(await toggle(nodes));
  }, [nodes]);

  const handleDirClick = (path: string) => {
    selfNav.current = true;
    toggleNode(path);
    onNavigate(path);
  };

  // Expand tree to match navPath changes (user navigating via TreeMap / breadcrumb)
  useEffect(() => {
    if (selfNav.current) { selfNav.current = false; return; }
    if (scanning || navPath.length <= 1 || nodes.length === 0) return;
    expandToNav(nodes, navPath).then(setNodes);
  }, [navPath]); // eslint-disable-line react-hooks/exhaustive-deps

  const renderRow = (item: DirEntry | TreeNode, depth: number, parentSize: number) => {
    const node = item as TreeNode;
    const isTreeNode = "expanded" in node;
    const pct = parentSize > 0 ? (item.logical_size / parentSize) * 100 : 0;
    const isScanning = scanning && item.is_dir && item.logical_size === 0;
    const isSelected = selected?.has(item.path) ?? false;
    const hasAnalysis = analyses?.has(item.path);
    const isActive = item.path === activePath;
    const hasExpandedChildren = isActive && isTreeNode && node.expanded && (node.children?.length ?? 0) > 0;

    return (
      <div
        key={item.path}
        ref={(el) => {
          if (el) rowRefs.current.set(item.path, el);
          else rowRefs.current.delete(item.path);
        }}
        className={cn(
          hasExpandedChildren && "rounded-xl ring-1 ring-[#C9A84C]/25 bg-[#C9A84C]/[0.04] my-0.5"
        )}
      >
        <div
          className={cn(
            "tree-row-anim group flex items-center gap-1.5 px-1 py-0.5 cursor-pointer border-l-2 transition-all hover:bg-[#FFFFFF]/5",
            isActive
              ? "border-[#C9A84C] bg-[#C9A84C]/10 text-[#FAF8F5]"
              : isSelected
                ? "border-[#C9A84C] bg-[#C9A84C]/5"
                : "border-transparent text-[#888899]",
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
                isSelected ? "bg-[#C9A84C] border-[#C9A84C]" : "border-[#2A2A35] group-hover:border-[#C9A84C]"
              )}
              onClick={(e) => { e.stopPropagation(); onSelect(item.path, item.logical_size); }}
            >
              {isSelected && <div className="w-2 h-2 bg-[#0D0D12] rounded-sm" />}
            </div>
          )}

          <span className="w-4 flex justify-center shrink-0">
            {item.is_dir ? (
              scanning ? (
                isScanning
                  ? <Loader2 className="w-3 h-3 animate-spin text-[#C9A84C]" />
                  : <ChevronRight className="w-3.5 h-3.5" />
              ) : (
                isTreeNode && node.expanded
                  ? <ChevronDown className="w-3.5 h-3.5 text-[#C9A84C]" />
                  : <ChevronRight className="w-3.5 h-3.5" />
              )
            ) : (
              <span className="w-1 h-1 rounded-full bg-[#EAE6DF]/30" />
            )}
          </span>

          <div className="flex items-center gap-1.5 w-[15%] min-w-0 shrink-0">
            {item.is_dir ? (
              <FolderOpen className={cn("w-3.5 h-3.5 shrink-0", isTreeNode && node.expanded ? "text-[#C9A84C]" : "text-[#C9A84C]/70")} />
            ) : (
              <File className="w-3.5 h-3.5 shrink-0 text-[#EAE6DF]/70" />
            )}
            <span className={cn(
              "break-all font-medium transition-colors group-hover:text-[#FAF8F5] text-[11px] leading-snug",
              isActive ? "text-[#FAF8F5] font-semibold" : item.is_dir ? "text-[#C9A84C]" : "text-[#EAE6DF]",
              isSelected && !isActive && "text-[#C9A84C] font-semibold"
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
                    className="opacity-0 group-hover/tag:opacity-100 hover:text-[#E74C3C] transition-opacity -mr-0.5"
                    onClick={(e) => { e.stopPropagation(); onRemoveTag(item.path, ft.tag_id, item.is_dir); }}
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
              <span className="text-[#C9A84C] animate-pulse">扫描中...</span>
            ) : (
              <span className="text-[#FAF8F5]">{formatSize(item.logical_size)}</span>
            )}
          </span>

          <span className="w-14 text-right shrink-0 font-mono text-[11px] text-[#888899]">
            {!isScanning && item.is_dir ? formatNumber(item.files) : "-"}
          </span>

          <span className="w-14 text-right shrink-0 font-mono text-[11px] text-[#888899]">
            {!isScanning && item.is_dir ? formatNumber(item.subdirs) : "-"}
          </span>

          <span className="w-14 text-right shrink-0 font-mono text-[11px] text-[#888899]">
            {!isScanning && `${pct.toFixed(1)}%`}
          </span>

          <span className="w-28 shrink-0 flex items-center justify-center">
            {analyses?.get(item.path) && (
              <StarRating priority={analyses.get(item.path)!.priority} />
            )}
          </span>

          <span className="flex-1 min-w-0 text-xs text-[#888899] group-hover:text-[#FAF8F5] transition-colors whitespace-pre-wrap break-words leading-snug">
            {analyses?.get(item.path)?.description ?? ""}
          </span>
        </div>

        {!scanning && isTreeNode && node.expanded && node.children && (
          <div className="flex flex-col">
            {node.children.map((c) => renderRow(c, depth + 1, item.logical_size))}
          </div>
        )}
      </div>
    );
  };

  if (!scanning && loading) {
    return (
      <div className="h-full flex items-center justify-center text-[#C9A84C] font-mono text-sm gap-2 uppercase tracking-widest">
        <Loader2 className="w-4 h-4 animate-spin" /> 加载中...
      </div>
    );
  }

  if (!scanning && !rootPath) {
    return (
      <div className="h-full flex items-center justify-center text-[#888899] font-mono text-sm uppercase tracking-widest">
        选择硬盘并开始扫描
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[#0D0D12] rounded-2xl border border-[#2A2A35] overflow-hidden shadow-2xl">
      <div className="flex items-center gap-1.5 px-1 py-2 bg-[#13131A] border-b border-[#2A2A35] shrink-0 font-mono text-[10px] uppercase tracking-[0.1em] text-[#888899]">
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
      <div ref={listRef} className="flex-1 overflow-y-auto py-2 custom-scrollbar">
        {displayItems.map((item) => renderRow(item, 0, totalSize))}
      </div>
    </div>
  );
}
