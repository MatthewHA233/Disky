import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, FolderOpen, Loader2, MoreHorizontal, ShieldCheck, X } from "lucide-react";
import type { ArchiveBoxRow, ArchivePart, ClaimKind, PartKind } from "../../types";
import { formatSize, timeAgo } from "../../lib/format";
import { basename, driveLabel, driveOf, shortenHome } from "../../lib/path";
import {
  CATEGORIES, JUNK_KINDS, checkOf, growthOf, isCleanable, isUnchecked, itemSize, sizeByDrive, visibleKids,
  type ArchiveIndex, type Guess, type Item, type Placement,
} from "../../lib/archiveModel";
import { CATEGORY_META } from "./categories";
import { Checkbox } from "../ui/Checkbox";
import { Button } from "../ui/Button";
import { Select } from "../ui/Field";
import { ItemIcon } from "./ItemViews";
import { ClaimMenu } from "./ClaimMenu";

const DRIVE_COLORS = ["#0078D4", "#8764B8", "#038387", "#CA5010", "#498205"];

const GROUPS: { kinds: PartKind[]; title: string; desc: string }[] = [
  { kinds: ["prog"], title: "程序本体", desc: "卸载软件才会释放。" },
  { kinds: ["regen"], title: "可重新下载", desc: "游戏本体、依赖包、扩展等，删了以后要重新下载。" },
  { kinds: ["data", "system"], title: "软件数据", desc: "设置、聊天记录、存档等，删之前要想清楚。" },
];

interface Props {
  item: Item;
  ix: ArchiveIndex;
  sizes: Map<string, number>;
  icon: string | null | undefined;
  boxes: ArchiveBoxRow[];
  placement: Placement | null;
  guess: Guess | null;
  onClose: () => void;
  onMove: (box: string | null) => void;
  onClean: (parts: ArchivePart[]) => void;
  onReveal: (path: string) => void;
  /** 让 AI 核对这几块「像是缓存 / 残留」的位置 */
  onCheck: (parts: ArchivePart[]) => void;
  checking: boolean;
  /** 改这块位置算这个软件的哪种成分 */
  onSetKind: (part: ArchivePart, kind: ClaimKind) => void;
  /** 这块位置不是这个软件的 */
  onDisown: (part: ArchivePart) => void;
  /** 撤掉指定的归属，回到自动认的 */
  onRestore: (part: ArchivePart) => void;
}

const ORIGIN_LABEL: Record<string, string> = { user: "你指定的", ai: "AI 认的" };

function placementText(p: Placement | null, g: Guess | null, boxes: Map<string, ArchiveBoxRow>): string {
  const name = (id: string) => boxes.get(id)?.name ?? "";
  if (p) {
    const by = p.source === "user" ? "你放的" : p.source === "ai" ? `AI 放的，把握 ${Math.round((p.confidence ?? 0) * 100)}%` : "按内置规则放的";
    return `在「${name(p.box)}」· ${by}`;
  }
  if (g?.source === "ai") return `在待整理 · AI 猜是${name(g.box)}（把握 ${Math.round((g.confidence ?? 0) * 100)}%）`;
  return "在待整理";
}

interface PartRowProps {
  part: ArchivePart;
  picked?: boolean;
  onPick?: () => void;
  onReveal: () => void;
  note?: string;
  /** 打开「改成分 / 不是它的」菜单（按钮的位置） */
  onMenu?: (part: ArchivePart, at: DOMRect) => void;
}

const ROW_BUTTON = "w-6 h-6 shrink-0 grid place-items-center rounded text-text-dim opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-hover hover:text-textMain";

function PartRow({ part, picked, onPick, onReveal, note, onMenu }: PartRowProps) {
  const origin = part.origin ? ORIGIN_LABEL[part.origin] : "";
  const tip = [
    part.path,
    part.contents_only ? "只清空里面的文件，文件夹本身保留" : "",
    origin ? `${origin}${part.reason ? `：${part.reason}` : ""}` : "",
    note ? `AI：${note}` : "",
  ].filter(Boolean).join("\n");
  return (
    <div className="group min-h-[34px] flex items-center gap-2 px-1.5 rounded hover:bg-hover" title={tip}>
      {onPick && <Checkbox checked={!!picked} onChange={onPick} label={`选中 ${part.path}`} />}
      <span className="w-5 h-[18px] shrink-0 grid place-items-center rounded-[3px] border border-[color:var(--fl-stroke-strong)] text-[11px] text-text-dim">
        {driveOf(part.path)}
      </span>
      {/* 从右往左截断：长路径保留末尾的目录名 */}
      <span className="flex-1 min-w-0 truncate text-xs" style={{ direction: "rtl", textAlign: "left" }}>
        <bdi>{shortenHome(part.path)}</bdi>
      </span>
      {origin && <span className="shrink-0 text-[11px] text-photon">{origin}</span>}
      {part.contents_only && <span className="shrink-0 text-[11px] text-text-faint">清空里面</span>}
      <button type="button" className={ROW_BUTTON} title="在文件夹中显示" aria-label="在文件夹中显示" onClick={onReveal}>
        <FolderOpen className="w-3.5 h-3.5" />
      </button>
      {onMenu && (
        <button
          type="button"
          className={ROW_BUTTON}
          title="改成分，或者说它不是这个软件的"
          aria-label="改成分"
          aria-haspopup="menu"
          onClick={(e) => onMenu(part, e.currentTarget.getBoundingClientRect())}
        >
          <MoreHorizontal className="w-3.5 h-3.5" />
        </button>
      )}
      <span className="shrink-0 text-xs text-text-dim">{formatSize(part.size)}</span>
    </div>
  );
}

