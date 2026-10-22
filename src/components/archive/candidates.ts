// 交给 AI 的东西怎么挑：归类的候选、核对缓存的候选、按猜测一键放好的范围。

import type { ArchiveApp, ArchiveCheckItem, ArchiveClassifyItem } from "../../types";
import { formatSize } from "../../lib/format";
import { shortenHome } from "../../lib/path";
import { guessOf, placementOf, visibleKids, type ArchiveIndex, type FolderNode, type PendingJunk } from "../../lib/archiveModel";

/** 交给 AI 的文件夹：上面两层里不小于这个大小的 */
const AI_FOLDER_MIN = 1024 * 1024 * 1024;
const AI_FOLDER_DEPTH = 2;
const AI_FOLDER_LIMIT = 50;
/** 一次最多核对多少块（按大小从大到小），免得一次花太多 AI 额度 */
const CHECK_LIMIT = 240;

export function appCandidates(apps: ArchiveApp[]): ArchiveClassifyItem[] {
  return apps.map((a) => {
    const size = a.parts.reduce((s, p) => s + p.size, 0);
    const where = a.parts.slice(0, 3).map((p) => shortenHome(p.path)).join("；");
    const who = a.publisher ? `发布者：${a.publisher}；` : "";
    return { key: a.key, name: a.name, detail: `${who}位置：${where}；共 ${formatSize(size)}${a.uninstalled ? "；像是已卸载" : ""}` };
  });
}

/**
 * 文件夹只交上面两层：深处的 bin、site-packages 脱离上下文没法判断，放进箱子也只会把整理弄碎。
 * 用途混杂的上层文件夹 AI 会答「看不出」，它的子文件夹在同一批里单独判断。
 */
export function folderCandidates(ix: ArchiveIndex, sizes: Map<string, number>): ArchiveClassifyItem[] {
  const folders: (ArchiveClassifyItem & { size: number })[] = [];
  const walk = (n: FolderNode) => {
    if (placementOf(ix, n.key)) return;
    const kids = visibleKids(ix, n, sizes);
    const size = sizes.get(n.key) ?? 0;
    if (n.depth > 0 && size >= AI_FOLDER_MIN) {
      const inside = kids.slice(0, 5).map((k) => k.name).join("、");
      folders.push({ key: n.key, name: n.name, size, detail: `文件夹，位置：${shortenHome(n.path)}；共 ${formatSize(size)}${inside ? `；里面最大的：${inside}` : ""}` });
    }
    if (n.depth < AI_FOLDER_DEPTH) kids.forEach(walk);
  };
  ix.roots.forEach(walk);
  folders.sort((a, b) => b.size - a.size);
  return folders.slice(0, AI_FOLDER_LIMIT).map(({ key, name, detail }) => ({ key, name, detail }));
}

/** 交给 AI 查归属：一次最多几个文件夹、最小多大 */
const ATTRIBUTE_LIMIT = 30;
const ATTRIBUTE_MIN = 200 * 1024 * 1024;
/** 装着很多软件的目录本身不交，往里一层（系统的目录结构，各台电脑都一样） */
const CONTAINERS = /^(users|appdata|local|locallow|roaming|programdata|program files|program files \(x86\)|programs|library|application support)$/i;

export interface AttributeCandidate {
  node: FolderNode;
  size: number;
  /** 名字像是个人文件（照片、项目、文档……）：默认不交给 AI，免得把个人文件夹的内容发出去 */
  personal: boolean;
}

/** 交给 AI 查「是哪个软件的」的文件夹：从上往下，跳过容器目录，按大小取最大的一批 */
export function attributeCandidates(ix: ArchiveIndex, sizes: Map<string, number>): AttributeCandidate[] {
  const out: AttributeCandidate[] = [];
  const walk = (n: FolderNode) => {
    const size = sizes.get(n.key) ?? 0;
    if (!n.isDir || size < ATTRIBUTE_MIN || placementOf(ix, n.key)) return;
    const container = n.depth === 0 || CONTAINERS.test(n.name) || n.parent?.name.toLowerCase() === "users";
    if (container) visibleKids(ix, n, sizes).forEach(walk);
    else out.push({ node: n, size, personal: !!n.hint });
  };
  ix.roots.forEach(walk);
  return out.sort((a, b) => b.size - a.size).slice(0, ATTRIBUTE_LIMIT);
}

export function checkCandidates(pending: PendingJunk[]): ArchiveCheckItem[] {
  return pending.slice(0, CHECK_LIMIT).map(({ part, app }) => ({ path: part.path, owner: app.name, kind: part.kind, size: part.size }));
}

/** 待整理目录树里带猜测的文件夹（从上往下，上级放了下级就不用再放） */
export function folderGuesses(ix: ArchiveIndex, sizes: Map<string, number>): { key: string; box: string }[] {
  const out: { key: string; box: string }[] = [];
  const walk = (n: FolderNode) => {
    const g = n.depth > 0 ? guessOf(ix, n.key) : null;
    if (g) {
      out.push({ key: n.key, box: g.box });
      return;
    }
    visibleKids(ix, n, sizes).forEach(walk);
  };
  ix.roots.forEach(walk);
  return out;
}
