// 平铺格子布局：若干列，每列若干格子。列宽、格高都是相对权重（flex-grow），
// 所有操作都返回新对象，不改动传入的布局。

export interface TileCell<T extends string> {
  readonly id: T;
  readonly h: number;
}

export interface TileColumn<T extends string> {
  readonly w: number;
  readonly cells: readonly TileCell<T>[];
}

export type TileLayout<T extends string> = readonly TileColumn<T>[];

/** 拖到目标格子的哪个位置：中间互换，四边停靠 */
export type DropZone = "center" | "top" | "bottom" | "left" | "right";

const MAX_COLUMNS = 4;
const MAX_CELLS_PER_COLUMN = 4;
/** 离边缘多近（占格子宽/高的比例）算停靠到那一边 */
const EDGE_RATIO = 0.25;

const normalize = (ws: readonly number[]): number[] => {
  const sum = ws.reduce((s, w) => s + w, 0);
  return sum > 0 ? ws.map((w) => w / sum) : ws.map(() => 1 / ws.length);
};

function tidy<T extends string>(layout: TileLayout<T>): TileLayout<T> {
  const cols = layout.filter((c) => c.cells.length > 0);
  const ws = normalize(cols.map((c) => c.w));
  return cols.map((c, i) => {
    const hs = normalize(c.cells.map((x) => x.h));
    return { w: ws[i], cells: c.cells.map((x, j) => ({ id: x.id, h: hs[j] })) };
  });
}

/**
 * 拖动两个相邻项之间的分隔条。startPx 是拖动开始时两项的像素尺寸，
 * 两项权重之和保持不变，其他项不受影响。
 */
export function resizePair(weights: readonly number[], i: number, startPx: readonly [number, number], delta: number, minPx: number): number[] {
  const [a, b] = startPx;
  const total = a + b;
  if (total <= 0 || i < 0 || i + 1 >= weights.length) return [...weights];
  const min = Math.min(minPx, total / 2);
  const na = Math.min(Math.max(a + delta, min), total - min);
  const sum = weights[i] + weights[i + 1];
  return weights.map((w, k) => (k === i ? (sum * na) / total : k === i + 1 ? (sum * (total - na)) / total : w));
}

export function withColumnWeights<T extends string>(layout: TileLayout<T>, ws: readonly number[]): TileLayout<T> {
  return layout.map((c, i) => ({ ...c, w: ws[i] ?? c.w }));
}

export function withCellWeights<T extends string>(layout: TileLayout<T>, col: number, hs: readonly number[]): TileLayout<T> {
  return layout.map((c, i) => (i === col ? { ...c, cells: c.cells.map((x, j) => ({ ...x, h: hs[j] ?? x.h })) } : c));
}

function locate<T extends string>(layout: TileLayout<T>, id: T): [number, number] | null {
  for (let i = 0; i < layout.length; i++) {
    const j = layout[i].cells.findIndex((x) => x.id === id);
    if (j >= 0) return [i, j];
  }
  return null;
}

export function swapTiles<T extends string>(layout: TileLayout<T>, a: T, b: T): TileLayout<T> {
  const swap = (id: T): T => (id === a ? b : id === b ? a : id);
  return layout.map((c) => ({ ...c, cells: c.cells.map((x) => ({ ...x, id: swap(x.id) })) }));
}

/** 把格子 id 拖到 target 的某个位置；放不下（列或格子太多）时退化为互换 */
export function moveTile<T extends string>(layout: TileLayout<T>, id: T, target: T, zone: DropZone): TileLayout<T> {
  if (id === target || !locate(layout, id) || !locate(layout, target)) return layout;
  if (zone === "center") return swapTiles(layout, id, target);

  const removed = tidy(layout.map((c) => ({ ...c, cells: c.cells.filter((x) => x.id !== id) })));
  const [ti, tj] = locate(removed, target)!;
  const col = removed[ti];

  if (zone === "top" || zone === "bottom") {
    if (col.cells.length >= MAX_CELLS_PER_COLUMN) return swapTiles(layout, id, target);
    const half = col.cells[tj].h / 2;
    const at = zone === "top" ? tj : tj + 1;
    const cells = col.cells.map((x, j) => (j === tj ? { ...x, h: half } : x));
    const next = [...cells.slice(0, at), { id, h: half }, ...cells.slice(at)];
    return tidy(removed.map((c, i) => (i === ti ? { ...c, cells: next } : c)));
  }

  if (removed.length >= MAX_COLUMNS) return swapTiles(layout, id, target);
  const half = col.w / 2;
  const at = zone === "left" ? ti : ti + 1;
  const cols = removed.map((c, i) => (i === ti ? { ...c, w: half } : c));
  return tidy([...cols.slice(0, at), { w: half, cells: [{ id, h: 1 }] }, ...cols.slice(at)]);
}