/** 点开一项看详情：分布在哪几块盘、各位置是什么性质、哪些能清理 */
export function DetailDrawer({
  item, ix, sizes, icon, boxes, placement, guess, onClose, onMove, onClean, onReveal, onCheck, checking, onSetKind, onDisown, onRestore,
}: Props) {
  const boxMap = useMemo(() => new Map(boxes.map((b) => [b.id, b])), [boxes]);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<{ part: ArchivePart; x: number; y: number } | null>(null);
  useEffect(() => {
    setPicked(new Set());
    setMenu(null);
  }, [item.key]);
  useEffect(() => {
    // 菜单开着时 Esc 只关菜单
    const key = (e: KeyboardEvent) => { if (e.key === "Escape" && !menu) onClose(); };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose, menu]);
  const openMenu = item.type === "app" ? (part: ArchivePart, at: DOMRect) => setMenu({ part, x: at.right - 260, y: at.bottom + 4 }) : undefined;

  const size = itemSize(item, sizes);
  const parts: ArchivePart[] = item.type === "app"
    ? item.app.parts
    : [{ path: item.node.path, kind: "data", size, deletable: false, contents_only: false }];
  const byDrive = [...sizeByDrive({ parts }, driveOf)].sort();
  const grown = growthOf(ix, item.key);
  const sinceAt = [...ix.growthSince].find(([root]) => driveOf(root) === byDrive[0]?.[0])?.[1];
  // 名字像缓存 / 残留的位置分四种：AI 核对过能删的、还没核对的、AI 说别删的、在受保护位置里的
  const bySize = (a: ArchivePart, b: ArchivePart) => b.size - a.size;
  const junk = item.type === "app" ? parts.filter((p) => isCleanable(ix, p)).sort(bySize) : [];
  const unchecked = item.type === "app" ? parts.filter((p) => isUnchecked(ix, p)).sort(bySize) : [];
  const kept = item.type === "app" ? parts.filter((p) => p.deletable && JUNK_KINDS.includes(p.kind) && checkOf(ix, p)?.verdict === "keep").sort(bySize) : [];
  const locked = item.type === "app" ? parts.filter((p) => JUNK_KINDS.includes(p.kind) && !p.deletable).sort(bySize) : [];
  const allPicked = junk.length > 0 && junk.every((p) => picked.has(p.path));
  const pickedParts = junk.filter((p) => picked.has(p.path));
  const pickedSize = pickedParts.reduce((s, p) => s + p.size, 0);
  const toggle = (path: string) => setPicked((prev) => {
    const next = new Set(prev);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    return next;
  });
  const color = placement ? boxMap.get(placement.box)?.color ?? "#7A7574" : "#7A7574";
  const publisher = item.type === "app" ? item.app.publisher : "";

  return (
    <aside
      aria-label={`${item.name} 的详情`}
      className="absolute z-30 top-0 right-0 bottom-0 w-[392px] max-w-full flex flex-col rounded-lg bg-card border border-slate dialog-in"
      style={{ boxShadow: "var(--fl-shadow-dialog)" }}
    >
      <div className="flex gap-3 pl-4 pr-3 pt-4 pb-3">
        <ItemIcon item={item} icon={icon} color={color} size={48} />
        <div className="flex-1 min-w-0">
          <h2 className="text-lg font-semibold leading-6 truncate" title={item.type === "dir" ? item.node.path : item.name}>{item.name}</h2>
          <p className="mt-0.5 text-xs text-text-dim">{placementText(placement, guess, boxMap)}</p>
          {publisher && <p className="text-xs text-text-faint truncate">{publisher}</p>}
        </div>
        <button type="button" className="w-8 h-8 shrink-0 grid place-items-center rounded text-text-dim hover:bg-hover hover:text-textMain" aria-label="关闭" onClick={onClose}>
          <X className="w-4 h-4" />
        </button>
      </div>

      <div className="flex-1 min-h-0 overflow-auto px-4 pb-3">
        <div className="flex items-baseline justify-between">
          <b className="text-2xl font-semibold">{formatSize(size)}</b>
          <span className="text-xs text-text-dim">{byDrive.length > 1 ? `分散在 ${byDrive.length} 块盘` : byDrive.length === 1 ? `都在${driveLabel(byDrive[0][0])}` : ""}</span>
        </div>
        {grown !== undefined && (
          <p className="mt-0.5 text-xs text-text-dim">
            比上次扫描{sinceAt ? `（${timeAgo(sinceAt)}）` : ""}
            <span className={grown > 0 ? "text-caution" : "text-success"}>{grown > 0 ? "大了" : "小了"} {formatSize(Math.abs(grown))}</span>
          </p>
        )}
        {byDrive.length > 0 && (
          <>
            <div className="flex gap-0.5 h-1.5 mt-2 mb-1.5 rounded-full overflow-hidden" aria-hidden="true">
              {byDrive.map(([d, s], i) => <i key={d} className="block h-full" style={{ flexBasis: `${(s / Math.max(1, size)) * 100}%`, background: DRIVE_COLORS[i % DRIVE_COLORS.length] }} />)}
            </div>
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-text-dim">
              {byDrive.map(([d, s], i) => (
                <span key={d} className="inline-flex items-center gap-1.5">
                  <i className="w-2 h-2 rounded-sm" style={{ background: DRIVE_COLORS[i % DRIVE_COLORS.length] }} />
                  {driveLabel(d)} {formatSize(s)}
                </span>
              ))}
            </div>
          </>
        )}

        <label className="mt-4 flex items-center gap-3 text-xs text-text-dim">
          放在
          <span className="flex-1">
            <Select value={placement?.box ?? ""} onChange={(e) => onMove(e.target.value || null)} aria-label="放在哪个箱子">
              <option value="">待整理</option>
              {CATEGORIES.map((c) => (
                <optgroup key={c} label={CATEGORY_META[c].name}>
                  {boxes.filter((b) => b.category === c).map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
                </optgroup>
              ))}
            </Select>
          </span>
        </label>

        {item.type === "app" && item.app.uninstalled && (
          <div className="mt-4 flex gap-2 p-3 rounded-md border border-slate bg-abyss text-xs leading-5">
            <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5 text-caution" />
            <span>程序目录里没有找到可执行文件，系统里也没有它的安装记录，像是卸载后留下的。确定不再需要再清理。</span>
          </div>
        )}

        {junk.length > 0 && (
          <section className="mt-5">
            <div className="flex items-baseline gap-2 mb-1">
              <h3 className="text-[13px] font-semibold">可以清理</h3>
              <span className="text-[11px] text-success">AI 核对过</span>
              <span className="text-xs text-text-faint">{formatSize(junk.reduce((s, p) => s + p.size, 0))}</span>
              <button type="button" className="ml-auto text-xs text-photon hover:underline" onClick={() => setPicked(allPicked ? new Set() : new Set(junk.map((p) => p.path)))}>
                {allPicked ? "全不选" : "全选"}
              </button>
            </div>
            <p className="mb-1.5 text-xs text-text-faint">{junk.some((p) => p.kind === "residue") ? "残留是卸载后留下的程序文件；缓存删了，软件会按需重新生成。" : "缓存删了，软件会按需重新生成。"}</p>
            {junk.map((p) => <PartRow key={p.path} part={p} picked={picked.has(p.path)} onPick={() => toggle(p.path)} onReveal={() => onReveal(p.path)} onMenu={openMenu} note={checkOf(ix, p)?.reason} />)}
          </section>
        )}

        {unchecked.length > 0 && (
          <section className="mt-5">
            <div className="flex items-baseline gap-2 mb-1">
              <h3 className="text-[13px] font-semibold">待核对</h3>
              <span className="text-xs text-text-faint">{formatSize(unchecked.reduce((s, p) => s + p.size, 0))}</span>
              <button
                type="button"
                className="ml-auto inline-flex items-center gap-1 text-xs text-ai hover:underline disabled:opacity-60 disabled:no-underline"
                disabled={checking}
                onClick={() => onCheck(unchecked)}
              >
                {checking ? <Loader2 className="w-3 h-3 animate-spin" /> : <ShieldCheck className="w-3 h-3" />}
                {checking ? "AI 正在核对" : "让 AI 核对"}
              </button>
            </div>
            <p className="mb-1.5 text-xs text-text-faint">名字像缓存或残留，但不一定真能删。AI 核对过、确认删了不丢东西的才能勾选。</p>
            {unchecked.map((p) => <PartRow key={p.path} part={p} onReveal={() => onReveal(p.path)} onMenu={openMenu} note={checkOf(ix, p)?.reason} />)}
          </section>
        )}

        {kept.length > 0 && (
          <section className="mt-5">
            <div className="flex items-baseline gap-2 mb-1">
              <h3 className="text-[13px] font-semibold">AI 建议保留</h3>
              <span className="text-xs text-text-faint">{formatSize(kept.reduce((s, p) => s + p.size, 0))}</span>
            </div>
            <p className="mb-1.5 text-xs text-text-faint">名字像缓存，但 AI 认为里面有不该删的东西。悬停能看到理由。</p>
            {kept.map((p) => <PartRow key={p.path} part={p} onReveal={() => onReveal(p.path)} onMenu={openMenu} note={checkOf(ix, p)?.reason} />)}
          </section>
        )}

        {locked.length > 0 && (
          <section className="mt-5">
            <div className="flex items-baseline gap-2 mb-1">
              <h3 className="text-[13px] font-semibold">在受保护的位置</h3>
              <span className="text-xs text-text-faint">{formatSize(locked.reduce((s, p) => s + p.size, 0))}</span>
            </div>
            <p className="mb-1.5 text-xs text-text-faint">这些缓存或残留在 Program Files、Windows 这类系统目录里，Disky 不会删。可以用软件自己的清理功能或卸载程序处理。</p>
            {locked.map((p) => <PartRow key={p.path} part={p} onReveal={() => onReveal(p.path)} onMenu={openMenu} />)}
          </section>
        )}

        {item.type === "app" && GROUPS.map((g) => {
          const list = parts.filter((p) => g.kinds.includes(p.kind)).sort((a, b) => b.size - a.size);
          if (list.length === 0) return null;
          return (
            <section key={g.title} className="mt-5">
              <div className="flex items-baseline gap-2 mb-1">
                <h3 className="text-[13px] font-semibold">{g.title}</h3>
                <span className="text-xs text-text-faint">{formatSize(list.reduce((s, p) => s + p.size, 0))}</span>
              </div>
              <p className="mb-1.5 text-xs text-text-faint">{g.desc}</p>
              {list.map((p) => <PartRow key={p.path} part={p} onReveal={() => onReveal(p.path)} onMenu={openMenu} />)}
            </section>
          );
        })}

        {item.type === "dir" && (
          <>
            <section className="mt-5">
              <h3 className="mb-1 text-[13px] font-semibold">位置</h3>
              <PartRow part={parts[0]} onReveal={() => onReveal(item.node.path)} />
            </section>
            {visibleKids(ix, item.node, sizes).length > 0 && (
              <section className="mt-5">
                <h3 className="text-[13px] font-semibold">里面最大的几项</h3>
                <p className="mb-1.5 text-xs text-text-faint">也可以在左边展开它，只把其中一项拖进箱子。</p>
                {visibleKids(ix, item.node, sizes).slice(0, 12).map((k) => (
                  <PartRow key={k.key} part={{ path: k.path, kind: "data", size: sizes.get(k.key) ?? 0, deletable: false, contents_only: false }} onReveal={() => onReveal(k.path)} />
                ))}
              </section>
            )}
          </>
        )}
      </div>

      <footer className="shrink-0 flex items-center justify-end gap-2 px-4 py-3 border-t border-slate bg-fiber rounded-b-lg">
        <span className="mr-auto text-xs text-text-faint">
          {item.type === "dir"
            ? "是哪个软件的，就把它拖到那个软件的图标上"
            : junk.length > 0
              ? "会移到回收站，删除前还会再确认"
              : unchecked.length > 0
                ? "先让 AI 核对，确认能删的才能勾选"
                : locked.length > 0
                  ? "能清理的都在受保护的位置"
                  : "没有可清理的东西"}
        </span>
        {junk.length > 0 && (
          <Button variant="primary" disabled={pickedParts.length === 0} onClick={() => onClean(pickedParts)}>
            清理{pickedSize > 0 ? ` ${formatSize(pickedSize)}` : "选中的"}
          </Button>
        )}
      </footer>

      {menu && (
        <ClaimMenu
          x={menu.x}
          y={menu.y}
          title={`「${basename(menu.part.path)}」算「${item.name}」的`}
          current={menu.part.kind}
          extra={[
            { label: "不是这个软件的", run: () => { setMenu(null); onDisown(menu.part); } },
            ...(menu.part.origin ? [{ label: "恢复成自动认的", run: () => { setMenu(null); onRestore(menu.part); } }] : []),
          ]}
          onPick={(kind) => {
            setMenu(null);
            if (kind !== menu.part.kind) onSetKind(menu.part, kind);
          }}
          onClose={() => setMenu(null)}
        />
      )}
    </aside>
  );
}
