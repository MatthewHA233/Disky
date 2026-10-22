import { memo } from "react";
import type { ArchiveBoxRow, ArchivePart } from "../../types";
import { formatDelta, formatSize } from "../../lib/format";
import { itemSize, sizeByDrive, type Item, type Placement, type Guess } from "../../lib/archiveModel";
import { cn } from "../../lib/utils";
import { driveOf } from "../../lib/path";

const sizeByDriveOf = (app: { parts: readonly ArchivePart[] }) => sizeByDrive(app, driveOf);

/** 可清理的量不小于这个才在图标下面单独标出来 */
export const JUNK_SHOWN = 100 * 1024 * 1024;

const FOLDER_ICON = (
  <svg viewBox="0 0 40 40" className="w-full h-full" aria-hidden="true">
    <path d="M4 11a3 3 0 0 1 3-3h8.2a3 3 0 0 1 2.1.9L19.8 11H33a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3Z" fill="#E8A93B" />
    <path d="M4 15h32v14a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3Z" fill="#F6C343" />
  </svg>
);

const FILE_ICON = (
  <svg viewBox="0 0 40 40" className="w-full h-full" aria-hidden="true">
    <path d="M10 4h14l8 8v22a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" fill="var(--fl-card)" stroke="var(--fl-stroke-strong)" />
    <path d="M24 4v8h8" fill="none" stroke="var(--fl-stroke-strong)" />
  </svg>
);

function initials(name: string): string {
  const chars = [...name.trim()];
  if (chars.length === 0) return "?";
  if (/[a-z0-9]/i.test(chars[0])) {
    const second = chars[1] && /[a-z0-9]/i.test(chars[1]) ? chars[1] : "";
    return (chars[0] + second).toUpperCase();
  }
  return chars[0];
}

/** 软件用真实图标；取不到图标时用箱子颜色的首字母；已卸载的用虚线框 */
export function ItemIcon({ item, icon, color, size = 40 }: { item: Item; icon: string | null | undefined; color: string; size?: number }) {
  const style = { width: size, height: size };
  if (item.type === "dir") return <span className="block shrink-0" style={style}>{item.node.isDir ? FOLDER_ICON : FILE_ICON}</span>;
  if (icon) return <img src={icon} alt="" draggable={false} className="block shrink-0 select-none" style={style} />;
  const ghost = item.app.uninstalled;
  return (
    <span
      className={cn("shrink-0 grid place-items-center rounded-[22%] font-semibold select-none", ghost && "border border-dashed border-[color:var(--fl-stroke-strong)] text-text-faint")}
      style={{
        ...style,
        fontSize: Math.round(size * 0.36),
        background: ghost ? "transparent" : `color-mix(in srgb, ${color} 16%, var(--fl-card))`,
        color: ghost ? undefined : color,
      }}
    >
      {initials(item.name)}
    </span>
  );
}

/** 悬停提示：大小、跨盘分布、可清理的量、谁放的 */
export function itemTooltip(item: Item, size: number, junk: number, placement: Placement | null, boxes: Map<string, ArchiveBoxRow>, grown?: number): string {
  const lines = [item.type === "app" ? item.name : item.node.path, formatSize(size)];
  if (grown !== undefined) lines.push(`比上次扫描${grown > 0 ? "大了" : "小了"} ${formatSize(Math.abs(grown))}`);
  if (item.type === "app") {
    const by = sizeByDriveOf(item.app);
    if (by.size > 1) lines.push([...by].sort().map(([d, s]) => `${d} 盘 ${formatSize(s)}`).join(" · "));
    if (item.app.uninstalled) lines.push("像是卸载后留下的残留");
    else if (junk > 0) lines.push(`可清理 ${formatSize(junk)}（AI 核对过）`);
  }
  if (placement?.source === "ai") lines.push(`AI 放进了「${boxes.get(placement.box)?.name ?? ""}」，把握 ${Math.round((placement.confidence ?? 0) * 100)}%`);
  if (placement?.source === "rule") lines.push("按内置规则放的");
  return lines.join("\n");
}

interface CellProps {
  item: Item;
  sizes: Map<string, number>;
  icon: string | null | undefined;
  color: string;
  open: boolean;
  dragging: boolean;
  title: string;
  /** AI 核对过、可以放心清理的量 */
  junk: number;
  guess?: { guess: Guess; box: ArchiveBoxRow } | null;
  onAccept?: (key: string, box: string) => void;
  /** 有文件夹正拖到这个软件上，松手就归给它 */
  claimTarget?: boolean;
  /** 比上次扫描变了多少（只在变化值得一提时给） */
  grown?: number;
}

const CLAIM_RING = "outline outline-2 -outline-offset-2 outline-photon bg-photon/10";

