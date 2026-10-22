// 新手引导左边的插画，每一步一张，动效写在 onboarding.css。
import type { CSSProperties } from "react";
import { HardDrive } from "lucide-react";
import type { ArchiveMode } from "../../types";
import { AiIcon } from "../ui/AiIcon";

type Vars = CSSProperties & Record<`--${string}`, string | number>;

const BOXES = [
  { x: 16, y: 20, w: 160, h: 120, color: "var(--oobe-sw)", tiles: 6, cols: 4 },
  { x: 184, y: 20, w: 160, h: 120, color: "var(--oobe-personal)", tiles: 5, cols: 4 },
  { x: 16, y: 152, w: 328, h: 80, color: "var(--oobe-dev)", tiles: 7, cols: 9 },
];

/** 欢迎：散落的图标飞进三个箱子（软件、个人文件、开发环境） */
export function WelcomeArt() {
  let i = 0;
  return (
    <svg viewBox="0 0 360 248" className="w-[min(420px,80%)] oobe-float" aria-hidden="true">
      {BOXES.map((b) => (
        <g key={b.x + b.y}>
          <rect className="oobe-box-fill" x={b.x} y={b.y} width={b.w} height={b.h} rx="10" fill={b.color} fillOpacity="0.08" />
          <rect className="oobe-box" pathLength={1} x={b.x} y={b.y} width={b.w} height={b.h} rx="10" fill="none" stroke={b.color} strokeWidth="1.5" />
          <rect className="oobe-box-fill" x={b.x + 14} y={b.y + 12} width="40" height="6" rx="3" fill={b.color} opacity="0.6" />
          {Array.from({ length: b.tiles }, (_, t) => {
            const n = i++;
            const x = b.x + 14 + (t % b.cols) * 34;
            const y = b.y + 30 + Math.floor(t / b.cols) * 34;
            const style: Vars = {
              "--i": n,
              "--sx": `${((n * 97) % 260) - 130}px`,
              "--sy": `${((n * 61) % 180) - 110}px`,
              "--r": `${((n * 47) % 120) - 60}deg`,
            };
            return (
              <g key={t} className="oobe-tile" style={style}>
                <rect x={x} y={y} width="24" height="24" rx="6" fill={b.color} />
                <rect x={x + 6} y={y + 6} width="12" height="12" rx="3" fill="#fff" opacity="0.35" />
              </g>
            );
          })}
        </g>
      ))}
    </svg>
  );
}

const delay = (ms: number): CSSProperties => ({ animationDelay: `${ms}ms` });

