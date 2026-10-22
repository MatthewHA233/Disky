import { useEffect, useRef, useCallback, useMemo, useState } from "react";
import type { AiAnalysis, DirEntry, Footprint } from "../types";
import { getChildren } from "../lib/invoke";
import { formatSize, formatNumber } from "../lib/format";
import { basename } from "../lib/path";
import { KINDS, KIND_ORDER, makeKindLens } from "../lib/archive";
import { squarify, type Rect } from "../lib/squarify";
import { StarRating } from "./StarRating";
import { ChevronRight, Loader2 } from "lucide-react";
import { AiIcon } from "./ui/AiIcon";
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
  onContextMenu?: (entry: DirEntry, x: number, y: number) => void;
  /** 档案馆的归类结果；有就按「性质」着色，没有就用中性色 */
  footprints?: Footprint[] | null;
}

/** depth 0 = 当前目录的子项；depth 1 = 大目录里再画一层的孙项。rest = 未画出的剩余部分（不可交互） */
interface Block extends Rect<DirEntry> {
  depth: 0 | 1;
  /** depth 1：从当前目录的子项到孙项所在目录的路径链（中间可能穿过只有一个大子目录的层） */
  chain?: string[];
  rest?: boolean;
}

// 目录块够大才往里画一层，标题条高度 HEADER_H
const NEST_MIN_W = 150;
const NEST_MIN_H = 96;
const HEADER_H = 22;
const NEST_LIMIT = 60;
const MAX_NESTED = 14;
const PAD = 3;
// 子目录几乎占满父目录时直接穿过去，最多穿 MAX_HOPS 层（Users › Administrator 这种）
const PASS_THROUGH = 0.9;
const MAX_HOPS = 3;

interface Theme { accent: string; layer: string; text: string; text2: string; faint: string }

function readTheme(): Theme {
  const cs = getComputedStyle(document.documentElement);
  const v = (name: string) => cs.getPropertyValue(name).trim();
  return { accent: v("--fl-accent"), layer: v("--fl-layer"), text: v("--fl-text"), text2: v("--fl-text-2"), faint: v("--fl-text-3") };
}

function hexRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h.slice(0, 6);
  const n = parseInt(full, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** 按比例 t 把 a 混进 b（canvas 不支持 color-mix，手动算） */
function mix(a: string, b: string, t: number): string {
  const A = hexRgb(a), B = hexRgb(b);
  return `rgb(${A.map((x, i) => Math.round(x * t + B[i] * (1 - t))).join(",")})`;
}

/** 浅色背景上淡色块不明显，着色时要加深 */
function isLight(hex: string): boolean {
  const [r, g, b] = hexRgb(hex);
  return 0.299 * r + 0.587 * g + 0.114 * b > 128;
}

/** 在目录块 r 里再画一层：穿过「几乎占满」的单个子目录，取前 NEST_LIMIT 项，剩余补一块「其他」 */
async function loadNested(r: Rect<DirEntry>): Promise<Block[]> {
  const chain = [r.entry];
  let kids = await getChildren(r.entry.path, NEST_LIMIT);
  while (chain.length <= MAX_HOPS && kids[0]?.is_dir && kids[0].logical_size >= chain[chain.length - 1].logical_size * PASS_THROUGH) {
    chain.push(kids[0]);
    kids = await getChildren(kids[0].path, NEST_LIMIT);
  }
  const chainPaths = chain.map((c) => c.path);
  const shown = kids.reduce((s, k) => s + k.logical_size, 0);
  // 只取了前 NEST_LIMIT 项，剩余部分用一块「其他」补齐，保证面积比例真实
  const rest = r.entry.logical_size - shown;
  const withRest = rest > r.entry.logical_size * 0.002
    ? [...kids, { name: "其他", path: `${r.entry.path}\u0000rest`, files: 0, subdirs: 0, logical_size: rest, is_dir: false }]
    : kids;
  return squarify(withRest, r.x + PAD, r.y + HEADER_H, r.w - PAD * 2, r.h - HEADER_H - PAD)
    .map((k): Block => ({ ...k, depth: 1, chain: chainPaths, rest: k.entry.path.endsWith("\u0000rest") }));
}

function fitText(ctx: CanvasRenderingContext2D, text: string, maxW: number): string {
  if (ctx.measureText(text).width <= maxW) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(`${s}…`).width > maxW) s = s.slice(0, -1);
  return `${s}…`;
}

