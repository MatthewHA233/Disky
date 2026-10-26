import { AlertTriangle, Loader2, Play, Plus, ShieldCheck } from "lucide-react";
import type { DriveInfo } from "../../types";
import { formatSize } from "../../lib/format";
import { driveLabel, driveOf, normalizePath } from "../../lib/path";
import { CATEGORIES, type ArchiveStats } from "../../lib/archiveModel";
import { CATEGORY_META } from "./categories";

interface Props {
  stats: ArchiveStats;
  /** 本机所有盘（用来提示还没扫描的） */
  drives: DriveInfo[];
  scanning: boolean;
  onScanDrive: (mount: string) => void;
  canAddBox: boolean;
  onAddBox: () => void;
  onCheck: () => void;
  checking: boolean;
  /** 模板模式：只按内置规则整理过 */
  template: boolean;
  /** 让整理助手复查一遍 */
  onReview: () => void;
  notesCount: number;
  onShowNotes: () => void;
  onReset: () => void;
}

function ago(unixSecs: number): string {
  const s = Math.max(0, Date.now() / 1000 - unixSecs);
  if (s < 90) return "刚刚扫描";
  if (s < 3600) return `${Math.round(s / 60)} 分钟前扫描`;
  if (s < 86400 * 2) return `${Math.round(s / 3600)} 小时前扫描`;
  return `${Math.round(s / 86400)} 天前扫描`;
}

const pct = (part: number, whole: number) => (whole > 0 ? Math.round((part / whole) * 100) : 0);

