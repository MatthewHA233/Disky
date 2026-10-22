import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import {
  computeGeometry, dropZoneAt, moveTile, resizePair, withCellWeights, withColumnWeights,
  type DropZone, type GutterRect, type TileLayout,
} from "../../lib/tileLayout";
import { cn } from "../../lib/utils";

/** 交给格子标题栏的拖动把手 */
export interface TileHandle {
  onPointerDown: (e: ReactPointerEvent) => void;
}

interface Props<T extends string> {
  layout: TileLayout<T>;
  onChange: (layout: TileLayout<T>) => void;
  renderTile: (id: T, handle: TileHandle, dragging: boolean) => ReactNode;
  titleOf: (id: T) => string;
}

const GAP = 8;
const MIN_COL_PX = 240;
const MIN_CELL_PX = 140;
/** 指针移动超过这个距离才算开始拖格子，避免和单击冲突 */
const DRAG_THRESHOLD = 5;
/** 按在这些元素上不触发格子拖动 */
const NO_DRAG = "button, input, a, [role=checkbox], [role=tab]";

const ZONE_BOX: Record<DropZone, string> = {
  center: "inset-1",
  top: "inset-x-1 top-1 h-[calc(50%-4px)]",
  bottom: "inset-x-1 bottom-1 h-[calc(50%-4px)]",
  left: "inset-y-1 left-1 w-[calc(50%-4px)]",
  right: "inset-y-1 right-1 w-[calc(50%-4px)]",
};

interface DragTarget<T> {
  id: T;
  target: T | null;
  zone: DropZone | null;
}

/** 拖动期间全局统一光标（压过子元素自己的 cursor）并禁止选中文字，返回恢复函数 */
function lockPointer(cursor: string): () => void {
  const { body } = document;
  body.style.setProperty("--drag-cursor", cursor);
  body.classList.add("pointer-locked");
  return () => {
    body.classList.remove("pointer-locked");
    body.style.removeProperty("--drag-cursor");
  };
}

/**
 * 铺满区域的格子：拖分隔条改大小，拖标题栏换位置或停靠到另一个格子的某一边。
 * 所有格子按固定顺序挂在同一个父节点下、绝对定位，移动格子只改坐标，
 * 不会重建组件（页签、展开状态、滚动位置都保留）。
 */
