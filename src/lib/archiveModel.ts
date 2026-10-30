// 档案馆的纯计算：每一项属于哪个门类、放在哪个箱子（用户 > AI > 内置规则）、
// 各箱子和待整理的大小、每块盘的整理进度、哪些缓存经过 AI 核对可以放心删。
// 只依赖 get_archive 返回的数据，不改动传入的对象。

import type {
  ArchiveAiPlacement, ArchiveApp, ArchiveBoxRow, ArchiveCacheCheck, ArchiveCategory, ArchiveFolder, ArchiveModel,
  ArchivePart, PartKind,
} from "../types";

/** AI 把握不低于该值才直接放进箱子，否则留在待整理、只作为猜测 */
export const AI_MIN_CONFIDENCE = 0.6;
/** 与后端一致：待整理目录树只列不小于该大小的位置（文件和文件夹一样），更小的合成一行 */
export const MIN_FOLDER = 50 * 1024 * 1024;
export const JUNK_KINDS: readonly PartKind[] = ["cache", "residue"];
export const CATEGORIES: readonly ArchiveCategory[] = ["software", "personal", "devenv"];

export type Source = "user" | "ai" | "rule";

export interface Placement {
  box: string;
  source: Source;
  confidence?: number;
  reason?: string;
}

export interface Guess {
  box: string;
  source: "ai" | "rule";
  confidence?: number;
  reason?: string;
}

export interface FolderNode {
  key: string;
  path: string;
  name: string;
  isDir: boolean;
  depth: number;
  rest: number;
  hint: string | null;
  parent: FolderNode | null;
  kids: FolderNode[];
}

export type Item =
  | { type: "app"; key: string; name: string; app: ArchiveApp }
  /** where：箱子里有重名的文件夹时，用来区分的上级名字 */
  | { type: "dir"; key: string; name: string; node: FolderNode; where?: string };

export interface ArchiveIndex {
  apps: Map<string, ArchiveApp>;
  folders: Map<string, FolderNode>;
  roots: FolderNode[];
  boxes: ArchiveBoxRow[];
  boxById: Map<string, ArchiveBoxRow>;
  user: Map<string, string | null>;
  ai: Map<string, ArchiveAiPlacement>;
  /** 小写路径 → AI 核对结论 */
  checks: Map<string, ArchiveCacheCheck>;
  /** 软件 / 文件夹 key → 和上次扫描比变了多少（字节） */
  growth: Map<string, number>;
  /** 盘根（大写盘符或卷名）→ 上次扫描的时间 */
  growthSince: Map<string, number>;
}

/** 变化不到这个就不提（扫描时总有些日志、缓存在动） */
export const GROWTH_SHOWN = 100 * 1024 * 1024;

/** 值得提一句的大小变化（没有、或者太小时为 undefined） */
export function growthOf(ix: ArchiveIndex, key: string): number | undefined {
  const g = ix.growth.get(key);
  return g !== undefined && Math.abs(g) >= GROWTH_SHOWN ? g : undefined;
}

export function indexModel(m: ArchiveModel): ArchiveIndex {
  const folders = new Map<string, FolderNode>();
  const build = (f: ArchiveFolder, parent: FolderNode | null): FolderNode => {
    const node: FolderNode = { key: f.key, path: f.path, name: f.name, isDir: f.is_dir, depth: f.depth, rest: f.rest, hint: f.hint, parent, kids: [] };
    node.kids = f.kids.map((k) => build(k, node));
    folders.set(node.key, node);
    return node;
  };
  const roots = m.tree.map((f) => build(f, null));
  return {
    apps: new Map(m.apps.map((a) => [a.key, a])),
    folders,
    roots,
    boxes: m.boxes,
    boxById: new Map(m.boxes.map((b) => [b.id, b])),
    user: new Map(m.user.map((u) => [u.key, u.box_id])),
    ai: new Map(m.ai.map((a) => [a.key, a])),
    checks: new Map((m.checks ?? []).map((c) => [c.path.toLowerCase(), c])),
    growth: new Map([...Object.entries(m.growth?.apps ?? {}), ...Object.entries(m.growth?.folders ?? {})]),
    growthSince: new Map(Object.entries(m.growth?.since ?? {}).map(([root, at]) => [root.toUpperCase(), at])),
  };
}

