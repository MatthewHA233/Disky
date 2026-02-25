import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import type { DirEntry } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize } from "../lib/format";

interface Props {
  rootPath: string | null;
  liveChildren: DirEntry[];
  scanning: boolean;
}

interface Rect {
  x: number; y: number; w: number; h: number;
  entry: DirEntry;
}

function squarify(items: DirEntry[], x: number, y: number, w: number, h: number): Rect[] {
  const total = items.reduce((s, i) => s + i.logical_size, 0);
  if (total === 0 || items.length === 0 || w <= 0 || h <= 0) return [];

  const rects: Rect[] = [];
  let cx = x, cy = y, cw = w, ch = h;
  let remaining = [...items];
  let remTotal = total;

  while (remaining.length > 0) {
    const isWide = cw >= ch;
    const side = isWide ? ch : cw;
    let row: DirEntry[] = [];
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
      const frac = (item.logical_size / remTotal) * cw * ch / rowArea;
      const len = frac * side;
      if (isWide) {
        rects.push({ x: cx, y: cy + offset, w: rowSide, h: len, entry: item });
      } else {
        rects.push({ x: cx + offset, y: cy, w: len, h: rowSide, entry: item });
      }
      offset += len;
    }

    if (isWide) { cx += rowSide; cw -= rowSide; }
    else { cy += rowSide; ch -= rowSide; }

    remaining = remaining.slice(row.length);
    remTotal -= row.reduce((s, i) => s + i.logical_size, 0);
    if (remTotal <= 0) break;
  }
  return rects;
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

export function TreeMap({ rootPath, liveChildren, scanning }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const [path, setPath] = useState<string[]>(rootPath ? [rootPath] : []);
  const [rects, setRects] = useState<Rect[]>([]);
  const [hover, setHover] = useState<Rect | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  // Reset path on rootPath change
  useEffect(() => {
    if (rootPath) setPath([rootPath]);
    else setPath([]);
  }, [rootPath]);

  const currentPath = path[path.length - 1];

  // Filter live children with size > 0 for treemap
  const liveFiltered = useMemo(() => {
    return liveChildren.filter((c) => c.logical_size > 0)
      .sort((a, b) => b.logical_size - a.logical_size);
  }, [liveChildren]);

  // Track container size via ResizeObserver
  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        const { width, height } = entry.contentRect;
        if (width > 0 && height > 0) {
          setSize({ w: Math.floor(width), h: Math.floor(height) });
        }
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Load treemap data whenever size, scanning state, or path changes
  const load = useCallback(async () => {
    if (size.w <= 0 || size.h <= 0) return;
    if (scanning) {
      setRects(squarify(liveFiltered, 0, 0, size.w, size.h));
      return;
    }
    if (!currentPath) {
      setRects([]);
      return;
    }
    const items = await getChildren(currentPath, 200);
    setRects(squarify(items, 0, 0, size.w, size.h));
  }, [currentPath, scanning, liveFiltered, size]);

  useEffect(() => { load(); }, [load]);

  // Canvas drawing
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w <= 0 || size.h <= 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, size.w, size.h);

    for (const r of rects) {
      const isHover = hover?.entry.path === r.entry.path;
      ctx.fillStyle = r.entry.is_dir
        ? (isHover ? "#5aafff" : "#3d7ac7")
        : (isHover ? "#f0a850" : "#e8943a");
      ctx.fillRect(r.x + 1, r.y + 1, r.w - 2, r.h - 2);

      if (r.w > 40 && r.h > 16) {
        ctx.fillStyle = "#fff";
        ctx.font = "12px Segoe UI, sans-serif";
        const label = r.entry.name.length > r.w / 7
          ? r.entry.name.slice(0, Math.floor(r.w / 7)) + "\u2026"
          : r.entry.name;
        ctx.fillText(label, r.x + 4, r.y + 14);
      }
      if (r.w > 50 && r.h > 30) {
        ctx.fillStyle = "rgba(255,255,255,0.6)";
        ctx.font = "10px Segoe UI, sans-serif";
        ctx.fillText(formatSize(r.entry.logical_size), r.x + 4, r.y + 26);
      }
    }
  }, [rects, hover, size]);

  const findRect = (e: React.MouseEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    return rects.find((r) => mx >= r.x && mx <= r.x + r.w && my >= r.y && my <= r.y + r.h) ?? null;
  };

  const handleClick = (e: React.MouseEvent) => {
    if (scanning) return;
    const r = findRect(e);
    if (r?.entry.is_dir) setPath([...path, r.entry.path]);
  };

  const goBack = (idx: number) => {
    if (!scanning) setPath(path.slice(0, idx + 1));
  };

  return (
    <div className="treemap-container">
      {!scanning && path.length > 0 && (
        <div className="breadcrumb">
          {path.map((p, i) => (
            <span key={p}>
              {i > 0 && <span className="bc-sep"> &rsaquo; </span>}
              <span className={`bc-item ${i === path.length - 1 ? "active" : ""}`} onClick={() => goBack(i)}>
                {p.split("\\").pop() || p}
              </span>
            </span>
          ))}
        </div>
      )}
      <div className="treemap-canvas-wrapper" ref={wrapperRef}>
        <canvas
          ref={canvasRef}
          style={{ width: size.w, height: size.h, cursor: scanning ? "default" : "pointer" }}
          onMouseMove={(e) => setHover(findRect(e))}
          onMouseLeave={() => setHover(null)}
          onClick={handleClick}
        />
      </div>
      {hover && (
        <div className="treemap-tooltip">
          {hover.entry.name} — {formatSize(hover.entry.logical_size)}
          {hover.entry.is_dir && ` (${hover.entry.subdirs} 个目录, ${hover.entry.files} 个文件)`}
        </div>
      )}
    </div>
  );
}