/** 接入 AI：助手先问一句、给出选项，替你点中一个 */
export function AiArt() {
  return (
    <div className="w-[300px] p-4 flex flex-col gap-3 rounded-lg bg-card border border-slate oobe-float" style={{ boxShadow: "var(--fl-shadow-dialog)" }} aria-hidden="true">
      <div className="flex items-center gap-2 text-xs font-semibold">
        <AiIcon className="w-4 h-4 text-photon" />整理助手
      </div>
      <div className="self-end w-[62%] p-2.5 flex flex-col gap-1.5 rounded-lg rounded-tr-sm bg-photon oobe-pop" style={delay(200)}>
        <i className="h-1.5 rounded-full bg-on-accent/60" />
        <i className="h-1.5 w-2/3 rounded-full bg-on-accent/60" />
      </div>
      <div className="flex items-start gap-2">
        <span className="w-7 h-7 shrink-0 grid place-items-center rounded-full bg-abyss border border-slate oobe-pop" style={delay(500)}>
          <AiIcon className="w-4 h-4 text-ai oobe-twinkle" />
        </span>
        <div className="relative flex-1 p-2.5 flex flex-col gap-1.5 rounded-lg rounded-tl-sm bg-abyss border border-slate oobe-pop" style={delay(500)}>
          <span className="absolute left-2.5 top-2.5 flex gap-1 oobe-typing">
            {[0, 150, 300].map((d) => <i key={d} className="w-1.5 h-1.5 rounded-full bg-text-dim oobe-dot" style={delay(d)} />)}
          </span>
          <i className="h-1.5 rounded-full bg-[color:var(--fl-stroke-strong)] oobe-line" style={delay(1500)} />
          <i className="h-1.5 w-5/6 rounded-full bg-[color:var(--fl-stroke-strong)] oobe-line" style={delay(1600)} />
          <i className="h-1.5 w-1/2 rounded-full bg-[color:var(--fl-stroke-strong)] oobe-line" style={delay(1700)} />
        </div>
      </div>
      <div className="pl-9 flex flex-wrap gap-1.5 text-[11px]">
        {["放进「游戏」", "这是我的文件", "先跳过"].map((t, k) => (
          <span key={t} className="oobe-pop" style={delay(2100 + k * 100)}>
            <span className={`inline-block px-2 py-1 rounded border border-slate bg-card ${k === 0 ? "oobe-pick" : ""}`}>{t}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

/** 扫描：Fluent 的进度环，扫描时转圈、中间是这块盘扫到百分之几；下面的条子是正在扫的最大几个文件夹 */
export function ScanArt({ done, total, scanning, pct, top }: { done: number; total: number; scanning: boolean; pct: number; top: number[] }) {
  const frac = total > 0 ? done / total : 0;
  const live = scanning && top.length > 0;
  const max = Math.max(1, ...top);
  return (
    <div className={`flex flex-col items-center gap-6 oobe-float ${scanning ? "oobe-scanning" : ""}`} aria-hidden="true">
      <div className="relative w-[220px] h-[220px]">
        <svg viewBox="0 0 220 220" className="absolute inset-0">
          <circle cx="110" cy="110" r="90" fill="none" stroke="var(--fl-stroke)" strokeWidth="6" />
          <g className={scanning ? "oobe-ring-spin" : ""}>
            <circle
              className="oobe-ring-arc"
              cx="110" cy="110" r="90"
              fill="none" stroke="var(--fl-accent)" strokeWidth="6" strokeLinecap="round"
              pathLength={1} strokeDasharray="1" strokeDashoffset={scanning ? 0.75 : 1 - frac}
              transform="rotate(-90 110 110)"
            />
          </g>
        </svg>
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-1">
          <HardDrive className="w-12 h-12 text-text-dim" strokeWidth={1.5} />
          <span className="text-lg font-semibold tabular-nums">{scanning ? `${Math.round(pct)}%` : `${done} / ${total}`}</span>
          <span className="text-xs text-text-dim">{scanning ? "正在扫描" : "块盘扫过了"}</span>
        </div>
      </div>
      <div className="w-[240px] flex flex-col gap-2">
        {live
          ? top.map((size, k) => (
              <i key={k} className="block h-2 rounded-full oobe-live-bar" style={{ width: `${Math.max(4, (size / max) * 100)}%`, background: k === 0 ? "var(--fl-accent)" : "var(--fl-stroke-strong)" }} />
            ))
          : [92, 66, 44, 26].map((w, k) => (
              <i key={w} className="block h-2 rounded-full oobe-bar" style={{ width: `${w}%`, background: k === 0 ? "var(--fl-accent)" : "var(--fl-stroke-strong)", ...delay(200 + k * 120) }} />
            ))}
      </div>
    </div>
  );
}

const SEGMENTS: [string, number][][] = [
  [["var(--oobe-sw)", 48], ["var(--oobe-personal)", 18], ["var(--oobe-dev)", 12]],
  [["var(--oobe-sw)", 30], ["var(--oobe-personal)", 34], ["var(--oobe-dev)", 8]],
];

/** 开始：每块盘的整理进度条填起来，打个勾 */
export function StartArt({ mode }: { mode: ArchiveMode }) {
  const color = mode === "ai" ? "var(--fl-accent)" : "var(--fl-caution)";
  return (
    <div className="flex flex-col items-center gap-7 oobe-float" aria-hidden="true">
      <svg viewBox="-40 -40 80 80" className="w-24 h-24">
        <circle r="34" fill={color} fillOpacity="0.12" className="oobe-box-fill" style={delay(0)} />
        <circle r="34" fill="none" stroke={color} strokeWidth="3" pathLength={1} className="oobe-box" />
        <path d="M-14 1 l9 9 l19 -21" fill="none" stroke={color} strokeWidth="4" strokeLinecap="round" strokeLinejoin="round" pathLength={1} className="oobe-check" />
      </svg>
      <div className="w-[280px] flex flex-col gap-3">
        {SEGMENTS.map((segs, row) => (
          <div key={row} className="flex items-center gap-2.5">
            <span className="w-9 text-xs font-semibold text-text-dim">{row === 0 ? "C 盘" : "D 盘"}</span>
            <span className="flex-1 flex gap-[2px] h-2 rounded-full overflow-hidden bg-[color:var(--fl-stroke)]">
              {segs.map(([c, w], k) => (
                <i key={k} className="block h-full oobe-fill" style={{ flexBasis: `${w}%`, background: c, ...delay(300 + row * 200 + k * 120) }} />
              ))}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
