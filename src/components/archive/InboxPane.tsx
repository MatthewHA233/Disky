import { memo, type ReactNode } from "react";
import { ChevronRight, HardDrive, Loader2 } from "lucide-react";
import { AiIcon } from "../ui/AiIcon";
import type { ArchiveBoxRow, ArchiveCategory } from "../../types";
import { formatSize } from "../../lib/format";
import { driveLabel, driveOf } from "../../lib/path";
import {
  appSize, growthOf, guessOf, itemSize, visibleKids, type ArchiveIndex, type ArchiveStats, type FolderNode, type Guess, type Item, type Placement,
} from "../../lib/archiveModel";
import { cn } from "../../lib/utils";
import { GrowthText, ItemCell, itemTooltip } from "./ItemViews";

interface Props {
  category: ArchiveCategory;
  ix: ArchiveIndex;
  stats: ArchiveStats;
  sizes: Map<string, number>;
  icons: ReadonlyMap<string, string | null>;
  boxes: Map<string, ArchiveBoxRow>;
  placementOf: (key: string) => Placement | null;
  junkOf: (item: Item) => number;
  width: number;
  dropActive: boolean;
  openKey: string | null;
  draggingKey: string | null;
  /** 文件夹正拖到哪个软件上 */
  claimKey: string | null;
  expanded: ReadonlySet<string>;
  onToggle: (key: string) => void;
  onAccept: (key: string, box: string) => void;
  /** 有猜测的都按猜测放 */
  onAcceptAll: () => void;
  guessable: number;
  onClassify: () => void;
  classifying: boolean;
  /** 让 AI 用工具查没认出的文件夹是哪个软件的 */
  onAttribute: () => void;
  attributing: boolean;
  agentStatus: string | null;
}

const NEUTRAL = "#7A7574";

function SectionHead({ title, meta, action }: { title: string; meta: string; action?: ReactNode }) {
  return (
    <div className="flex items-center gap-2 px-3.5 pt-3 pb-1 text-xs text-text-dim">
      <b className="font-semibold text-textMain">{title}</b>
      <span>{meta}</span>
      {action && <span className="ml-auto">{action}</span>}
    </div>
  );
}

function SmallFolder({ isDir }: { isDir: boolean }) {
  return isDir ? (
    <svg viewBox="0 0 20 20" className="w-[18px] h-[18px] shrink-0" aria-hidden="true">
      <path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h4.1a1.5 1.5 0 0 1 1 .4L9.9 5.5h6.6A1.5 1.5 0 0 1 18 7v7.5a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 2 14.5Z" fill="#E8A93B" />
      <path d="M2 7.5h16v7a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 2 14.5Z" fill="#F6C343" />
    </svg>
  ) : (
    <svg viewBox="0 0 20 20" className="w-[18px] h-[18px] shrink-0" aria-hidden="true">
      <path d="M5 2h7l4 4v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1Z" fill="var(--fl-card)" stroke="var(--fl-stroke-strong)" />
    </svg>
  );
}

function GuessButton({ guess, box, onAccept }: { guess: Guess; box: ArchiveBoxRow; onAccept: () => void }) {
  const who = guess.source === "ai" ? `AI 猜是${box.name}（把握 ${Math.round((guess.confidence ?? 0) * 100)}%）` : `按名字看像是${box.name}`;
  return (
    <button
      type="button"
      title={`${who}，点一下放进去`}
      className="shrink-0 inline-flex items-center gap-1 h-[18px] px-1.5 rounded-full text-[11px] text-text-dim hover:text-textMain"
      style={{ background: `color-mix(in srgb, ${box.color} 14%, transparent)` }}
      onClick={onAccept}
    >
      <i className="w-1.5 h-1.5 rounded-full" style={{ background: box.color }} />
      像是{box.name}
    </button>
  );
}

interface RowProps {
  node: FolderNode;
  ix: ArchiveIndex;
  sizes: Map<string, number>;
  boxes: Map<string, ArchiveBoxRow>;
  expanded: ReadonlySet<string>;
  openKey: string | null;
  draggingKey: string | null;
  onToggle: (key: string) => void;
  onAccept: (key: string, box: string) => void;
  /** 个人文件页签里给出「像是××」的猜测；软件页签里这棵树是拿来往软件上拖的，不显示 */
  showGuesses: boolean;
}

