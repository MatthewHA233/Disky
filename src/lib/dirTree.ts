// 目录树状态（纯函数，不可变更新）+ 拍平成虚拟列表的行。
// 扫描根作为一个虚拟节点持有顶层子项，顶层与子目录共用同一套展开/分页逻辑。

import type { ChildrenPage, DirEntry } from "../types";
import { isSameOrInside } from "./path";

/** 每次向后端取的子项数；超出部分在滚动到列表末尾时自动续取 */
export const PAGE_SIZE = 500;

export interface TreeNode extends DirEntry {
  children?: TreeNode[];
  expanded?: boolean;
  loaded?: boolean;
  /** 子项总数（后端分页）；children 少于 total 表示还有未加载的 */
  total?: number;
}

/** 行在「当前目录」高亮框中的位置（原嵌套渲染时的 ring 容器，拍平后逐行拼出） */
export type GroupPos = "none" | "first" | "middle" | "last";

interface RowBase {
  key: string;
  depth: number;
  group: GroupPos;
}

export interface EntryRow extends RowBase {
  kind: "entry";
  node: TreeNode;
  parentSize: number;
}

/** 分页占位行：滚动到这里时自动加载该目录的下一页 */
export interface MoreRow extends RowBase {
  kind: "more";
  parentPath: string;
  remaining: number;
}

export type TreeRow = EntryRow | MoreRow;

export function toNodes(entries: DirEntry[]): TreeNode[] {
  return entries.map((e) => ({ ...e, expanded: false, loaded: false }));
}

export function makeRoot(path: string, page: ChildrenPage): TreeNode {
  return {
    name: path,
    path,
    files: 0,
    subdirs: 0,
    logical_size: page.parent_size,
    is_dir: true,
    expanded: true,
    loaded: true,
    total: page.total,
    children: toNodes(page.entries),
  };
}

export function findNode(node: TreeNode, path: string): TreeNode | undefined {
  if (node.path === path) return node;
  if (!node.children || !isSameOrInside(node.path, path)) return undefined;
  for (const child of node.children) {
    const found = findNode(child, path);
    if (found) return found;
  }
  return undefined;
}

/** 不可变地更新 path 对应的节点：只复制祖先链，其余子树原样复用 */
export function updateNode(
  node: TreeNode,
  path: string,
  fn: (n: TreeNode) => TreeNode
): TreeNode {
  if (node.path === path) return fn(node);
  if (!node.children || !isSameOrInside(node.path, path)) return node;
  const children = node.children.map((c) => updateNode(c, path, fn));
  return children.some((c, i) => c !== node.children![i]) ? { ...node, children } : node;
}

/** 依次展开 paths；尚未加载的目录用 pages 中取到的首页填充 */
export function expandWithPages(
  root: TreeNode,
  paths: string[],
  pages: Map<string, ChildrenPage>
): TreeNode {
  return paths.reduce(
    (acc, p) =>
      updateNode(acc, p, (n) => {
        const page = pages.get(p);
        if (n.is_dir && !n.loaded && page) {
          return {
            ...n,
            expanded: true,
            loaded: true,
            total: page.total,
            children: toNodes(page.entries),
          };
        }
        return { ...n, expanded: true };
      }),
    root
  );
}

export interface ExpandPages {
  pages: Map<string, ChildrenPage>;
  /** 某一级取页失败时的错误；此前取到的页仍然有效，可照常应用 */
  error: unknown;
}

/** 为依次展开 paths 预取所需首页。嵌套路径要等父目录加载后才能定位，所以串行；
 *  某一级失败即停止（更深的路径依赖它），已取到的页照常返回 */
export async function fetchExpandPages(
  root: TreeNode,
  paths: string[],
  fetchPage: (path: string) => Promise<ChildrenPage>
): Promise<ExpandPages> {
  const pages = new Map<string, ChildrenPage>();
  let working = root;
  for (const p of paths) {
    const node = findNode(working, p);
    if (!node) continue;
    if (node.is_dir && !node.loaded) {
      try {
        pages.set(p, await fetchPage(p));
      } catch (error) {
        return { pages, error };
      }
    }
    working = expandWithPages(working, [p], pages);
  }
  return { pages, error: null };
}

/** 追加一页子项。offset 与已加载数不一致视为过期结果丢弃；
 *  另按路径去重：页之间若有排序变化（祖先大小刚被刷新），避免重复行与重复 key */
export function appendPage(
  root: TreeNode,
  path: string,
  offset: number,
  page: ChildrenPage
): TreeNode {
  return updateNode(root, path, (n) => {
    const children = n.children ?? [];
    if (children.length !== offset) return n;
    const seen = new Set(children.map((c) => c.path));
    const fresh = page.entries.filter((e) => !seen.has(e.path));
    // 整页都是重复项说明排序已大幅变化，继续翻页只会反复取到同一页：就此结束分页
    // （树变化后 App 会整体重挂载目录树，重新从第一页加载）
    if (fresh.length === 0) return { ...n, total: children.length };
    return { ...n, total: page.total, children: [...children, ...toNodes(fresh)] };
  });
}

export function pagePaths(pages: Iterable<ChildrenPage>): string[] {
  return [...pages].flatMap((p) => p.entries.map((e) => e.path));
}

/** 扫描进行中：只有顶层的实时快照，按大小降序 */
export function liveRows(entries: DirEntry[]): TreeRow[] {
  const sorted = [...entries].sort((a, b) => b.logical_size - a.logical_size);
  const total = sorted.reduce((s, e) => s + e.logical_size, 0);
  return sorted.map((e) => ({
    kind: "entry",
    key: e.path,
    depth: 0,
    group: "none",
    node: { ...e, expanded: false, loaded: false },
    parentSize: total,
  }));
}

/** 把展开的树拍平成可见行（虚拟列表的数据源）；根自身不出行 */
export function flattenTree(root: TreeNode, activePath: string | null): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (node: TreeNode, depth: number) => {
    const children = node.children ?? [];
    for (const child of children) {
      rows.push({
        kind: "entry",
        key: child.path,
        depth,
        group: "none",
        node: child,
        parentSize: node.logical_size,
      });
      if (child.expanded && child.children) walk(child, depth + 1);
    }
    const remaining = (node.total ?? children.length) - children.length;
    if (remaining > 0) {
      rows.push({
        kind: "more",
        key: `more:${node.path}`,
        depth,
        group: "none",
        parentPath: node.path,
        remaining,
      });
    }
  };
  walk(root, 0);
  return markActiveGroup(rows, activePath);
}

/** 当前目录展开且有子项时，它与其整棵可见子树圈在一个高亮框里 */
function markActiveGroup(rows: TreeRow[], activePath: string | null): TreeRow[] {
  if (!activePath) return rows;
  const start = rows.findIndex((r) => r.kind === "entry" && r.node.path === activePath);
  const head = rows[start];
  if (!head || head.kind !== "entry" || !head.node.expanded || !head.node.children?.length) {
    return rows;
  }
  let end = start + 1;
  while (end < rows.length && rows[end].depth > head.depth) end++;
  return rows.map((r, i): TreeRow => {
    if (i < start || i >= end) return r;
    const group: GroupPos = i === start ? "first" : i === end - 1 ? "last" : "middle";
    return { ...r, group };
  });
}
