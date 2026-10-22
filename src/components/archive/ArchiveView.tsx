import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { HardDrive, Loader2, Play } from "lucide-react";
import type { ArchiveCategory, ArchivePart, ClaimKind, DriveInfo } from "../../types";
import type { ArchiveSession } from "../../hooks/useArchive";
import { useAppIcons } from "../../hooks/useAppIcons";
import { formatSize } from "../../lib/format";
import { basename, driveLabel, driveOf } from "../../lib/path";
import { showInExplorer } from "../../lib/invoke";
import {
  AI_MIN_CONFIDENCE, CATEGORIES, computeStats, folderSizes, growthOf, guessOf, indexModel, itemJunk, itemOf, placementOf, type Item,
} from "../../lib/archiveModel";
import { MAX_TILES, reconcileLayout, sameLayout, type TileLayout } from "../../lib/tileLayout";
import { cn } from "../../lib/utils";
import { Button } from "../ui/Button";
import { TileGrid } from "./TileGrid";
import { BoxPane, type BoxView } from "./BoxPane";
import { InboxPane } from "./InboxPane";
import { DetailDrawer } from "./DetailDrawer";
import { ArchiveSummary } from "./ArchiveSummary";
import { AcceptGuessesDialog } from "./AcceptGuessesDialog";
import { ClaimMenu, claimKindName } from "./ClaimMenu";
import { ItemIcon } from "./ItemViews";
import { CATEGORY_META } from "./categories";
import { AttributeConfirmDialog } from "./AttributeConfirmDialog";
import { NotesDialog } from "./NotesDialog";
import { ResetDialog } from "./ResetDialog";
import { appCandidates, attributeCandidates, checkCandidates, folderCandidates, folderGuesses, type AttributeCandidate } from "./candidates";
import { INBOX_KEY, TAB_KEY, VIEWS_KEY, layoutKey, loadLayouts, readJson, removeKey, writeJson } from "./layouts";

interface Props {
  archive: ArchiveSession;
  drives: DriveInfo[];
  scanning: boolean;
  onScanDrive: (mount: string) => void;
  /** empty：只清空里面、保留文件夹本身的路径 */
  onCleanup: (items: Map<string, number>, empty: Set<string>) => void;
  onOpenAiSettings: () => void;
  /** 打开侧栏，让整理助手开场（看一遍现状再开始问） */
  onStartAssistant: () => void;
}

const NEW_BOX_COLORS = ["#CA5010", "#4F6BED", "#00B7C3", "#498205", "#B4009E", "#986F0B", "#E74856", "#0099BC"];
const DRAG_PX = 5;
/** 拖动的落点是某个软件（文件夹归给它），和箱子 id 区分开 */
const CLAIM_PREFIX = "claim:";
const INBOX_MIN = 260, INBOX_MAX = 560, INBOX_DEFAULT = 330;

/** 拖动期间统一光标、禁止选中文字 */
function lockPointer(cursor: string): () => void {
  document.body.style.setProperty("--drag-cursor", cursor);
  document.body.classList.add("pointer-locked");
  return () => {
    document.body.classList.remove("pointer-locked");
    document.body.style.removeProperty("--drag-cursor");
  };
}

interface Toast {
  text: string;
  action?: { label: string; run: () => void };
}

const isCategory = (v: unknown): v is ArchiveCategory => typeof v === "string" && (CATEGORIES as readonly string[]).includes(v);