/** 待整理目录树的一行（及展开的子行） */
const FolderRow = memo(function FolderRow(props: RowProps) {
  const { node, ix, sizes, boxes, expanded, openKey, draggingKey, onToggle, onAccept, showGuesses } = props;
  const kids = visibleKids(ix, node, sizes);
  const open = expanded.has(node.key);
  const isDrive = node.depth === 0;
  const size = sizes.get(node.key) ?? 0;
  const guess = isDrive || !showGuesses ? null : guessOf(ix, node.key);
  const guessBox = guess ? boxes.get(guess.box) : undefined;
  const grown = isDrive ? undefined : growthOf(ix, node.key);
  return (
    <>
      <div
        data-item={isDrive ? undefined : node.key}
        onClick={isDrive ? () => onToggle(node.key) : undefined}
        title={node.path}
        className={cn(
          "h-8 flex items-center gap-1.5 pr-2 rounded select-none transition-colors",
          openKey === node.key ? "bg-photon/10" : "hover:bg-hover",
          draggingKey === node.key && "opacity-40",
          isDrive && "font-semibold"
        )}
        style={{ paddingLeft: 4 + node.depth * 16 }}
      >
        {kids.length > 0 ? (
          <button
            type="button"
            className="w-5 h-5 shrink-0 grid place-items-center rounded text-text-faint hover:bg-hover hover:text-textMain"
            aria-expanded={open}
            aria-label={open ? "收起" : "展开"}
            onClick={(e) => { e.stopPropagation(); onToggle(node.key); }}
          >
            <ChevronRight className={cn("w-3 h-3 transition-transform", open && "rotate-90")} />
          </button>
        ) : (
          <span className="w-5 shrink-0" />
        )}
        {isDrive ? <HardDrive className="w-[18px] h-[18px] shrink-0 text-text-dim" /> : <SmallFolder isDir={node.isDir} />}
        <span className="flex-1 min-w-0 truncate text-[13px]">{isDrive ? driveLabel(driveOf(node.path)) : node.name}</span>
        {guess && guessBox && <GuessButton guess={guess} box={guessBox} onAccept={() => onAccept(node.key, guess.box)} />}
        {!open && kids.length > 0 && <span className="shrink-0 text-[11px] text-text-faint">{kids.length} 项</span>}
        {grown !== undefined && <GrowthText grown={grown} className="shrink-0 text-[11px]" />}
        <span className="shrink-0 text-xs text-text-dim">{formatSize(size)}</span>
      </div>
      {open && kids.map((k) => <FolderRow key={k.key} {...props} node={k} />)}
    </>
  );
});

/** 左侧「待整理」：软件 / 开发环境页签里是还没进箱子的项，下面跟着没认出归属的文件夹树（拖到软件上就归给它）；
 * 个人文件页签里只有这棵树 */