export interface TileRect<T extends string> {
  id: T;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 分隔条：axis "x" 在列之间（拖动改列宽），"y" 在同列格子之间（拖动改格高） */
export interface GutterRect {
  axis: "x" | "y";
  /** x：左侧列的序号；y：所在列的序号 */
  col: number;
  /** y：上方格子在列里的序号 */
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface TileGeometry<T extends string> {
  tiles: TileRect<T>[];
  gutters: GutterRect[];
  /** 每列宽度、每列各格高度（不含分隔条），拖动分隔条时作为起始尺寸 */
  colWidths: number[];
  cellHeights: number[][];
}

/** 按权重把 width × height 的区域切成格子，相邻格子之间留 gap 宽的分隔条；边缘取整避免发虚 */
export function computeGeometry<T extends string>(layout: TileLayout<T>, width: number, height: number, gap: number): TileGeometry<T> {
  const tiles: TileRect<T>[] = [];
  const gutters: GutterRect[] = [];
  const colWidths: number[] = [];
  const cellHeights: number[][] = [];
  const ws = normalize(layout.map((c) => c.w));
  const usableW = Math.max(0, width - gap * (layout.length - 1));
  let acc = 0;
  layout.forEach((col, ci) => {
    const left = Math.round(acc + ci * gap);
    acc += usableW * ws[ci];
    const right = Math.round(acc + ci * gap);
    const w = right - left;
    colWidths.push(w);
    if (ci > 0) gutters.push({ axis: "x", col: ci - 1, index: 0, x: left - gap, y: 0, w: gap, h: height });

    const hs = normalize(col.cells.map((c) => c.h));
    const usableH = Math.max(0, height - gap * (col.cells.length - 1));
    const heights: number[] = [];
    let accY = 0;
    col.cells.forEach((cell, j) => {
      const top = Math.round(accY + j * gap);
      accY += usableH * hs[j];
      const bottom = Math.round(accY + j * gap);
      heights.push(bottom - top);
      if (j > 0) gutters.push({ axis: "y", col: ci, index: j - 1, x: left, y: top - gap, w, h: gap });
      tiles.push({ id: cell.id, x: left, y: top, w, h: bottom - top });
    });
    cellHeights.push(heights);
  });
  return { tiles, gutters, colWidths, cellHeights };
}

/** 指针落在格子矩形里的哪个区域 */
export function dropZoneAt(rect: { left: number; top: number; width: number; height: number }, x: number, y: number): DropZone {
  const fx = (x - rect.left) / rect.width;
  const fy = (y - rect.top) / rect.height;
  const edges: [DropZone, number][] = [["left", fx], ["right", 1 - fx], ["top", fy], ["bottom", 1 - fy]];
  const [zone, dist] = edges.reduce((best, e) => (e[1] < best[1] ? e : best));
  return dist < EDGE_RATIO ? zone : "center";
}

/** 最多能摆下的格子数 */
export const MAX_TILES = MAX_COLUMNS * MAX_CELLS_PER_COLUMN;

const mean = (ws: readonly number[]) => (ws.length ? ws.reduce((s, w) => s + w, 0) / ws.length : 1);

/**
 * 让布局和现有的格子对上：去掉已经没有的，新出现的放进格子最少的一列，那一列也满了就新开一列。
 * 摆不下（超过 MAX_TILES）的不显示。
 */
export function reconcileLayout<T extends string>(layout: TileLayout<T>, ids: readonly T[]): TileLayout<T> {
  const want = new Set<string>(ids);
  let cols: TileColumn<T>[] = layout
    .map((c) => ({ ...c, cells: c.cells.filter((x) => want.has(x.id)) }))
    .filter((c) => c.cells.length > 0);
  const present = new Set<string>(cols.flatMap((c) => c.cells.map((x) => x.id)));
  for (const id of ids) {
    if (present.has(id)) continue;
    present.add(id);
    const fewest = cols.reduce((best, c, i) => (best < 0 || c.cells.length < cols[best].cells.length ? i : best), -1);
    if (fewest >= 0 && cols[fewest].cells.length < MAX_CELLS_PER_COLUMN) {
      cols = cols.map((c, i) => (i === fewest ? { ...c, cells: [...c.cells, { id, h: mean(c.cells.map((x) => x.h)) }] } : c));
    } else if (cols.length < MAX_COLUMNS) {
      cols = [...cols, { w: mean(cols.map((c) => c.w)), cells: [{ id, h: 1 }] }];
    }
  }
  return tidy(cols);
}

/** 校验从存储里读出的布局：每个 id 恰好出现一次、权重为正数，否则返回 null */
export function parseLayout<T extends string>(raw: unknown, ids: readonly T[]): TileLayout<T> | null {
  if (!Array.isArray(raw) || raw.length === 0 || raw.length > MAX_COLUMNS) return null;
  const seen = new Set<string>();
  const valid = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n) && n > 0;
  for (const col of raw) {
    if (!col || !valid(col.w) || !Array.isArray(col.cells) || col.cells.length === 0 || col.cells.length > MAX_CELLS_PER_COLUMN) return null;
    for (const cell of col.cells) {
      if (!cell || !valid(cell.h) || !ids.includes(cell.id) || seen.has(cell.id)) return null;
      seen.add(cell.id);
    }
  }
  if (seen.size !== ids.length) return null;
  return tidy(raw as TileLayout<T>);
}

export function sameLayout<T extends string>(a: TileLayout<T>, b: TileLayout<T>): boolean {
  const ta = tidy(a), tb = tidy(b);
  const close = (x: number, y: number) => Math.abs(x - y) < 0.005;
  return ta.length === tb.length && ta.every((c, i) =>
    close(c.w, tb[i].w) && c.cells.length === tb[i].cells.length &&
    c.cells.every((x, j) => x.id === tb[i].cells[j].id && close(x.h, tb[i].cells[j].h)));
}