export function TreeMap({ scanning, liveChildren, analyses, onAnalyzePath, analyzing, navPath, onNavigate, onContextMenu, footprints }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const requestRef = useRef(0);
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [hover, setHover] = useState<Block | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [themeTick, setThemeTick] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);

  const currentPath = navPath[navPath.length - 1];
  const lens = useMemo(() => (footprints && !scanning ? makeKindLens(footprints) : null), [footprints, scanning]);

  const liveFiltered = useMemo(
    () => liveChildren.filter((c) => c.logical_size > 0).sort((a, b) => b.logical_size - a.logical_size),
    [liveChildren]
  );

  // 系统切换浅色 / 深色时重绘
  useEffect(() => {
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setThemeTick((t) => t + 1);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const load = useCallback(async () => {
    if (size.w <= 0 || size.h <= 0) return;
    const req = ++requestRef.current;
    if (scanning) {
      setBlocks(squarify(liveFiltered, 0, 0, size.w, size.h).map((r) => ({ ...r, depth: 0 })));
      return;
    }
    if (!currentPath) { setBlocks([]); return; }
    try {
      const items = await getChildren(currentPath, 200);
      const top: Block[] = squarify(items, 0, 0, size.w, size.h).map((r) => ({ ...r, depth: 0 }));
      const nestable = top.filter((r) => r.entry.is_dir && r.w >= NEST_MIN_W && r.h >= NEST_MIN_H).slice(0, MAX_NESTED);
      // 某个大目录往里读失败（比如刚被删掉）只是少画那一层，不影响整张图
      const nested = await Promise.all(nestable.map((r) => loadNested(r).catch((): Block[] => [])));
      if (req === requestRef.current) {
        setBlocks([...top, ...nested.flat()]);
        setLoadError(null);
      }
    } catch (e) {
      if (req === requestRef.current) {
        setBlocks([]);
        setLoadError(String(e));
      }
    }
  }, [currentPath, scanning, liveFiltered, size]);

  useEffect(() => { load(); }, [load]);

  // 被嵌套的顶层块 → 标题（穿过的层用 › 连起来）
  const headers = useMemo(() => {
    const m = new Map<string, string>();
    blocks.forEach((b) => {
      if (b.depth === 1 && b.chain && !m.has(b.chain[0])) m.set(b.chain[0], b.chain.map(basename).join(" › "));
    });
    return m;
  }, [blocks]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w <= 0 || size.h <= 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const t = readTheme();
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);
    const font = getComputedStyle(document.body).fontFamily;
    const tint = isLight(t.layer) ? 1.5 : 1.3;

    for (const b of blocks) {
      const isHover = !b.rest && hover?.entry.path === b.entry.path;
      const share = b.rest ? null : lens?.(b.entry.path, b.entry.logical_size);
      const strength = (b.depth === 0 ? 0.16 : 0.26) + (isHover ? 0.14 : 0);
      // 有归类结果时：按性质着色，没认出的用中性灰；扫描中还没有归类，目录用强调色区分文件
      const base = b.rest
        ? mix(t.faint, t.layer, 0.07)
        : share
          ? mix(KINDS[share.kind].color, t.layer, Math.min(0.8, strength * tint * (0.55 + 0.45 * share.purity)))
          : b.entry.is_dir && !lens
            ? mix(t.accent, t.layer, strength - 0.03)
            : mix(t.faint, t.layer, strength);
      ctx.fillStyle = base;
      const inset = b.depth === 0 ? 1 : 0.75;
      ctx.beginPath();
      ctx.roundRect(b.x + inset, b.y + inset, Math.max(0, b.w - inset * 2), Math.max(0, b.h - inset * 2), b.depth === 0 ? 4 : 2);
      ctx.fill();
      if (isHover) {
        ctx.strokeStyle = t.accent;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }

      const header = b.depth === 0 ? headers.get(b.entry.path) : undefined;
      const isHeader = header !== undefined;
      if (b.depth === 0 && b.w > 44 && b.h > 20) {
        ctx.fillStyle = t.text;
        ctx.font = `600 12px ${font}`;
        const name = fitText(ctx, header ?? b.entry.name, b.w - 16);
        ctx.fillText(name, b.x + 8, b.y + 16);
        ctx.font = `11px ${font}`;
        ctx.fillStyle = t.text2;
        const sizeText = formatSize(b.entry.logical_size);
        if (isHeader) {
          const nameW = ctx.measureText(name).width;
          const sw = ctx.measureText(sizeText).width;
          if (8 + nameW + 10 + sw < b.w - 8) ctx.fillText(sizeText, b.x + b.w - 8 - sw, b.y + 16);
        } else if (b.h > 40) {
          ctx.fillText(sizeText, b.x + 8, b.y + 32);
        }
      } else if (b.depth === 1 && !b.rest && b.w > 46 && b.h > 18) {
        ctx.fillStyle = t.text;
        ctx.font = `11px ${font}`;
        ctx.fillText(fitText(ctx, b.entry.name, b.w - 10), b.x + 5, b.y + 13);
        if (b.h > 32) {
          ctx.fillStyle = t.text2;
          ctx.fillText(formatSize(b.entry.logical_size), b.x + 5, b.y + 27);
        }
      }
    }
  }, [blocks, hover, size, headers, themeTick, lens]);

  // 命中最深一层（不可交互的「其他」除外）
  const findBlock = (e: React.MouseEvent): Block | null => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const r = canvas.getBoundingClientRect();
    const mx = e.clientX - r.left, my = e.clientY - r.top;
    const hit = (b: Block) => mx >= b.x && mx <= b.x + b.w && my >= b.y && my <= b.y + b.h;
    return blocks.find((b) => b.depth === 1 && !b.rest && hit(b)) ?? blocks.find((b) => b.depth === 0 && hit(b)) ?? null;
  };

  const handleClick = (e: React.MouseEvent) => {
    if (scanning) return;
    const b = findBlock(e);
    if (!b) return;
    if (b.depth === 1 && b.chain) {
      b.chain.forEach(onNavigate);
      if (b.entry.is_dir) onNavigate(b.entry.path);
    } else if (b.entry.is_dir) {
      onNavigate(b.entry.path);
    }
  };

  return (
    <div className="flex flex-col h-full bg-abyss rounded-lg border border-slate overflow-hidden">
      {!scanning && navPath.length > 0 && (
        <div className="flex items-center gap-1 px-2 h-10 border-b border-slate shrink-0 text-[13px] overflow-x-auto">
          {navPath.map((p, i) => (
            <div key={p} className="flex items-center gap-1 shrink-0">
              {i > 0 && <ChevronRight className="w-3.5 h-3.5 text-text-faint" />}
              <button
                type="button"
                className={cn(
                  "px-2 h-7 rounded transition-colors hover:bg-hover",
                  i === navPath.length - 1 ? "text-textMain font-semibold" : "text-text-dim"
                )}
                onClick={() => !scanning && onNavigate(navPath[i])}
              >
                {basename(p)}
              </button>
            </div>
          ))}
          <div className="flex-1 min-w-[20px]" />
          {lens && (
            <div className="hidden xl:flex items-center gap-3 mr-2 shrink-0 text-[11px] text-text-dim" aria-label="颜色表示空间的性质">
              {KIND_ORDER.map((k) => (
                <span key={k} className="inline-flex items-center gap-1">
                  <i className="w-2 h-2 rounded-sm" style={{ background: KINDS[k].color }} />
                  {KINDS[k].name}
                </span>
              ))}
            </div>
          )}
          {onAnalyzePath && currentPath && (
            <button
              type="button"
              className="shrink-0 flex items-center gap-1.5 h-7 px-3 rounded text-xs bg-card border border-slate text-ai hover:bg-hover disabled:opacity-50 transition-colors"
              disabled={analyzing}
              onClick={() => onAnalyzePath(currentPath)}
            >
              {analyzing ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <AiIcon className="w-3.5 h-3.5" />}
              {analyzing ? "分析中" : "AI 分析这一层"}
            </button>
          )}
        </div>
      )}

      <div className="flex-1 min-h-0 relative" ref={wrapperRef}>
        {loadError && (
          <div className="absolute inset-0 grid place-items-center px-6 text-center text-xs text-text-dim">
            <span>这一层读不出来：{loadError}<br />可能已被删除或移动，点上方的路径回到上一层。</span>
          </div>
        )}
        <canvas
          ref={canvasRef}
          className="absolute inset-0 block"
          style={{ width: size.w, height: size.h, cursor: scanning ? "default" : "pointer" }}
          onMouseMove={(e) => setHover(findBlock(e))}
          onMouseLeave={() => setHover(null)}
          onClick={handleClick}
          onContextMenu={(e) => {
            if (!onContextMenu || scanning) return;
            const b = findBlock(e);
            if (b) { e.preventDefault(); onContextMenu(b.entry, e.clientX, e.clientY); }
          }}
        />
      </div>

      {hover && (() => {
        const ai = analyses?.get(hover.entry.path);
        const kind = lens?.(hover.entry.path, hover.entry.logical_size);
        return (
          <div className="fixed bottom-10 left-6 max-w-sm bg-card border border-slate rounded-lg p-3 pointer-events-none z-50" style={{ boxShadow: "var(--fl-shadow)" }}>
            <div className="flex items-start justify-between gap-6">
              <span className="text-[13px] font-semibold text-textMain break-all">{hover.entry.name}</span>
              <span className="text-[13px] text-textMain shrink-0">{formatSize(hover.entry.logical_size)}</span>
            </div>
            {hover.chain && <div className="mt-0.5 text-xs text-text-dim">在「{hover.chain.map(basename).join(" › ")}」里</div>}
            {kind && (
              <div className="mt-1 flex items-center gap-1.5 text-xs">
                <i className="w-2 h-2 rounded-sm" style={{ background: KINDS[kind.kind].color }} />
                {kind.purity >= 0.95 ? (
                  <span>{KINDS[kind.kind].name}</span>
                ) : kind.purity >= 0.5 ? (
                  <span>{KINDS[kind.kind].name}为主<span className="text-text-dim">，约占 {Math.round(kind.purity * 100)}%</span></span>
                ) : (
                  <span>性质混杂<span className="text-text-dim">，{KINDS[kind.kind].name}最多，约 {Math.round(kind.purity * 100)}%</span></span>
                )}
              </div>
            )}
            {hover.entry.is_dir && (
              <div className="mt-1 text-xs text-text-dim flex gap-3">
                <span>{formatNumber(hover.entry.subdirs)} 个目录</span>
                <span>{formatNumber(hover.entry.files)} 个文件</span>
              </div>
            )}
            {ai && (
              <div className="mt-2 pt-2 border-t border-slate flex items-center gap-2">
                <StarRating priority={ai.priority} />
                <span className="text-xs text-text-dim">{ai.description}</span>
              </div>
            )}
          </div>
        );
      })()}
    </div>
  );
}
