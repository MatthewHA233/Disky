import { useEffect, useState, useMemo } from "react";
import type { DirEntry } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import { SizeBar } from "./SizeBar";

interface Props {
  rootPath: string | null;
  liveChildren: DirEntry[];
  scanning: boolean;
  onSelect?: (path: string, size: number) => void;
  selected?: Map<string, number>;
}

interface TreeNode extends DirEntry {
  children?: TreeNode[];
  expanded?: boolean;
  loaded?: boolean;
}

export function DirectoryTree({ rootPath, liveChildren, scanning, onSelect, selected }: Props) {
  const [nodes, setNodes] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);

  // Scanning mode: use liveChildren sorted by size
  const sortedLive = useMemo(() => {
    return [...liveChildren].sort((a, b) => b.logical_size - a.logical_size);
  }, [liveChildren]);

  const displayItems = scanning ? sortedLive : nodes;

  const totalSize = useMemo(() => {
    return displayItems.reduce((s, i) => s + i.logical_size, 0);
  }, [displayItems]);

  // Load root children when scan completes (scanning: true->false with rootPath set)
  // Also reload when rootPath changes
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

  const renderRow = (item: DirEntry | TreeNode, depth: number, parentSize: number) => {
    const node = item as TreeNode;
    const isTreeNode = "expanded" in node;
    const pct = parentSize > 0 ? (item.logical_size / parentSize) * 100 : 0;
    const isScanning = scanning && item.is_dir && item.logical_size === 0;

    return (
      <div key={item.path}>
        <div
          className="tree-row"
          style={{ paddingLeft: depth * 20 }}
          onClick={() => !scanning && item.is_dir && toggleNode(item.path)}
        >
          {onSelect && !scanning && (
            <input
              type="checkbox"
              checked={selected?.has(item.path) ?? false}
              onChange={(e) => { e.stopPropagation(); onSelect(item.path, item.logical_size); }}
              onClick={(e) => e.stopPropagation()}
            />
          )}
          <span className="tree-toggle">
            {item.is_dir
              ? scanning
                ? (isScanning ? "●" : "▶")
                : (isTreeNode && node.expanded ? "▼" : "▶")
              : " "}
          </span>
          <span className={`tree-name ${item.is_dir ? "dir" : "file"}`}>
            {item.name}
          </span>
          <span className="tree-col-size">
            {isScanning ? (
              <span className="scanning-text">扫描中...</span>
            ) : (
              formatSize(item.logical_size)
            )}
          </span>
          <span className="tree-col-files">
            {!isScanning && item.is_dir ? formatNumber(item.files) : ""}
          </span>
          <span className="tree-col-dirs">
            {!isScanning && item.is_dir ? formatNumber(item.subdirs) : ""}
          </span>
          <span className="tree-col-pct">
            {!isScanning && (
              <>
                <SizeBar
                  ratio={pct / 100}
                  color={item.is_dir ? "var(--dir-color)" : "var(--file-color)"}
                  width={60}
                />
                <span className="pct-text">{pct.toFixed(1)}%</span>
              </>
            )}
          </span>
        </div>
        {!scanning && isTreeNode && node.expanded && node.children && (
          <div className="tree-children">
            {node.children.map((c) => renderRow(c, depth + 1, item.logical_size))}
          </div>
        )}
      </div>
    );
  };

  if (!scanning && loading) {
    return <div className="status-msg">加载中...</div>;
  }

  if (!scanning && !rootPath) {
    return <div className="status-msg">请选择磁盘并点击"扫描"开始分析。</div>;
  }

  return (
    <div className="dir-tree">
      <div className="tree-header">
        <span className="tree-hdr-name">名称</span>
        <span className="tree-hdr-size">大小</span>
        <span className="tree-hdr-files">文件数</span>
        <span className="tree-hdr-dirs">目录数</span>
        <span className="tree-hdr-pct">占比</span>
      </div>
      <div className="tree-body">
        {displayItems.map((item) => renderRow(item, 0, totalSize))}
      </div>
    </div>
  );
}