export function TileGrid<T extends string>({ layout, onChange, renderTile, titleOf }: Props<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const pointerRef = useRef({ x: -9999, y: -9999 });
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [drag, setDrag] = useState<DragTarget<T> | null>(null);
  const stopRef = useRef<(() => void) | null>(null);

  const geo = useMemo(() => computeGeometry(layout, size.w, size.h, GAP), [layout, size]);
  const latest = useRef({ layout, geo });
  latest.current = { layout, geo };
  // 渲染顺序固定（按 id），布局变化时 DOM 节点不移动
  const ordered = useMemo(() => [...geo.tiles].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)), [geo]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => () => stopRef.current?.(), []);

  /**
   * 跟踪一次拖动直到松开。onEnd 收到 null 表示取消（Esc、指针被系统收回、窗口失焦、
   * 或者回来时按键早已松开）。同一时间只跟踪一次，新的拖动会先结束旧的。
   */
  const track = useCallback((cursor: string, onMove: (e: PointerEvent) => void, onEnd: (e: PointerEvent | null) => void) => {
    stopRef.current?.();
    const unlock = lockPointer(cursor);
    const finish = (e: PointerEvent | null) => { stop(); onEnd(e); };
    const move = (e: PointerEvent) => { if (e.buttons === 0) finish(null); else onMove(e); };
    const up = (e: PointerEvent) => finish(e);
    const cancel = () => finish(null);
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") finish(null); };
    function stop() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", key);
      unlock();
      if (stopRef.current === stop) stopRef.current = null;
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", key);
    stopRef.current = stop;
  }, []);

  const startResize = (g: GutterRect) => (e: ReactPointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const { layout: start, geo: startGeo } = latest.current;
    const x = g.axis === "x";
    const px: [number, number] = x
      ? [startGeo.colWidths[g.col], startGeo.colWidths[g.col + 1]]
      : [startGeo.cellHeights[g.col][g.index], startGeo.cellHeights[g.col][g.index + 1]];
    const weights = x ? start.map((c) => c.w) : start[g.col].cells.map((c) => c.h);
    const origin = x ? e.clientX : e.clientY;

    track(x ? "col-resize" : "row-resize", (ev) => {
      const next = resizePair(weights, x ? g.col : g.index, px, (x ? ev.clientX : ev.clientY) - origin, x ? MIN_COL_PX : MIN_CELL_PX);
      onChange(x ? withColumnWeights(start, next) : withCellWeights(start, g.col, next));
    }, (ev) => {
      if (!ev) onChange(start);
    });
  };

  const hitTest = (clientX: number, clientY: number, self: T): { id: T; zone: DropZone } | null => {
    const box = containerRef.current?.getBoundingClientRect();
    if (!box) return null;
    const x = clientX - box.left, y = clientY - box.top;
    const t = latest.current.geo.tiles.find((r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h);
    if (!t || t.id === self) return null;
    return { id: t.id, zone: dropZoneAt({ left: t.x, top: t.y, width: t.w, height: t.h }, x, y) };
  };

  const startDrag = (id: T) => (e: ReactPointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(NO_DRAG)) return;
    const sx = e.clientX, sy = e.clientY;
    let active = false;
    let last: DragTarget<T> | null = null;

    track("grabbing", (ev) => {
      if (!active && Math.hypot(ev.clientX - sx, ev.clientY - sy) < DRAG_THRESHOLD) return;
      active = true;
      // 跟手的标签直接改 DOM，只有落点变化时才重新渲染
      pointerRef.current = { x: ev.clientX + 12, y: ev.clientY + 12 };
      if (ghostRef.current) ghostRef.current.style.transform = `translate(${pointerRef.current.x}px, ${pointerRef.current.y}px)`;
      const hit = hitTest(ev.clientX, ev.clientY, id);
      const next: DragTarget<T> = { id, target: hit?.id ?? null, zone: hit?.zone ?? null };
      if (!last || last.target !== next.target || last.zone !== next.zone) {
        last = next;
        setDrag(next);
      }
    }, (ev) => {
      setDrag(null);
      if (!active || !ev) return;
      const hit = hitTest(ev.clientX, ev.clientY, id);
      if (hit) onChange(moveTile(latest.current.layout, id, hit.id, hit.zone));
    });
  };

  return (
    <div ref={containerRef} className="relative flex-1 min-h-0">
      {size.w > 0 && ordered.map((t) => (
        <div key={t.id} className="absolute flex" style={{ left: t.x, top: t.y, width: t.w, height: t.h }}>
          {renderTile(t.id, { onPointerDown: startDrag(t.id) }, drag?.id === t.id)}
          {drag?.target === t.id && drag.zone && (
            <div className={cn("absolute z-20 rounded-lg bg-photon/15 border-2 border-photon pointer-events-none", ZONE_BOX[drag.zone])} />
          )}
        </div>
      ))}

      {size.w > 0 && geo.gutters.map((g) => (
        <div
          key={g.axis === "x" ? `x${g.col}` : `y${g.col}-${g.index}`}
          role="separator"
          aria-orientation={g.axis === "x" ? "vertical" : "horizontal"}
          className={cn("group absolute z-10", g.axis === "x" ? "cursor-col-resize" : "cursor-row-resize")}
          style={{ left: g.x, top: g.y, width: g.w, height: g.h }}
          onPointerDown={startResize(g)}
        >
          <span
            className={cn(
              "absolute rounded-full bg-photon opacity-0 transition-opacity group-hover:opacity-100 group-active:opacity-100",
              g.axis === "x" ? "inset-y-3 left-[3px] w-0.5" : "inset-x-3 top-[3px] h-0.5"
            )}
          />
        </div>
      ))}

      {drag && (
        <div
          ref={ghostRef}
          className="fixed left-0 top-0 z-[130] px-3 h-8 flex items-center rounded bg-card border border-slate text-[13px] whitespace-nowrap pointer-events-none"
          style={{ boxShadow: "var(--fl-shadow)", transform: `translate(${pointerRef.current.x}px, ${pointerRef.current.y}px)` }}
        >
          {titleOf(drag.id)}
        </div>
      )}
    </div>
  );
}