/** 大小变化的小字：变大用警示色，变小用成功色 */
export function GrowthText({ grown, className }: { grown: number; className?: string }) {
  return (
    <span className={cn("tabular-nums", grown > 0 ? "text-caution" : "text-success", className)} title={`比上次扫描${grown > 0 ? "大了" : "小了"} ${formatSize(Math.abs(grown))}`}>
      {formatDelta(grown)}
    </span>
  );
}

function GuessChip({ guess, box, itemKey, onAccept }: { guess: Guess; box: ArchiveBoxRow; itemKey: string; onAccept?: (key: string, box: string) => void }) {
  const tip = guess.source === "ai"
    ? `AI 猜是${box.name}（把握 ${Math.round((guess.confidence ?? 0) * 100)}%${guess.reason ? `，${guess.reason}` : ""}），点一下放进去`
    : `像是${box.name}，点一下放进去`;
  return (
    <button
      type="button"
      title={tip}
      className="inline-flex items-center gap-1 max-w-full h-[18px] px-1.5 rounded-full text-[11px] text-text-dim hover:text-textMain transition-colors"
      style={{ background: `color-mix(in srgb, ${box.color} 14%, transparent)` }}
      onClick={() => onAccept?.(itemKey, guess.box)}
    >
      <i className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: box.color }} />
      <span className="truncate">像是{box.name}</span>
    </button>
  );
}

/** 图标格子里的一项 */
export const ItemCell = memo(function ItemCell({ item, sizes, icon, color, open, dragging, title, junk, guess, onAccept, claimTarget, grown }: CellProps) {
  const size = itemSize(item, sizes);
  return (
    <div
      data-item={item.key}
      title={title}
      className={cn(
        "min-w-0 flex flex-col items-center px-1 pt-2.5 pb-2 rounded-md text-center select-none transition-colors",
        open ? "bg-photon/10" : "hover:bg-hover active:bg-press",
        dragging && "opacity-35",
        claimTarget && CLAIM_RING
      )}
    >
      <ItemIcon item={item} icon={icon} color={color} />
      <div className="mt-1.5 max-w-full text-xs leading-4 truncate">{item.name}</div>
      {item.type === "dir" && item.where && <div className="max-w-full text-[11px] leading-[15px] text-text-dim truncate">{item.where}</div>}
      <div className="text-[11px] leading-[15px] text-text-faint">
        {formatSize(size)}
        {grown !== undefined && <GrowthText grown={grown} className="ml-1" />}
      </div>
      {item.type === "app" && item.app.uninstalled ? (
        <div className="text-[11px] leading-[15px] text-caution">像是已卸载</div>
      ) : junk >= JUNK_SHOWN ? (
        <div className="text-[11px] leading-[15px] text-success">可清 {formatSize(junk)}</div>
      ) : null}
      {guess && <div className="mt-1 max-w-full"><GuessChip guess={guess.guess} box={guess.box} itemKey={item.key} onAccept={onAccept} /></div>}
    </div>
  );
});

/** 清单视图里的一行 */
export const ItemRow = memo(function ItemRow({ item, sizes, icon, color, open, dragging, title, junk, claimTarget, grown }: CellProps) {
  const size = itemSize(item, sizes);
  const drives = item.type === "app" ? [...sizeByDriveOf(item.app).keys()].sort() : [driveOf(item.node.path)];
  return (
    <div
      data-item={item.key}
      title={title}
      className={cn(
        "h-9 flex items-center gap-2 px-2 rounded select-none transition-colors",
        open ? "bg-photon/10" : "hover:bg-hover active:bg-press",
        dragging && "opacity-35",
        claimTarget && CLAIM_RING
      )}
    >
      <ItemIcon item={item} icon={icon} color={color} size={22} />
      <span className="flex-1 min-w-0 truncate text-[13px]">
        {item.name}
        {item.type === "dir" && item.where && <span className="ml-1.5 text-xs text-text-faint">{item.where}</span>}
      </span>
      <span className="flex gap-0.5">
        {drives.map((d) => (
          <span key={d} className="w-5 h-[18px] grid place-items-center rounded-[3px] border border-[color:var(--fl-stroke-strong)] text-[11px] text-text-dim">{d}</span>
        ))}
      </span>
      {grown !== undefined && <GrowthText grown={grown} className="shrink-0 text-xs" />}
      <span className="w-16 text-right text-xs text-text-dim">{formatSize(size)}</span>
      <span className={cn("w-[76px] text-right text-xs", item.type === "app" && item.app.uninstalled ? "text-caution" : "text-success")}>
        {item.type === "app" && item.app.uninstalled ? "像是已卸载" : junk >= JUNK_SHOWN ? `可清 ${formatSize(junk)}` : ""}
      </span>
    </div>
  );
});