/** 一项现在放在哪：用户的决定优先，其次是有把握的 AI 判断，最后是内置规则；都没有就在待整理 */
export function placementOf(ix: ArchiveIndex, key: string): Placement | null {
  if (ix.user.has(key)) {
    const box = ix.user.get(key);
    return box && ix.boxById.has(box) ? { box, source: "user" } : null;
  }
  const ai = ix.ai.get(key);
  if (ai && ai.confidence >= AI_MIN_CONFIDENCE && ix.boxById.has(ai.box_id) && !ancestorPlaced(ix, key)) {
    return { box: ai.box_id, source: "ai", confidence: ai.confidence, reason: ai.reason };
  }
  const hint = ix.apps.get(key)?.hint;
  if (hint && ix.boxById.has(hint)) return { box: hint, source: "rule" };
  return null;
}

/** 文件夹的上级已经放进箱子：AI 对它的单独判断不再采用（用户亲手放的照样算） */
function ancestorPlaced(ix: ArchiveIndex, key: string): boolean {
  for (let p = ix.folders.get(key)?.parent ?? null; p; p = p.parent) {
    if (placementOf(ix, p.key)) return true;
  }
  return false;
}

/** 给待整理里的项一个建议：AI 的判断（哪怕把握不高），没有就用内置规则（文件夹按名字猜的也算） */
export function guessOf(ix: ArchiveIndex, key: string): Guess | null {
  const ai = ix.ai.get(key);
  if (ai && ix.boxById.has(ai.box_id)) return { box: ai.box_id, source: "ai", confidence: ai.confidence, reason: ai.reason };
  const hint = ix.apps.get(key)?.hint ?? ix.folders.get(key)?.hint;
  return hint && ix.boxById.has(hint) ? { box: hint, source: "rule" } : null;
}

/** 一项天生属于哪个门类：软件 / 插件与开发环境由后端认出，没认出归属的文件夹是个人文件 */
export function naturalCategory(item: Item): ArchiveCategory {
  return item.type === "app" ? item.app.category : "personal";
}

/** 一项现在显示在哪个页签：放进箱子的跟着箱子走，待整理的按天生门类 */
export function categoryOf(ix: ArchiveIndex, item: Item): ArchiveCategory {
  const p = placementOf(ix, item.key);
  return (p && ix.boxById.get(p.box)?.category) || naturalCategory(item);
}

export const appSize = (a: ArchiveApp) => a.parts.reduce((s, p) => s + p.size, 0);

/** 缓存、残留里 Disky 删得动的（受保护的系统目录里的只展示） */
export const isJunkCandidate = (p: ArchivePart) => JUNK_KINDS.includes(p.kind) && p.deletable;
export const checkOf = (ix: ArchiveIndex, p: ArchivePart) => ix.checks.get(p.path.toLowerCase());
/** 经过 AI 核对、确定删了不丢东西的才算可清理；用户自己指定成缓存 / 残留的，用户说了算 */
export const isCleanable = (ix: ArchiveIndex, p: ArchivePart) =>
  isJunkCandidate(p) && (p.origin === "user" || checkOf(ix, p)?.verdict === "safe");
/** 还没核对过、或者 AI 也拿不准的候选 */
export const isUnchecked = (ix: ArchiveIndex, p: ArchivePart) => {
  const v = checkOf(ix, p)?.verdict;
  return isJunkCandidate(p) && p.origin !== "user" && (v === undefined || v === "unsure");
};
export const appJunk = (ix: ArchiveIndex, a: ArchiveApp) => a.parts.reduce((s, p) => s + (isCleanable(ix, p) ? p.size : 0), 0);

/** 各盘分别占了多少 */
export function sizeByDrive(a: { parts: readonly ArchivePart[] }, driveOf: (path: string) => string): Map<string, number> {
  const out = new Map<string, number>();
  for (const p of a.parts) {
    const d = driveOf(p.path);
    out.set(d, (out.get(d) ?? 0) + p.size);
  }
  return out;
}

/**
 * 每个待整理位置扣掉「里面已经单独放进箱子的子位置」后的大小。
 * 文件夹放进箱子后，它下面没单独放的部分都随它算整理好了。
 */
export function folderSizes(ix: ArchiveIndex): Map<string, number> {
  const out = new Map<string, number>();
  const placedWithin = (n: FolderNode): number =>
    n.kids.reduce((s, k) => s + (placementOf(ix, k.key) ? k.rest : placedWithin(k)), 0);
  const visit = (n: FolderNode) => {
    out.set(n.key, Math.max(0, n.rest - placedWithin(n)));
    n.kids.forEach(visit);
  };
  ix.roots.forEach(visit);
  return out;
}

