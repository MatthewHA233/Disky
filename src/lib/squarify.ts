// Squarified treemap 布局（Bruls 等人的经典算法）：把按大小排好的条目铺进矩形，尽量接近正方形。

export interface Sized {
  logical_size: number;
}

export interface Rect<T extends Sized> {
  x: number;
  y: number;
  w: number;
  h: number;
  entry: T;
}

function worstRatio(areas: number[], totalArea: number, side: number): number {
  const s2 = side * side;
  let worst = 0;
  for (const a of areas) {
    const r = Math.max((s2 * a) / (totalArea * totalArea), (totalArea * totalArea) / (s2 * a));
    if (r > worst) worst = r;
  }
  return worst;
}

/** items 需按 logical_size 降序；大小为 0 的条目不参与布局 */
export function squarify<T extends Sized>(items: T[], x: number, y: number, w: number, h: number): Rect<T>[] {
  const list = items.filter((i) => i.logical_size > 0);
  const total = list.reduce((s, i) => s + i.logical_size, 0);
  if (total === 0 || list.length === 0 || w <= 0 || h <= 0) return [];

  const rects: Rect<T>[] = [];
  let cx = x, cy = y, cw = w, ch = h;
  let remaining = list;
  let remTotal = total;

  while (remaining.length > 0) {
    const isWide = cw >= ch;
    const side = isWide ? ch : cw;
    let row: T[] = [];
    let rowArea = 0;
    let worst = Infinity;

    for (const item of remaining) {
      const area = (item.logical_size / remTotal) * cw * ch;
      const testRow = [...row, item];
      const testArea = rowArea + area;
      const testWorst = worstRatio(testRow.map((r) => (r.logical_size / remTotal) * cw * ch), testArea, side);
      if (testWorst <= worst || row.length === 0) {
        row = testRow;
        rowArea = testArea;
        worst = testWorst;
      } else {
        break;
      }
    }

    const rowSide = rowArea / side;
    let offset = 0;
    for (const item of row) {
      const len = ((item.logical_size / remTotal) * cw * ch / rowArea) * side;
      rects.push(isWide
        ? { x: cx, y: cy + offset, w: rowSide, h: len, entry: item }
        : { x: cx + offset, y: cy, w: len, h: rowSide, entry: item });
      offset += len;
    }

    if (isWide) { cx += rowSide; cw -= rowSide; } else { cy += rowSide; ch -= rowSide; }
    remaining = remaining.slice(row.length);
    remTotal -= row.reduce((s, i) => s + i.logical_size, 0);
    if (remTotal <= 0) break;
  }
  return rects;
}