/** 档案馆：三个页签（软件 / 个人文件 / 插件与开发环境），各自左边待整理、右边箱子；图标拖进箱子就是整理 */
export function ArchiveView({ archive, drives, scanning, onScanDrive, onCleanup, onOpenAiSettings, onStartAssistant }: Props) {
  const { model } = archive;
  const ix = useMemo(() => (model ? indexModel(model) : null), [model]);
  const sizes = useMemo(() => (ix ? folderSizes(ix) : new Map<string, number>()), [ix]);
  const stats = useMemo(() => (ix && model ? computeStats(ix, sizes, model.drives, driveOf) : null), [ix, sizes, model]);
  const iconSources = useMemo(() => (model ? model.apps.map((a) => a.icon) : []), [model]);
  const icons = useAppIcons(iconSources);
  const boxes = model?.boxes ?? [];
  const boxMap = useMemo(() => new Map(boxes.map((b) => [b.id, b])), [boxes]);
  const place = useCallback((key: string) => (ix ? placementOf(ix, key) : null), [ix]);
  const junkOf = useCallback((item: Item) => (ix ? itemJunk(ix, item) : 0), [ix]);
  const growth = useCallback((key: string) => (ix ? growthOf(ix, key) : undefined), [ix]);

  const [tab, setTab] = useState<ArchiveCategory>(() => {
    const t = readJson<unknown>(TAB_KEY, "software");
    return isCategory(t) ? t : "software";
  });
  const switchTab = (t: ArchiveCategory) => {
    setTab(t);
    writeJson(TAB_KEY, t);
    // 详情是上一个页签里的东西，换页签就收起
    setOpenKey(null);
  };
  const tabBoxes = useMemo(() => boxes.filter((b) => b.category === tab), [boxes, tab]);

  // 布局：每个页签各存一份，和现有箱子对齐
  const [layouts, setLayouts] = useState(loadLayouts);
  const tabBoxIds = useMemo(() => tabBoxes.map((b) => b.id), [tabBoxes]);
  const layout = useMemo(() => reconcileLayout(layouts[tab], tabBoxIds), [layouts, tab, tabBoxIds]);
  useEffect(() => {
    // 数据还没到时箱子列表是空的，这时的布局不能存，否则会把用户摆好的布局冲掉
    if (!model || layout.length === 0) return;
    const t = window.setTimeout(() => writeJson(layoutKey(tab), layout), 300);
    return () => window.clearTimeout(t);
  }, [layout, model, tab]);
  const setLayout = (next: TileLayout<string>) =>
    setLayouts((prev) => (sameLayout(prev[tab], next) ? prev : { ...prev, [tab]: next }));

  const [views, setViews] = useState<Record<string, BoxView>>(() => readJson(VIEWS_KEY, {}));
  const [inboxW, setInboxW] = useState(() => {
    const w = readJson<number>(INBOX_KEY, INBOX_DEFAULT);
    return Math.min(INBOX_MAX, Math.max(INBOX_MIN, Number.isFinite(w) ? w : INBOX_DEFAULT));
  });
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    // 各盘的根默认展开
    if (ix) setExpanded((prev) => (prev.size ? prev : new Set(ix.roots.map((r) => r.key))));
  }, [ix]);

  const [openKey, setOpenKey] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const toastTimer = useRef(0);
  const say = useCallback((t: Toast) => {
    setToast(t);
    window.clearTimeout(toastTimer.current);
    // 带操作的、长的（AI 的总结）多留一会儿
    toastTimer.current = window.setTimeout(() => setToast(null), t.action || t.text.length > 60 ? 8000 : 3500);
  }, []);
  useEffect(() => () => window.clearTimeout(toastTimer.current), []);

  // ---- 拖动图标 / 文件夹进箱子 ----
  const [drag, setDrag] = useState<{ key: string; target: string | null } | null>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const ghostPos = useRef({ x: -9999, y: -9999 });
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);

  const dropOn = useCallback((key: string, target: string) => {
    if (!ix) return;
    const box = target === "inbox" ? null : target;
    if ((place(key)?.box ?? null) === box) return;
    const name = itemOf(ix, key)?.name ?? "";
    const row = box ? boxMap.get(box) : undefined;
    const elsewhere = row && row.category !== tab ? `（在「${CATEGORY_META[row.category].name}」页签）` : "";
    archive
      .place([key], box)
      .then(() => say({ text: row ? `「${name}」放进了「${row.name}」${elsewhere}` : `「${name}」放回了待整理` }))
      .catch((e) => say({ text: `没能保存：${String(e)}` }));
  }, [ix, place, boxMap, tab, archive, say]);

  // 文件夹拖到软件上松手：先选它算这个软件的哪种成分
  const [claimMenu, setClaimMenu] = useState<{ dirKey: string; appKey: string; x: number; y: number } | null>(null);
  useEffect(() => {
    // 重新识别后文件夹或软件不在了，菜单就收起，免得以后在旧位置冒出来
    if (claimMenu && ix && (!ix.folders.has(claimMenu.dirKey) || !ix.apps.has(claimMenu.appKey))) setClaimMenu(null);
  }, [ix, claimMenu]);

  const assignTo = useCallback((path: string, name: string, appKey: string, appName: string, kind: ClaimKind) => {
    setClaimMenu(null);
    say({ text: `正在把「${name}」归给「${appName}」…` });
    archive
      .assign(path, appKey, kind)
      .then(() => say({
        text: `「${name}」归给了「${appName}」，算${claimKindName(kind)}`,
        action: { label: "撤销", run: () => { archive.unassign(path).catch((e) => say({ text: `没能撤销：${String(e)}` })); } },
      }))
      .catch((e) => say({ text: `没能保存：${String(e)}` }));
  }, [archive, say]);

  const onPointerDown = useCallback((e: ReactPointerEvent) => {
    if (e.button !== 0) return;
    const el = e.target as HTMLElement;
    if (el.closest("button, input, select, a, [role=checkbox], [role=menu]")) return;
    const src = el.closest<HTMLElement>("[data-item]");
    const found = src?.dataset.item;
    if (!found) return;
    const key: string = found;
    e.preventDefault();
    stopDrag.current?.();
    const sx = e.clientX, sy = e.clientY;
    let active = false;
    let shown = false;
    let target: string | null = null;
    let last = { x: sx, y: sy };
    let unlock: (() => void) | null = null;
    // 文件夹还能拖到软件的图标上，归给那个软件
    const isDir = key.startsWith("dir:");
    const zoneAt = (x: number, y: number) => {
      const under = document.elementFromPoint(x, y);
      const app = isDir ? under?.closest<HTMLElement>("[data-item^='app:']")?.dataset.item : undefined;
      if (app) return CLAIM_PREFIX + app;
      return under?.closest<HTMLElement>("[data-drop]")?.dataset.drop ?? null;
    };
    const move = (ev: PointerEvent) => {
      if (ev.buttons === 0) return finish(true);
      if (!active) {
        if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < DRAG_PX) return;
        active = true;
        unlock = lockPointer("grabbing");
      }
      last = { x: ev.clientX, y: ev.clientY };
      // 跟手的标签直接改 DOM，只有落点变化时才重新渲染
      ghostPos.current = { x: ev.clientX + 14, y: ev.clientY + 12 };
      if (ghostRef.current) ghostRef.current.style.transform = `translate(${ghostPos.current.x}px, ${ghostPos.current.y}px)`;
      const next = zoneAt(ev.clientX, ev.clientY);
      if (!shown || next !== target) {
        shown = true;
        target = next;
        setDrag({ key, target });
      }
    };
    const up = () => finish(false);
    const cancel = () => finish(true);
    const esc = (ev: KeyboardEvent) => { if (ev.key === "Escape") finish(true); };
    function finish(cancelled: boolean) {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", esc);
      unlock?.();
      stopDrag.current = null;
      setDrag(null);
      if (cancelled) return;
      if (!active) setOpenKey((k) => (k === key ? null : key));
      else if (target?.startsWith(CLAIM_PREFIX)) setClaimMenu({ dirKey: key, appKey: target.slice(CLAIM_PREFIX.length), ...last });
      else if (target) dropOn(key, target);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", esc);
    stopDrag.current = () => finish(true);
  }, [dropOn]);

  const startInboxResize = (e: ReactPointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const start = e.clientX, base = inboxW;
    const unlock = lockPointer("col-resize");
    let last = base;
    const move = (ev: PointerEvent) => {
      if (ev.buttons === 0) return up();
      last = Math.min(INBOX_MAX, Math.max(INBOX_MIN, base + ev.clientX - start));
      setInboxW(last);
    };
    function up() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      window.removeEventListener("blur", up);
      unlock();
      writeJson(INBOX_KEY, last);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    window.addEventListener("blur", up);
  };

  // ---- 箱子 ----
  const addBox = useCallback(async () => {
    const used = new Set(tabBoxes.map((b) => b.color));
    const color = NEW_BOX_COLORS.find((c) => !used.has(c)) ?? NEW_BOX_COLORS[tabBoxes.length % NEW_BOX_COLORS.length];
    try {
      const row = await archive.createBox("新箱子", color, tab);
      setRenaming(row.id);
    } catch (e) {
      say({ text: `没能新建箱子：${String(e)}` });
    }
  }, [archive, tabBoxes, tab, say]);

  const renameBox = useCallback((id: string, name: string | null) => {
    setRenaming(null);
    if (name && name !== boxMap.get(id)?.name) archive.renameBox(id, name).catch((e) => say({ text: `没能改名：${String(e)}` }));
  }, [archive, boxMap, say]);

  const deleteBox = useCallback((id: string) => {
    const box = boxMap.get(id);
    const n = stats?.boxes.get(id)?.items.length ?? 0;
    archive.deleteBox(id)
      .then(() => say({ text: `删掉了「${box?.name ?? ""}」${n > 0 ? `，里面的 ${n} 项回到了待整理` : ""}` }))
      .catch((e) => say({ text: `没能删除箱子：${String(e)}` }));
  }, [archive, boxMap, stats, say]);

  // ---- AI 归类（当前页签）----
  const classify = useCallback(async () => {
    if (!ix || !stats) return;
    const items = tab === "personal" ? folderCandidates(ix, sizes) : appCandidates(stats.inbox[tab]);
    if (items.length === 0) {
      say({ text: "待整理里没有需要 AI 归类的东西了" });
      return;
    }
    try {
      const rows = await archive.classify(items, tab);
      const placed = rows.filter((r) => r.confidence >= AI_MIN_CONFIDENCE).length;
      say({ text: `AI 看了 ${items.length} 项：放好了 ${placed} 项${rows.length - placed > 0 ? `，拿不准的 ${rows.length - placed} 项留在待整理，带着它的猜测` : ""}` });
    } catch (e) {
      say({ text: `AI 没能归类：${String(e)}`, action: { label: "打开 AI 设置", run: onOpenAiSettings } });
    }
  }, [ix, stats, sizes, tab, archive, say, onOpenAiSettings]);

  // ---- AI 查没认出的文件夹是哪个软件的：先过目要交出去的文件夹 ----
  const [attributeList, setAttributeList] = useState<AttributeCandidate[] | null>(null);
  const reviewAttribute = useCallback(() => {
    if (!ix) return;
    const list = attributeCandidates(ix, sizes);
    if (list.length === 0) say({ text: "没有需要 AI 查的大文件夹了" });
    else setAttributeList(list);
  }, [ix, sizes, say]);

  const attribute = useCallback(async (paths: string[]) => {
    setAttributeList(null);
    try {
      const { records, summary } = await archive.attribute(paths);
      const sure = records.filter((r) => r.confidence >= AI_MIN_CONFIDENCE).length;
      const head = `AI 查了 ${paths.length} 个文件夹，认出 ${sure} 处${records.length - sure > 0 ? `（另有 ${records.length - sure} 处拿不准，没采用）` : ""}`;
      say({ text: summary ? `${head}。${summary}` : head });
    } catch (e) {
      say({ text: `AI 没能查完：${String(e)}`, action: { label: "打开 AI 设置", run: onOpenAiSettings } });
    }
  }, [archive, say, onOpenAiSettings]);

  // ---- AI 核对缓存 ----
  const runCheck = useCallback(async (items: ReturnType<typeof checkCandidates>) => {
    if (items.length === 0) return;
    try {
      const rows = await archive.check(items);
      const safe = rows.filter((r) => r.verdict === "safe");
      const freed = items.filter((it) => safe.some((s) => s.path.toLowerCase() === it.path.toLowerCase())).reduce((s, it) => s + it.size, 0);
      const keep = rows.filter((r) => r.verdict === "keep").length;
      say({ text: `AI 核对了 ${rows.length} 处：${safe.length} 处可以放心删（${formatSize(freed)}）${keep > 0 ? `，${keep} 处建议保留` : ""}` });
    } catch (e) {
      say({ text: `AI 没能核对：${String(e)}`, action: { label: "打开 AI 设置", run: onOpenAiSettings } });
    }
  }, [archive, say, onOpenAiSettings]);

  const guessable = useMemo(() => {
    if (!ix || !stats) return [] as { key: string; box: string }[];
    if (tab === "personal") return folderGuesses(ix, sizes);
    return stats.inbox[tab].flatMap((a) => {
      const g = guessOf(ix, a.key);
      return g ? [{ key: a.key, box: g.box }] : [];
    });
  }, [ix, stats, sizes, tab]);

  // 「都按猜的放」先弹确认框过目，确认后只放勾着的
  const [reviewing, setReviewing] = useState(false);
  const acceptGuesses = useCallback((picked: { key: string; box: string }[]) => {
    setReviewing(false);
    const byBox = new Map<string, string[]>();
    for (const g of picked) byBox.set(g.box, [...(byBox.get(g.box) ?? []), g.key]);
    Promise.all([...byBox].map(([box, keys]) => archive.place(keys, box)))
      .then(() => say({ text: `按猜测放好了 ${picked.length} 项，不对的话再拖出来` }))
      .catch((e) => say({ text: `没能保存：${String(e)}` }));
  }, [archive, say]);

  // ---- 重新开始整理、AI 记住的 ----
  const [resetOpen, setResetOpen] = useState(false);
  const [notesOpen, setNotesOpen] = useState(false);
  const resetAll = useCallback(async () => {
    const backup = await archive.reset();
    // 本机记的布局、视图、页签也回到默认
    for (const c of CATEGORIES) removeKey(layoutKey(c));
    [VIEWS_KEY, TAB_KEY].forEach(removeKey);
    setLayouts(loadLayouts());
    setViews({});
    setOpenKey(null);
    setResetOpen(false);
    say({ text: `已经清空，原来的数据备份在 ${backup}` });
  }, [archive, say]);

  const clean = useCallback((parts: ArchivePart[]) => {
    onCleanup(new Map(parts.map((p) => [p.path, p.size])), new Set(parts.filter((p) => p.contents_only).map((p) => p.path)));
  }, [onCleanup]);

  // ---- 空状态 ----
  if (!model || !ix || !stats) {
    return archive.error ? (
      <div className="h-full grid place-items-center text-center">
        <div>
          <div className="text-sm">档案馆没能打开</div>
          <div className="mt-1 text-xs text-text-dim">{archive.error}</div>
          <Button className="mt-3" onClick={() => archive.reload()}>重试</Button>
        </div>
      </div>
    ) : (
      <div className="h-full flex items-center justify-center gap-2 text-sm text-text-dim">
        <Loader2 className="w-4 h-4 animate-spin" />正在整理档案
      </div>
    );
  }
  if (model.drives.length === 0) {
    return (
      <div className="h-full grid place-items-center p-6">
        <div className="max-w-[560px] text-center">
          <h1 className="text-xl font-semibold">档案馆还是空的</h1>
          <p className="mt-2 text-[13px] text-text-dim">
            扫描一块整盘后，这里会把盘上的东西分成软件、个人文件、插件与开发环境三类，再装进箱子，标出每一项散在哪些位置、有哪些能清理。扫描多块盘，同一个软件在各盘上的东西会合在一起。
          </p>
          <div className="mt-5 flex flex-wrap justify-center gap-2">
            {drives.map((d) => (
              <Button key={d.mount_point} disabled={scanning} onClick={() => onScanDrive(d.mount_point)}>
                <HardDrive className="w-4 h-4" />扫描{driveLabel(driveOf(d.mount_point))}<Play className="w-3 h-3" />
              </Button>
            ))}
          </div>
        </div>
      </div>
    );
  }

  const openItem = openKey ? itemOf(ix, openKey) : null;
  const dragItem = drag ? itemOf(ix, drag.key) : null;
  const dragColor = dragItem ? boxMap.get(place(dragItem.key)?.box ?? "")?.color ?? "#7A7574" : "#7A7574";
  const claimKey = drag?.target?.startsWith(CLAIM_PREFIX) ? drag.target.slice(CLAIM_PREFIX.length) : null;
  const claimDir = claimMenu ? ix.folders.get(claimMenu.dirKey) : undefined;
  const claimApp = claimMenu ? ix.apps.get(claimMenu.appKey) : undefined;
  const inboxCount = (c: ArchiveCategory) => (c === "personal" ? stats.looseCount : stats.inbox[c].length);

  return (
    <div className="h-full flex flex-col gap-2 min-h-0" onPointerDown={onPointerDown}>
      <ArchiveSummary
        stats={stats}
        drives={drives}
        scanning={scanning}
        onScanDrive={onScanDrive}
        canAddBox={tabBoxes.length < MAX_TILES}
        onAddBox={addBox}
        onCheck={() => runCheck(checkCandidates(stats.pending))}
        checking={archive.checking}
        template={archive.mode === "template"}
        onReview={onStartAssistant}
        notesCount={archive.notes.length}
        onShowNotes={() => setNotesOpen(true)}
        onReset={() => setResetOpen(true)}
      />

      <nav role="tablist" aria-label="门类" className="shrink-0 flex items-center gap-1 px-1">
        {CATEGORIES.map((c) => {
          const active = tab === c;
          const pending = inboxCount(c);
          return (
            <button
              key={c}
              type="button"
              role="tab"
              aria-selected={active}
              title={CATEGORY_META[c].hint}
              className={cn(
                "relative h-9 px-3 inline-flex items-center gap-2 rounded text-[13px] transition-colors hover:bg-hover",
                active ? "text-textMain font-semibold" : "text-text-dim hover:text-textMain"
              )}
              onClick={() => switchTab(c)}
            >
              <i className="w-2 h-2 rounded-sm" style={{ background: CATEGORY_META[c].color }} />
              {CATEGORY_META[c].name}
              <span className="text-xs font-normal text-text-faint">{formatSize(stats.byCategory[c])}{pending > 0 ? ` · ${pending} 项待整理` : ""}</span>
              {active && <span className="absolute left-3 right-3 bottom-0.5 h-[3px] rounded-full bg-photon" />}
            </button>
          );
        })}
      </nav>

      <div className="relative flex-1 min-h-0 flex">
        <InboxPane
          category={tab}
          ix={ix}
          stats={stats}
          sizes={sizes}
          icons={icons}
          boxes={boxMap}
          placementOf={place}
          junkOf={junkOf}
          width={inboxW}
          dropActive={drag?.target === "inbox" && !!place(drag.key)}
          openKey={openKey}
          draggingKey={drag?.key ?? null}
          claimKey={claimKey}
          expanded={expanded}
          onToggle={(k) => setExpanded((prev) => {
            const next = new Set(prev);
            if (next.has(k)) next.delete(k);
            else next.add(k);
            return next;
          })}
          onAccept={(key, box) => dropOn(key, box)}
          onAcceptAll={() => setReviewing(true)}
          guessable={guessable.length}
          onClassify={classify}
          classifying={archive.classifying}
          onAttribute={reviewAttribute}
          attributing={archive.attributing}
          agentStatus={archive.agentStatus}
        />
        <div
          role="separator"
          aria-orientation="vertical"
          title="拖动调整宽度"
          className="group w-2 shrink-0 relative cursor-col-resize"
          onPointerDown={startInboxResize}
        >
          <span className="absolute inset-y-3 left-[3px] w-0.5 rounded-full bg-photon opacity-0 group-hover:opacity-100 transition-opacity" />
        </div>
        <div className="flex-1 min-w-0 flex">
          <TileGrid
            key={tab}
            layout={layout}
            onChange={setLayout}
            titleOf={(id) => boxMap.get(id)?.name ?? ""}
            renderTile={(id, handle, tileDragging) => {
              const box = boxMap.get(id);
              const content = stats.boxes.get(id);
              if (!box || !content) return null;
              return (
                <BoxPane
                  box={box}
                  content={content}
                  sizes={sizes}
                  icons={icons}
                  boxes={boxMap}
                  placementOf={place}
                  junkOf={junkOf}
                  growthOf={growth}
                  view={views[id] ?? "icons"}
                  onToggleView={() => setViews((v) => {
                    const next = { ...v, [id]: v[id] === "list" ? "icons" as const : "list" as const };
                    writeJson(VIEWS_KEY, next);
                    return next;
                  })}
                  handle={handle}
                  tileDragging={tileDragging}
                  dropActive={drag?.target === id && place(drag.key)?.box !== id}
                  openKey={openKey}
                  draggingKey={drag?.key ?? null}
                  claimKey={claimKey}
                  renaming={renaming === id}
                  onStartRename={() => setRenaming(id)}
                  onRename={(name) => renameBox(id, name)}
                  onDelete={() => deleteBox(id)}
                />
              );
            }}
          />
        </div>

        {openItem && (
          <DetailDrawer
            item={openItem}
            ix={ix}
            sizes={sizes}
            icon={openItem.type === "app" && openItem.app.icon ? icons.get(openItem.app.icon) : null}
            boxes={boxes}
            placement={place(openItem.key)}
            guess={guessOf(ix, openItem.key)}
            onClose={() => setOpenKey(null)}
            onMove={(box) => dropOn(openItem.key, box ?? "inbox")}
            onClean={clean}
            onReveal={(p) => { showInExplorer(p).catch((e) => say({ text: `打不开：${String(e)}` })); }}
            onCheck={(parts) => openItem.type === "app" && runCheck(parts.map((p) => ({ path: p.path, owner: openItem.app.name, kind: p.kind, size: p.size })))}
            checking={archive.checking}
            onSetKind={(part, kind) => openItem.type === "app" && assignTo(part.path, basename(part.path), openItem.app.key, openItem.app.name, kind)}
            onDisown={(part) => {
              if (openItem.type !== "app") return;
              const name = basename(part.path);
              archive
                .assign(part.path, null, "data")
                .then(() => say({
                  text: `「${name}」不再算「${openItem.app.name}」的`,
                  action: { label: "撤销", run: () => { archive.unassign(part.path).catch((e) => say({ text: `没能撤销：${String(e)}` })); } },
                }))
                .catch((e) => say({ text: `没能保存：${String(e)}` }));
            }}
            onRestore={(part) => {
              archive.unassign(part.path)
                .then(() => say({ text: `「${basename(part.path)}」恢复成自动认的归属` }))
                .catch((e) => say({ text: `没能保存：${String(e)}` }));
            }}
          />
        )}

        {claimMenu && claimDir && claimApp && (
          <ClaimMenu
            x={claimMenu.x}
            y={claimMenu.y}
            title={`「${claimDir.name}」算「${claimApp.name}」的`}
            onPick={(kind) => assignTo(claimDir.path, claimDir.name, claimApp.key, claimApp.name, kind)}
            onClose={() => setClaimMenu(null)}
          />
        )}
      </div>

      {drag && dragItem && (
        <div
          ref={ghostRef}
          className="fixed left-0 top-0 z-[130] flex items-center gap-2 pl-1.5 pr-3 py-1.5 rounded-md bg-card border border-slate text-xs whitespace-nowrap pointer-events-none"
          style={{ boxShadow: "var(--fl-shadow)", transform: `translate(${ghostPos.current.x}px, ${ghostPos.current.y}px)` }}
        >
          <ItemIcon item={dragItem} icon={dragItem.type === "app" && dragItem.app.icon ? icons.get(dragItem.app.icon) : null} color={dragColor} size={28} />
          {dragItem.name}
        </div>
      )}

      {resetOpen && <ResetDialog onConfirm={resetAll} onClose={() => setResetOpen(false)} />}
      {notesOpen && (
        <NotesDialog
          notes={archive.notes}
          onDelete={(id) => { archive.deleteNote(id).catch((e) => say({ text: `没能删掉：${String(e)}` })); }}
          onClose={() => setNotesOpen(false)}
        />
      )}

      {attributeList && (
        <AttributeConfirmDialog candidates={attributeList} onConfirm={attribute} onClose={() => setAttributeList(null)} />
      )}

      {reviewing && ix && (
        <AcceptGuessesDialog
          guesses={guessable}
          ix={ix}
          sizes={sizes}
          icons={icons}
          boxes={boxMap}
          onConfirm={acceptGuesses}
          onClose={() => setReviewing(false)}
        />
      )}

      {toast && (
        <div role="status" className="fixed left-1/2 bottom-10 -translate-x-1/2 z-[140] max-w-[560px] flex items-center gap-3 px-4 py-2.5 rounded-md bg-card border border-slate text-[13px]" style={{ boxShadow: "var(--fl-shadow)" }}>
          <span>{toast.text}</span>
          {toast.action && (
            <button type="button" className="shrink-0 text-photon hover:underline" onClick={() => { toast.action?.run(); setToast(null); }}>{toast.action.label}</button>
          )}
        </div>
      )}
    </div>
  );
}