export function itemOf(ix: ArchiveIndex, key: string): Item | null {
  const app = ix.apps.get(key);
  if (app) return { type: "app", key, name: app.name, app };
  const node = ix.folders.get(key);
  return node ? { type: "dir", key, name: node.name, node } : null;
}

export function itemSize(item: Item, sizes: Map<string, number>): number {
  return item.type === "app" ? appSize(item.app) : sizes.get(item.key) ?? 0;
}

export const itemJunk = (ix: ArchiveIndex, item: Item) => (item.type === "app" ? appJunk(ix, item.app) : 0);

/** 同一个箱子里重名的文件夹带上上级的名字（三个 Documents 才分得清是谁的）；上级也重名就带上级的完整路径 */
function withDistinctNames(items: Item[]): Item[] {
  const same = new Map<string, FolderNode[]>();
  for (const it of items) if (it.type === "dir") same.set(it.name, [...(same.get(it.name) ?? []), it.node]);
  return items.map((it) => {
    const group = it.type === "dir" ? same.get(it.name) ?? [] : [];
    if (it.type !== "dir" || group.length < 2 || !it.node.parent) return it;
    const parent = it.node.parent;
    const clash = group.filter((n) => n.parent?.name === parent.name).length > 1;
    return { ...it, where: clash ? parent.path : parent.name };
  });
}

export interface BoxContent {
  items: Item[];
  size: number;
  junk: number;
}

export interface DriveStat {
  /** 盘根（C:） */
  root: string;
  total: number;
  scannedAt: number;
  live: boolean;
  organized: number;
  byCategory: Record<ArchiveCategory, number>;
}

/** 等 AI 核对的一块缓存 / 残留 */
export interface PendingJunk {
  part: ArchivePart;
  app: ArchiveApp;
}

export interface ArchiveStats {
  total: number;
  organized: number;
  boxes: Map<string, BoxContent>;
  /** 各门类整理好的大小 */
  byCategory: Record<ArchiveCategory, number>;
  /** 还没进箱子的软件 / 开发环境（按大小降序） */
  inbox: Record<"software" | "devenv", ArchiveApp[]>;
  /** 没认出归属、也没放进箱子的空间 */
  looseSize: number;
  /** 待整理目录树里还剩几处（最底层的位置） */
  looseCount: number;
  /** 最深一处在第几层（盘根为 0） */
  deepest: number;
  /** AI 核对过、可以放心清理的 */
  junk: number;
  /** 还没核对的候选 */
  pending: PendingJunk[];
  pendingSize: number;
  bySource: Record<Source, number>;
  drives: DriveStat[];
}

const zero = (): Record<ArchiveCategory, number> => ({ software: 0, personal: 0, devenv: 0 });

export function computeStats(
  ix: ArchiveIndex,
  sizes: Map<string, number>,
  drives: { root: string; total: number; scanned_at: number; live?: boolean }[],
  driveOf: (path: string) => string,
): ArchiveStats {
  const boxes = new Map<string, BoxContent>(ix.boxes.map((b) => [b.id, { items: [], size: 0, junk: 0 }]));
  const bySource: Record<Source, number> = { user: 0, ai: 0, rule: 0 };
  const byCategory = zero();
  const inbox: ArchiveStats["inbox"] = { software: [], devenv: [] };
  const driveStats = new Map<string, DriveStat>(
    drives.map((d) => [driveOf(d.root), { root: d.root, total: d.total, scannedAt: d.scanned_at, live: d.live ?? false, organized: 0, byCategory: zero() }]),
  );
  const addDrive = (path: string, cat: ArchiveCategory, size: number) => {
    const d = driveStats.get(driveOf(path));
    if (!d) return;
    d.byCategory[cat] += size;
    d.organized += size;
  };
  const pending: PendingJunk[] = [];
  let junk = 0;

  const put = (p: Placement, item: Item) => {
    const box = boxes.get(p.box);
    const cat = ix.boxById.get(p.box)?.category;
    if (!box || !cat) return;
    const size = itemSize(item, sizes);
    if (size <= 0) return;
    box.items.push(item);
    box.size += size;
    box.junk += itemJunk(ix, item);
    byCategory[cat] += size;
    bySource[p.source] += 1;
    if (item.type === "app") item.app.parts.forEach((pt) => addDrive(pt.path, cat, pt.size));
    else addDrive(item.node.path, cat, size);
  };
  for (const app of ix.apps.values()) {
    for (const part of app.parts) {
      if (isCleanable(ix, part)) junk += part.size;
      else if (isUnchecked(ix, part)) pending.push({ part, app });
    }
    const p = placementOf(ix, app.key);
    if (p) put(p, { type: "app", key: app.key, name: app.name, app });
    else if (appSize(app) > 0) inbox[app.category].push(app);
  }
  for (const node of ix.folders.values()) {
    if (node.depth === 0) continue;
    const p = placementOf(ix, node.key);
    // 上级文件夹也放进了箱子时，它单独算（上级的大小已经扣掉了它）
    if (p) put(p, { type: "dir", key: node.key, name: node.name, node });
  }
  for (const box of boxes.values()) {
    box.items = withDistinctNames([...box.items].sort((a, b) => itemSize(b, sizes) - itemSize(a, sizes)));
  }
  inbox.software.sort((a, b) => appSize(b) - appSize(a));
  inbox.devenv.sort((a, b) => appSize(b) - appSize(a));
  pending.sort((a, b) => b.part.size - a.part.size);

  let looseCount = 0;
  let deepest = 0;
  const walk = (n: FolderNode) => {
    if (placementOf(ix, n.key)) return;
    const kids = visibleKids(ix, n, sizes);
    if (n.depth > 0 && (sizes.get(n.key) ?? 0) >= MIN_FOLDER) {
      deepest = Math.max(deepest, n.depth);
      if (kids.length === 0) looseCount += 1;
    }
    kids.forEach(walk);
  };
  ix.roots.forEach(walk);
  const looseSize = ix.roots.reduce((s, r) => s + (sizes.get(r.key) ?? 0), 0);
  const organized = [...boxes.values()].reduce((s, b) => s + b.size, 0);
  const total = drives.reduce((s, d) => s + d.total, 0);
  return {
    total,
    organized,
    boxes,
    byCategory,
    inbox,
    looseSize,
    looseCount,
    deepest,
    junk,
    pending,
    pendingSize: pending.reduce((s, p) => s + p.part.size, 0),
    bySource,
    drives: [...driveStats.values()],
  };
}

