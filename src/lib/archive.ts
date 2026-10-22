// 「性质」透镜：目录视图的矩形图按性质着色用的分类元数据与纯函数。

import type { Footprint, FootprintKind } from "../types";
import { normalizePath, SEP } from "./path";

export interface KindMeta {
  name: string;
  color: string;
  /** 格子顶部的一句说明 */
  hint: string;
  /** 可以放心清理的性质：格子标题栏提供「全选」 */
  cleanable: boolean;
}

export const KIND_ORDER: FootprintKind[] = ["data", "cache", "regen", "prog", "residue", "system"];

export const KINDS: Record<FootprintKind, KindMeta> = {
  data: { name: "你的数据", color: "#E9B949", hint: "你自己产生的文件，清理前要确认。", cleanable: false },
  cache: { name: "缓存", color: "#F2994A", hint: "可以安全清理，软件会按需重新生成。", cleanable: true },
  regen: { name: "可重新下载", color: "#5BC98C", hint: "删了以后能重新下载，不常用的可以先删。", cleanable: true },
  prog: { name: "程序本体", color: "#8B8FF7", hint: "软件本身，要释放请通过卸载。", cleanable: false },
  residue: { name: "安装包与残留", color: "#B794F6", hint: "软件装好以后就没用了，可以删。", cleanable: true },
  system: { name: "系统", color: "#9CA3AF", hint: "系统运行需要，不要手动删除。", cleanable: false },
};

export interface KindShare {
  kind: FootprintKind;
  /** 主导性质占这块空间的比例（0–1），混杂的目录颜色更淡 */
  purity: number;
}

/**
 * 给目录视图着色用：一个路径（大小 size）主要是什么性质。
 * 落在某个条目里面 → 就是它的性质；包含若干条目 → 取占比最大的；
 * 没被单独认出的部分算进最近的「其他文件」条目。结果按路径缓存。
 */
export function makeKindLens(items: Footprint[]): (path: string, size: number) => KindShare | null {
  // 路径都预先规范化成小写，比较时不再逐次规范化（每次着色要比较 条目数 × 块数 次）
  const prepared = items.map((f) => ({ f, key: normalizePath(f.path).toLowerCase() }));
  const inside = (root: string, target: string) =>
    target === root || target.startsWith(root.endsWith(SEP) ? root : root + SEP);
  const cache = new Map<string, KindShare | null>();

  const compute = (key: string, size: number): KindShare | null => {
    let owner: { f: Footprint; len: number } | null = null;
    let rest: { f: Footprint; len: number } | null = null;
    const sums = new Map<FootprintKind, number>();
    let covered = 0;
    for (const { f, key: fk } of prepared) {
      if (inside(fk, key)) {
        if (!f.remainder && (!owner || fk.length > owner.len)) owner = { f, len: fk.length };
        else if (f.remainder && fk !== key && (!rest || fk.length > rest.len)) rest = { f, len: fk.length };
      }
      if (inside(key, fk)) {
        sums.set(f.kind, (sums.get(f.kind) ?? 0) + f.size);
        covered += f.size;
      }
    }
    if (owner) return { kind: owner.f.kind, purity: 1 };
    if (rest && size > covered) sums.set(rest.f.kind, (sums.get(rest.f.kind) ?? 0) + size - covered);
    let best: FootprintKind | null = null;
    let bestSize = 0;
    for (const [k, v] of sums) if (v > bestSize) { best = k; bestSize = v; }
    return best ? { kind: best, purity: Math.min(1, bestSize / Math.max(size, covered)) } : null;
  };

  return (path, size) => {
    const key = normalizePath(path).toLowerCase();
    if (!cache.has(key)) cache.set(key, compute(key, size));
    return cache.get(key)!;
  };
}
