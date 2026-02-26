import { useEffect, useRef, useState, useMemo } from "react";
import type { AiAnalysis, DirEntry } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import { SizeBar } from "./SizeBar";
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
}

interface TreeNode extends DirEntry {
  children?: TreeNode[];
  expanded?: boolean;
  loaded?: boolean;
}

export function DirectoryTree({ rootPath, liveChildren, scanning, onSelect, selected, analyses, onAnalyzeSelected, analyzing, navPath, onNavigate }: Props) {
  const [nodes, setNodes] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);
  const selfNav = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);

  // GSAP Entrance Animation
  useEffect(() => {
    if (!scanning && nodes.length > 0 && listRef.current) {
      const ctx = gsap.context(() => {
        gsap.from(".tree-row-anim", {
          y: 20,
          opacity: 0,
          duration: 0.4,
          stagger: 0.02,
          ease: "power3.out",
          clearProps: "all"
        });
      }, listRef);
      return () => ctx.revert();
    }
  }, [scanning, rootPath]); // Trigger when root path is loaded or scan stops

  const sortedLive = useMemo(() => {
    return [...liveChildren].sort((a, b) => b.logical_size - a.logical_size);
  }, [liveChildren]);

  const displayItems = scanning ? sortedLive : nodes;

  const totalSize = useMemo(() => {
    return displayItems.reduce((s, i) => s + i.logical_size, 0);
  }, [displayItems]);

  useEffect(() => {
    if (!scanning && rootPath) {
      setLoading(true);
      getChildren(rootPath, 200)
        .then((items) => {
          setNodes(items.map((i) => ({ ...i, expanded: false, loaded: false })));
        })
        .finally(() => setLoading(false));
    } else {
      setNodes([]);
    }
  }, [scanning, rootPath]);

  const toggleNode = async (path: string) => {
    const toggle = async (list: TreeNode[]): Promise<TreeNode[]> => {
      const result: TreeNode[] = [];
      for (const node of list) {
        if (node.path === path) {
          if (!node.loaded && node.is_dir) {
            const kids = await getChildren(node.path, 200);
            result.push({
              ...node,
              expanded: true,
              loaded: true,
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
  };

  const handleDirClick = (path: string) => {
    selfNav.current = true;
    toggleNode(path);
    onNavigate(path);
  };

  useEffect(() => {
    if (selfNav.current) {
      selfNav.current = false;
      return;
    }
    if (scanning || navPath.length <= 1 || nodes.length === 0) return;

    const ensureExpanded = async (list: TreeNode[], target: string): Promise<TreeNode[]> => {
      const result: TreeNode[] = [];
      for (const node of list) {
        if (node.path === target) {
          if (!node.loaded && node.is_dir) {
            const kids = await getChildren(node.path, 200);
            result.push({
              ...node,
              expanded: true,
              loaded: true,
              children: kids.map((k) => ({ ...k, expanded: false, loaded: false })),
            });
          } else {
            result.push({ ...node, expanded: true });
          }
        } else if (node.children) {
          result.push({ ...node, children: await ensureExpanded(node.children, target) });
        } else {
          result.push(node);
        }
      }
      return result;
    };

    const expandToNavPath = async () => {
      let updated = nodes;
      for (let i = 1; i < navPath.length; i++) {
        updated = await ensureExpanded(updated, navPath[i]);
      }
      setNodes(updated);
    };

    expandToNavPath();
  }, [navPath]); // eslint-disable-line react-hooks/exhaustive-deps

  const renderRow = (item: DirEntry | TreeNode, depth: number, parentSize: number) => {
    const node = item as TreeNode;
    const isTreeNode = "expanded" in node;
    const pct = parentSize > 0 ? (item.logical_size / parentSize) * 100 : 0;
    const isScanning = scanning && item.is_dir && item.logical_size === 0;
    const isSelected = selected?.has(item.path) ?? false;
    const hasAnalysis = analyses?.has(item.path);

    return (
      <div key={item.path}>
        <div
          className={cn(
            "tree-row-anim group flex items-center gap-2 px-2 py-0.5 cursor-pointer border-l-2 transition-all hover:bg-[#FFFFFF]/5",
            isSelected ? "border-[#C9A84C] bg-[#C9A84C]/5" : "border-transparent text-[#888899]",
            hasAnalysis && !isSelected && "border-[#7B61FF]/30"
          )}
          style={{ paddingLeft: `${depth * 1.25 + 0.5}rem` }}
          onClick={() => !scanning && item.is_dir && handleDirClick(item.path)}
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
                isScanning ? <Loader2 className="w-3 h-3 animate-spin text-[#C9A84C]" /> : <ChevronRight className="w-3.5 h-3.5" />
              ) : (
                isTreeNode && node.expanded ? <ChevronDown className="w-3.5 h-3.5 text-[#C9A84C]" /> : <ChevronRight className="w-3.5 h-3.5" />
              )
            ) : (
              <span className="w-1 h-1 rounded-full bg-[#EAE6DF]/30" />
            )}
          </span>

          <div className="flex items-center gap-2 flex-1 min-w-0">
            {item.is_dir ? (
              <FolderOpen className={cn("w-3.5 h-3.5 shrink-0", isTreeNode && node.expanded ? "text-[#C9A84C]" : "text-[#C9A84C]/70")} />
            ) : (
              <File className="w-3.5 h-3.5 shrink-0 text-[#EAE6DF]/70" />
            )}
            <span className={cn(
              "truncate font-medium transition-colors group-hover:text-[#FAF8F5] text-[11px]",
              item.is_dir ? "text-[#C9A84C]" : "text-[#EAE6DF]",
              isSelected && "text-[#C9A84C] font-semibold"
            )}>
              {item.name}
            </span>
          </div>

          <span className="w-20 text-right shrink-0 font-mono text-[10px]">
            {isScanning ? (
              <span className="text-[#C9A84C] animate-pulse">Scanning...</span>
            ) : (
              <span className="text-[#FAF8F5]">{formatSize(item.logical_size)}</span>
            )}
          </span>

          <span className="w-16 text-right shrink-0 font-mono text-[11px] text-[#888899]">
            {!isScanning && item.is_dir ? formatNumber(item.files) : "-"}
          </span>

          <span className="w-16 text-right shrink-0 font-mono text-[11px] text-[#888899]">
            {!isScanning && item.is_dir ? formatNumber(item.subdirs) : "-"}
          </span>

          <span className="w-28 flex items-center justify-end gap-2 shrink-0 pr-2">
            {!isScanning && (
              <>
                <SizeBar
                  ratio={pct / 100}
                  color={item.is_dir ? "#C9A84C" : "#EAE6DF"}
                  width={60}
                />
                <span className="font-mono text-[11px] text-[#888899] min-w-[36px] text-right">
                  {pct.toFixed(1)}%
                </span>
              </>
            )}
          </span>

          <span className="w-32 shrink-0 flex items-center justify-center">
            {analyses?.get(item.path) && (
              <StarRating priority={analyses.get(item.path)!.priority} />
            )}
          </span>

          <span className="flex-1 min-w-0 truncate text-xs text-[#888899] group-hover:text-[#FAF8F5] transition-colors">
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
        <Loader2 className="w-4 h-4 animate-spin" /> Retrieving Telemetry...
      </div>
    );
  }

  if (!scanning && !rootPath) {
    return (
      <div className="h-full flex items-center justify-center text-[#888899] font-mono text-sm uppercase tracking-widest">
        System idling. Initiate scan.
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[#0D0D12] rounded-2xl border border-[#2A2A35] overflow-hidden shadow-2xl">
      <div className="flex items-center gap-3 px-4 py-3 bg-[#13131A] border-b border-[#2A2A35] shrink-0 font-mono text-[10px] uppercase tracking-[0.1em] text-[#888899]">
        <span className="flex-1 min-w-0 pl-14">Identifier</span>
        <span className="w-24 text-right">Size</span>
        <span className="w-16 text-right">Files</span>
        <span className="w-16 text-right">Dirs</span>
        <span className="w-28 text-right pr-2">Allocation</span>
        <span className="w-32 flex items-center justify-center gap-2">
          AI Status
          {onAnalyzeSelected && selected && selected.size > 0 && (
            <button
              className="px-2 py-0.5 rounded bg-[#7B61FF]/20 text-[#7B61FF] border border-[#7B61FF]/40 hover:bg-[#7B61FF]/30 transition-colors flex items-center gap-1"
              disabled={analyzing}
              onClick={onAnalyzeSelected}
            >
              {analyzing ? <Loader2 className="w-3 h-3 animate-spin" /> : <Sparkles className="w-3 h-3" />}
              {analyzing ? "ANALYZING" : `ANALYZE (${selected.size})`}
            </button>
          )}
        </span>
        <span className="flex-1 min-w-0">Telemetry Notes</span>
      </div>
      <div ref={listRef} className="flex-1 overflow-y-auto py-2 custom-scrollbar">
        {displayItems.map((item) => renderRow(item, 0, totalSize))}
      </div>
    </div>
  );
}