/** 顶部：总进度、每块盘一条按门类分段的进度条、能清理多少（AI 核对过的） */
export function ArchiveSummary({
  stats, drives, scanning, onScanDrive, canAddBox, onAddBox, onCheck, checking, template, onReview, notesCount, onShowNotes, onReset,
}: Props) {
  const scannedRoots = new Set(stats.drives.map((d) => normalizePath(d.root).toLowerCase()));
  const missing = drives.filter((d) => !scannedRoots.has(normalizePath(d.mount_point).toLowerCase()));
  const { user, ai, rule } = stats.bySource;
  const placedBy = [ai > 0 && `AI 放好了 ${ai} 项`, rule > 0 && `内置规则认出 ${rule} 项`, user > 0 && `你整理了 ${user} 项`].filter(Boolean).join("，");

  return (
    <section className="shrink-0 px-4 pt-3 pb-3 rounded-lg bg-abyss border border-slate">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-xl font-semibold">已整理 {pct(stats.organized, stats.total)}%</h1>
        <span className="text-[13px] text-text-dim">{formatSize(stats.organized)} / {formatSize(stats.total)}</span>
        <span className="flex flex-wrap items-center gap-x-3 text-xs text-text-dim">
          {CATEGORIES.map((c) => (
            <span key={c} className="inline-flex items-center gap-1.5">
              <i className="w-2 h-2 rounded-sm" style={{ background: CATEGORY_META[c].color }} />
              {CATEGORY_META[c].name} {formatSize(stats.byCategory[c])}
            </span>
          ))}
          <span className="inline-flex items-center gap-1.5">
            <i className="w-2 h-2 rounded-sm bg-slate" />还没整理 {formatSize(Math.max(0, stats.total - stats.organized))}
          </span>
        </span>
        <span className="ml-auto inline-flex items-baseline gap-2 text-[13px] text-text-dim">
          <span title="只算 AI 核对过、删了不丢东西的缓存和残留">能清理 <b className="font-semibold text-success">{formatSize(stats.junk)}</b></span>
          {stats.pending.length > 0 && (
            <button
              type="button"
              className="inline-flex items-center gap-1 text-xs text-ai hover:underline disabled:opacity-60 disabled:no-underline"
              disabled={checking}
              title="规则只按名字找出这些「像是缓存」的位置，名字像不代表真能删，让 AI 逐个核对"
              onClick={onCheck}
            >
              {checking ? <Loader2 className="w-3 h-3 animate-spin" /> : <ShieldCheck className="w-3 h-3" />}
              {checking ? "AI 正在核对" : `还有 ${formatSize(stats.pendingSize)} 待核对，让 AI 核对`}
            </button>
          )}
        </span>
        <button
          type="button"
          className="h-7 px-2 inline-flex items-center gap-1 rounded text-xs text-photon hover:bg-hover active:bg-press disabled:opacity-40 disabled:pointer-events-none transition-colors"
          disabled={!canAddBox}
          title={canAddBox ? "在当前页签新建一个自己的箱子" : "这个页签的箱子已经放满了"}
          onClick={onAddBox}
        >
          <Plus className="w-3.5 h-3.5" />新建箱子
        </button>
      </div>

      {/* 每块盘一条：按门类分段，灰色是还没整理的 */}
      <div className="mt-2.5 grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-1.5">
        {stats.drives.map((d) => (
          <div key={d.root} className="contents">
            <span className="text-xs font-semibold w-12">{driveLabel(driveOf(d.root))}</span>
            <div className="flex gap-[2px] h-2 rounded-full overflow-hidden bg-slate" aria-label={`${driveLabel(driveOf(d.root))}已整理 ${pct(d.organized, d.total)}%`}>
              {CATEGORIES.map((c) => (d.byCategory[c] > 0 && d.total > 0 ? (
                <i
                  key={c}
                  className="block h-full transition-[flex-basis]"
                  style={{ flexBasis: `${(d.byCategory[c] / d.total) * 100}%`, background: CATEGORY_META[c].color }}
                  title={`${CATEGORY_META[c].name} ${formatSize(d.byCategory[c])}`}
                />
              ) : null))}
            </div>
            <span className="text-xs text-text-dim whitespace-nowrap">
              <b className="font-semibold text-textMain">{pct(d.organized, d.total)}%</b> · {formatSize(d.organized)} / {formatSize(d.total)} · {ago(d.scannedAt)}
              {d.live && <span title="扫描之后新建、删除、变大变小的，几秒内自动更新到这里，不用重扫">，之后实时更新</span>}
            </span>
          </div>
        ))}
        {missing.map((d) => (
          <div key={d.mount_point} className="contents">
            <span className="text-xs font-semibold w-12 text-text-dim">{driveLabel(driveOf(d.mount_point))}</span>
            <div className="h-2 rounded-full border border-dashed border-[color:var(--fl-stroke-strong)]" />
            <span className="text-xs text-text-dim whitespace-nowrap">
              还没扫描
              <button
                type="button"
                className="ml-2 inline-flex items-center gap-0.5 text-photon hover:underline disabled:opacity-50 disabled:no-underline"
                disabled={scanning}
                onClick={() => onScanDrive(d.mount_point)}
              >
                <Play className="w-3 h-3" />扫描
              </button>
            </span>
          </div>
        ))}
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        {template ? (
          <p className="flex items-center gap-1.5 text-caution">
            <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
            模板模式：只按内置规则看名字认，大概率有放错、认错的地方。拖动图标就能改，
            <button type="button" className="text-photon hover:underline" onClick={onReview}>或者让 AI 复查一遍</button>
          </p>
        ) : (
          <p className="text-text-dim">{placedBy ? `${placedBy}，拖动图标就能改。` : "把左边的东西拖进箱子，或者让 AI 先归一遍类。"}</p>
        )}
        <span className="ml-auto flex items-center gap-3">
          {notesCount > 0 && (
            <button type="button" className="text-text-dim hover:text-textMain hover:underline" title="整理助手记下的你的回答和偏好，每次开场都会交代给它" onClick={onShowNotes}>
              AI 记住了 {notesCount} 条
            </button>
          )}
          <button type="button" className="text-text-dim hover:text-textMain hover:underline" onClick={onReset}>重新开始整理</button>
        </span>
      </div>
    </section>
  );
}