/** 一个位置下没单独显示的部分（小于下限的子位置、没记进摘要的零散文件），点开时去后端现场列 */
export interface Loose {
  size: number;
  /** 树上单独显示着、或者放进了箱子的子位置：不用再列 */
  skip: string[];
  /** 树上有、小到不显示的子位置和扣过箱子后的大小 */
  known: [string, number][];
}

/** 零散的不到这个就不单独给一行 */
export const LOOSE_SHOWN = 1024 * 1024;

export function looseOf(ix: ArchiveIndex, n: FolderNode, sizes: Map<string, number>): Loose {
  const shown = new Set(visibleKids(ix, n, sizes).map((k) => k.key));
  const skip: string[] = [];
  const known: [string, number][] = [];
  let size = sizes.get(n.key) ?? 0;
  for (const k of n.kids) {
    if (shown.has(k.key)) {
      size -= sizes.get(k.key) ?? 0;
      skip.push(k.path);
    } else if (placementOf(ix, k.key)) {
      skip.push(k.path);
    } else {
      known.push([k.path, sizes.get(k.key) ?? 0]);
    }
  }
  return { size: Math.max(0, size), skip, known };
}

/** 目录树里一个位置下还要显示的子位置：没放进箱子、扣完后仍不小于下限 */
export function visibleKids(ix: ArchiveIndex, n: FolderNode, sizes: Map<string, number>): FolderNode[] {
  return n.kids
    .filter((k) => !placementOf(ix, k.key) && (sizes.get(k.key) ?? 0) >= MIN_FOLDER)
    .sort((a, b) => (sizes.get(b.key) ?? 0) - (sizes.get(a.key) ?? 0));
}

// ---------------------------------------------------------------------------
// 状态更新（返回新对象）
// ---------------------------------------------------------------------------

export function withUserPlacement(m: ArchiveModel, keys: string[], box: string | null): ArchiveModel {
  const set = new Set(keys);
  return { ...m, user: [...m.user.filter((u) => !set.has(u.key)), ...keys.map((key) => ({ key, box_id: box }))] };
}

export function withAiPlacements(m: ArchiveModel, rows: ArchiveAiPlacement[]): ArchiveModel {
  const set = new Set(rows.map((r) => r.key));
  return { ...m, ai: [...m.ai.filter((a) => !set.has(a.key)), ...rows] };
}

export function withChecks(m: ArchiveModel, rows: ArchiveCacheCheck[]): ArchiveModel {
  const set = new Set(rows.map((r) => r.path.toLowerCase()));
  return { ...m, checks: [...(m.checks ?? []).filter((c) => !set.has(c.path.toLowerCase())), ...rows] };
}
