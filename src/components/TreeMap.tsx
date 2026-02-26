import { useEffect, useRef, useCallback, useMemo, useState } from "react";
import type { AiAnalysis, DirEntry } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import { StarRating } from "./StarRating";
import { ChevronRight, Loader2, Sparkles } from "lucide-react";
import gsap from "gsap";
import { cn } from "../lib/utils";

interface Props {
  rootPath: string | null;
  liveChildren: DirEntry[];
  scanning: boolean;
  analyses?: Map<string, AiAnalysis>;
  onAnalyzePath?: (currentPath: string) => void;
  analyzing?: boolean;
  navPath: string[];
  onNavigate: (path: string) => void;
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

export function TreeMap({ liveChildren, scanning, analyses, onAnalyzePath, analyzing, navPath, onNavigate }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const bgRef = useRef<HTMLDivElement>(null);
  const [rects, setRects] = useState<Rect[]>([]);
  const [hover, setHover] = useState<Rect | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  const currentPath = navPath[navPath.length - 1];

  // GSAP animation on mount
  useEffect(() => {
    if (bgRef.current) {
      gsap.fromTo(bgRef.current,
        { opacity: 0, scale: 0.98 },
        { opacity: 1, scale: 1, duration: 0.6, ease: "power2.out" }
      );
    }
  }, [currentPath]);

  const liveFiltered = useMemo(() => {
    return liveChildren.filter((c) => c.logical_size > 0)
      .sort((a, b) => b.logical_size - a.logical_size);
  }, [liveChildren]);

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

      // Midnight Luxury block styling
      // Directories: deep obsidian -> slate -> champagne hover
      // Files: champagne -> muted -> white hover
      ctx.fillStyle = r.entry.is_dir
        ? (isHover ? "#C9A84C" : "#2A2A35")
        : (isHover ? "#FAF8F5" : "#EAE6DF");

      // Inner shadow/border effect simulating gap
      ctx.fillRect(r.x + 1, r.y + 1, r.w - 2, r.h - 2);

      // Text styling
      const textColor = r.entry.is_dir
        ? (isHover ? "#0D0D12" : "#FAF8F5")
        : (isHover ? "#0D0D12" : "#0D0D12");

      if (r.w > 40 && r.h > 20) {
        ctx.fillStyle = textColor;
        ctx.font = "500 12px Inter, sans-serif";
        const label = r.entry.name.length > r.w / 7
          ? r.entry.name.slice(0, Math.floor(r.w / 7)) + "\u2026"
          : r.entry.name;
        // Vertically center text if it's a short block
        ctx.fillText(label, r.x + 8, r.y + 18);
      }
      if (r.w > 60 && r.h > 40) {
        ctx.fillStyle = r.entry.is_dir
          ? (isHover ? "rgba(13,13,18,0.7)" : "rgba(250,248,245,0.5)")
          : (isHover ? "rgba(13,13,18,0.7)" : "rgba(13,13,18,0.7)");
        ctx.font = "11px 'JetBrains Mono', monospace";
        ctx.fillText(formatSize(r.entry.logical_size), r.x + 8, r.y + 34);
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
    if (r?.entry.is_dir) onNavigate(r.entry.path);
  };

  const goBack = (idx: number) => {
    if (!scanning) onNavigate(navPath[idx]);
  };

  return (
    <div className="flex flex-col h-full bg-[#0D0D12] overflow-hidden" ref={bgRef}>
      {!scanning && navPath.length > 0 && (
        <div className="flex items-center gap-2 px-6 py-3 border-b border-[#2A2A35] shrink-0 font-mono text-xs overflow-x-auto custom-scrollbar">
          {navPath.map((p, i) => (
            <div key={p} className="flex items-center gap-2 shrink-0">
              {i > 0 && <ChevronRight className="w-3 h-3 text-[#555]" />}
              <span
                className={cn(
                  "cursor-pointer transition-colors px-2 py-1 rounded hover:bg-[#2A2A35]",
                  i === navPath.length - 1 ? "text-[#C9A84C] bg-[#C9A84C]/10" : "text-[#888899]"
                )}
                onClick={() => goBack(i)}
              >
                {p.split("\\").pop() || p}
              </span>
            </div>
          ))}
          <div className="flex-1 min-w-[20px]" />
          {onAnalyzePath && currentPath && (
            <button
              className="magnetic-btn shrink-0 flex items-center gap-2 px-4 py-1.5 rounded-full bg-[#13131A] border border-[#C9A84C] text-[#C9A84C] hover:bg-[#C9A84C] hover:text-[#0D0D12] transition-colors"
              disabled={analyzing}
              onClick={() => onAnalyzePath(currentPath)}
            >
              {analyzing ? <Loader2 className="w-4 h-4 animate-spin" /> : <Sparkles className="w-4 h-4" />}
              {analyzing ? "ANALYZING..." : "AI STRUCTURAL ANALYSIS"}
            </button>
          )}
        </div>
      )}

      <div className="flex-1 min-h-0 relative p-0 border border-[#2A2A35] rounded-b-2xl" ref={wrapperRef}>
        <div className="absolute inset-0 overflow-hidden bg-[#13131A]">
          <canvas
            ref={canvasRef}
            style={{ width: size.w, height: size.h, cursor: scanning ? "default" : "crosshair", display: "block" }}
            onMouseMove={(e) => setHover(findRect(e))}
            onMouseLeave={() => setHover(null)}
            onClick={handleClick}
          />
        </div>
      </div>

      {hover && (() => {
        const ai = analyses?.get(hover.entry.path);
        return (
          <div className="fixed bottom-12 left-8 bg-[#13131A]/90 backdrop-blur-xl border border-[#2A2A35] rounded-xl p-4 shadow-2xl pointer-events-none z-50 animate-in fade-in slide-in-from-bottom-4 duration-200">
            <div className="flex items-start justify-between gap-6 mb-2">
              <span className="font-semibold text-[#FAF8F5] max-w-xs break-all">{hover.entry.name}</span>
              <span className="font-mono text-[#C9A84C]">{formatSize(hover.entry.logical_size)}</span>
            </div>
            {hover.entry.is_dir && (
              <div className="font-mono text-xs text-[#888899] mb-3 flex gap-4">
                <span>{formatNumber(hover.entry.subdirs)} DIRS</span>
                <span>{formatNumber(hover.entry.files)} FILES</span>
              </div>
            )}
            {ai && (
              <div className="pt-3 border-t border-[#2A2A35] flex items-center gap-3">
                <StarRating priority={ai.priority} />
                <span className="text-xs text-[#EAE6DF] max-w-xs">{ai.description}</span>
              </div>
            )}
          </div>
        );
      })()}
    </div>
  );
}