export function InboxPane(props: Props) {
  const {
    category, ix, stats, sizes, icons, boxes, placementOf, junkOf, width, dropActive, openKey, draggingKey, claimKey, expanded,
    onToggle, onAccept, onAcceptAll, guessable, onClassify, classifying, onAttribute, attributing, agentStatus,
  } = props;
  const personal = category === "personal";
  const apps = personal ? [] : stats.inbox[category];
  const total = personal ? stats.looseSize : apps.reduce((s, a) => s + appSize(a), 0);
  const noun = category === "software" ? "软件" : "工具";
  const tree = (
    <div className="px-1.5">
      {ix.roots.map((r) => (
        <FolderRow
          key={r.key}
          node={r}
          ix={ix}
          sizes={sizes}
          boxes={boxes}
          expanded={expanded}
          openKey={openKey}
          draggingKey={draggingKey}
          onToggle={onToggle}
          onAccept={onAccept}
          showGuesses={personal}
        />
      ))}
    </div>
  );

  return (
    <aside
      aria-label="待整理"
      className={cn(
        "shrink-0 flex flex-col min-h-0 rounded-lg bg-abyss border border-slate overflow-hidden",
        dropActive && "outline outline-2 -outline-offset-2 outline-photon"
      )}
      style={{ width }}
    >
      <header className="h-11 shrink-0 flex items-center gap-2 pl-3.5 pr-1.5 border-b border-slate">
        <h2 className="text-sm font-semibold">待整理</h2>
        <span className="text-xs text-text-faint">{formatSize(total)}</span>
        <button
          type="button"
          className="ml-auto h-7 px-2.5 inline-flex items-center gap-1.5 rounded text-xs text-ai hover:bg-hover active:bg-press disabled:opacity-60 disabled:pointer-events-none transition-colors"
          onClick={onClassify}
          disabled={classifying}
          title={category === "personal" ? "让 AI 按内容给这些文件夹归类，拿不准的会留在这里" : `让 AI 给这些${noun}归类，拿不准的会留在这里`}
        >
          {classifying ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <AiIcon className="w-3.5 h-3.5" />}
          {classifying ? "AI 正在归类" : "让 AI 归类"}
        </button>
      </header>

      <div data-drop="inbox" className={cn("flex-1 min-h-0", personal ? "overflow-auto pb-2" : "flex flex-col")}>
        {personal ? (
          <>
            <SectionHead
              title="没认出归属的文件夹"
              meta={formatSize(stats.looseSize)}
              action={guessable > 0 ? (
                <button type="button" className="text-xs text-photon hover:underline" onClick={onAcceptAll}>都按猜的放（{guessable}）</button>
              ) : undefined}
            />
            <p className="px-3.5 pb-1.5 text-xs leading-5 text-text-faint">
              {stats.looseCount > 0
                ? `还有 ${stats.looseCount} 处没整理，最深在第 ${stats.deepest} 层。把文件夹拖进箱子，它下面的层就都算整理好了；是某个软件的，就拖到软件页签里那个软件的图标上。`
                : "大的文件夹都整理好了。"}
            </p>
            {tree}
          </>
        ) : (
          <>
            {/* 上下两段各自滚动：软件多了最多占一半，下面的文件夹树始终露着，拖到软件上才方便 */}
            <div className="shrink-0 max-h-[50%] overflow-auto">
              <SectionHead
                title={`还没进箱子的${noun}`}
                meta={String(apps.length)}
                action={guessable > 0 ? (
                  <button type="button" className="text-xs text-photon hover:underline" onClick={onAcceptAll}>都按猜的放（{guessable}）</button>
                ) : undefined}
              />
              {apps.length === 0 ? (
                <p className="px-3.5 pb-1 text-xs text-text-faint">都已经放进箱子了。</p>
              ) : (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(86px,1fr))] gap-0.5 px-1.5 pb-1.5">
                  {apps.map((a) => {
                    const item = { type: "app" as const, key: a.key, name: a.name, app: a };
                    const g = guessOf(ix, a.key);
                    const gb = g ? boxes.get(g.box) : undefined;
                    const junk = junkOf(item);
                  const grown = growthOf(ix, a.key);
                    return (
                      <ItemCell
                        key={a.key}
                        item={item}
                        sizes={sizes}
                        icon={a.icon ? icons.get(a.icon) : null}
                        color={gb?.color ?? NEUTRAL}
                        open={openKey === a.key}
                        dragging={draggingKey === a.key}
                        claimTarget={claimKey === a.key}
                        junk={junk}
                        grown={grown}
                      title={itemTooltip(item, itemSize(item, sizes), junk, placementOf(a.key), boxes, grown)}
                        guess={g && gb ? { guess: g, box: gb } : null}
                        onAccept={onAccept}
                      />
                    );
                  })}
                </div>
              )}
            </div>
            <div className="flex-1 min-h-0 overflow-auto pb-2 border-t border-slate">
              <SectionHead
                title="没认出归属的文件夹"
                meta={formatSize(stats.looseSize)}
                action={stats.looseCount > 0 ? (
                  <button
                    type="button"
                    className="inline-flex items-center gap-1 text-xs text-ai hover:underline disabled:opacity-60 disabled:no-underline"
                    disabled={attributing}
                    title="AI 会自己翻看这些文件夹、读 exe 的厂商信息和配置文件，查出是哪个软件的；拿不准的留给你"
                    onClick={onAttribute}
                  >
                    {attributing ? <Loader2 className="w-3 h-3 animate-spin" /> : <AiIcon className="w-3 h-3" />}
                    {attributing ? "AI 正在查" : "让 AI 查是谁的"}
                  </button>
                ) : undefined}
              />
              {attributing && agentStatus ? (
                <p className="px-3.5 pb-1.5 text-xs leading-5 text-ai truncate" title={agentStatus}>{agentStatus}</p>
              ) : (
                <p className="px-3.5 pb-1.5 text-xs leading-5 text-text-faint">
                  {stats.looseCount > 0
                    ? "是哪个软件的，就拖到那个软件的图标上，再选它算数据、缓存还是别的。"
                    : "大的文件夹都认出归属了。"}
                </p>
              )}
              {tree}
            </div>
          </>
        )}
      </div>
    </aside>
  );
}
