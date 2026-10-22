import { useEffect, useRef, useState } from "react";
import { LayoutGrid, List, MoreHorizontal } from "lucide-react";
import type { ArchiveBoxRow } from "../../types";
import { formatSize } from "../../lib/format";
import { itemSize, type BoxContent, type Item, type Placement } from "../../lib/archiveModel";
import { cn } from "../../lib/utils";
import type { TileHandle } from "./TileGrid";
import { ItemCell, ItemRow, itemTooltip, JUNK_SHOWN } from "./ItemViews";

export type BoxView = "icons" | "list";

interface Props {
  box: ArchiveBoxRow;
  content: BoxContent;
  sizes: Map<string, number>;
  icons: ReadonlyMap<string, string | null>;
  boxes: Map<string, ArchiveBoxRow>;
  placementOf: (key: string) => Placement | null;
  /** AI 核对过可以放心清理的量 */
  junkOf: (item: Item) => number;
  /** 比上次扫描变了多少（值得一提时才有） */
  growthOf: (key: string) => number | undefined;
  view: BoxView;
  onToggleView: () => void;
  handle: TileHandle;
  tileDragging: boolean;
  dropActive: boolean;
  openKey: string | null;
  draggingKey: string | null;
  /** 文件夹正拖到哪个软件上 */
  claimKey: string | null;
  renaming: boolean;
  onStartRename: () => void;
  onRename: (name: string | null) => void;
  onDelete: () => void;
}

function BoxMenu({ onRename, onDelete, onClose }: { onRename: () => void; onDelete: () => void; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // 按在菜单或「更多」按钮上不算点外面（否则按钮先关后开，永远关不上）
    const down = (e: PointerEvent) => { if (!ref.current?.parentElement?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  const itemCls = "w-full h-8 px-3 rounded text-left text-[13px] hover:bg-hover active:bg-press transition-colors";
  return (
    <div ref={ref} role="menu" className="absolute right-0 top-full mt-1 z-30 w-36 p-1 rounded-lg bg-card border border-slate" style={{ boxShadow: "var(--fl-shadow)" }}>
      <button type="button" role="menuitem" className={itemCls} onClick={() => { onClose(); onRename(); }}>重命名</button>
      <button type="button" role="menuitem" className={itemCls} onClick={() => { onClose(); onDelete(); }}>删除箱子</button>
    </div>
  );
}

function RenameInput({ value, onDone }: { value: string; onDone: (name: string | null) => void }) {
  const [text, setText] = useState(value);
  const done = useRef(false);
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(name);
  };
  return (
    <input
      autoFocus
      aria-label="箱子名字"
      value={text}
      maxLength={20}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(text.trim() || null);
        if (e.key === "Escape") finish(null);
      }}
      onBlur={() => finish(text.trim() || null)}
      className="flex-1 min-w-0 h-7 px-2 rounded bg-card border border-slate border-b-2 border-b-photon text-sm font-semibold outline-none"
    />
  );
}

/** 一个箱子：标题栏可拖动换位置，内容是图标库或清单 */
export function BoxPane({
  box, content, sizes, icons, boxes, placementOf, junkOf, growthOf, view, onToggleView, handle, tileDragging, dropActive,
  openKey, draggingKey, claimKey, renaming, onStartRename, onRename, onDelete,
}: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const live = content.items.filter((it) => !(it.type === "app" && it.app.uninstalled));
  const gone = content.items.filter((it) => it.type === "app" && it.app.uninstalled);
  const goneSize = gone.reduce((s, it) => s + itemSize(it, sizes), 0);

  const render = (it: Item) => {
    const junk = junkOf(it);
    const grown = growthOf(it.key);
    const props = {
      junk,
      grown,
      item: it,
      sizes,
      icon: it.type === "app" && it.app.icon ? icons.get(it.app.icon) : null,
      color: box.color,
      open: openKey === it.key,
      dragging: draggingKey === it.key,
      claimTarget: claimKey === it.key,
      title: itemTooltip(it, itemSize(it, sizes), junk, placementOf(it.key), boxes, grown),
    };
    return view === "list" ? <ItemRow key={it.key} {...props} /> : <ItemCell key={it.key} {...props} />;
  };

  return (
    <section
      aria-label={box.name}
      className={cn(
        "@container flex-1 min-w-0 flex flex-col rounded-lg bg-abyss border border-slate overflow-hidden transition-opacity",
        tileDragging && "opacity-50",
        dropActive && "outline outline-2 -outline-offset-2 outline-photon"
      )}
      style={dropActive ? { background: "color-mix(in srgb, var(--fl-accent) 5%, var(--fl-layer))" } : undefined}
    >
      <header
        className="h-11 shrink-0 flex items-center gap-2 pl-3.5 pr-1.5 border-b border-slate select-none cursor-grab"
        onPointerDown={handle.onPointerDown}
        title="拖动标题栏可以换位置"
      >
        <i className="w-2 h-2 rounded-full shrink-0" style={{ background: box.color }} />
        {renaming ? (
          <RenameInput value={box.name} onDone={onRename} />
        ) : (
          <>
            <h3 className="shrink-0 max-w-[60%] truncate text-sm font-semibold" onDoubleClick={onStartRename}>{box.name}</h3>
            <span className="min-w-0 truncate text-xs text-text-faint">{content.items.length} 个 · {formatSize(content.size)}</span>
            <span className="flex-1" />
            {/* 箱子窄的时候先省掉「可清」，再窄省掉视图切换 */}
            {content.junk >= JUNK_SHOWN && <span className="shrink-0 text-xs text-success @max-[260px]:hidden">可清 {formatSize(content.junk)}</span>}
            <button
              type="button"
              className="w-7 h-7 shrink-0 grid place-items-center rounded text-text-dim hover:bg-hover hover:text-textMain transition-colors @max-[180px]:hidden"
              title={view === "list" ? "换成图标" : "换成清单"}
              aria-label={view === "list" ? "换成图标" : "换成清单"}
              onClick={onToggleView}
            >
              {view === "list" ? <LayoutGrid className="w-4 h-4" /> : <List className="w-4 h-4" />}
            </button>
            <div className="relative shrink-0">
              <button
                type="button"
                className="w-7 h-7 grid place-items-center rounded text-text-dim hover:bg-hover hover:text-textMain transition-colors"
                aria-label="更多"
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen((v) => !v)}
              >
                <MoreHorizontal className="w-4 h-4" />
              </button>
              {menuOpen && <BoxMenu onRename={onStartRename} onDelete={onDelete} onClose={() => setMenuOpen(false)} />}
            </div>
          </>
        )}
      </header>

      <div
        data-drop={box.id}
        className={cn(
          "flex-1 min-h-0 overflow-auto p-1.5",
          view === "icons" && "grid grid-cols-[repeat(auto-fill,minmax(86px,1fr))] content-start gap-0.5"
        )}
      >
        {content.items.length === 0 ? (
          <div className="col-span-full grid place-items-center min-h-24 m-1 rounded-md border border-dashed border-[color:var(--fl-stroke-strong)] text-xs text-text-faint">
            把图标或文件夹拖到这里
          </div>
        ) : (
          <>
            {live.map(render)}
            {gone.length > 0 && (
              <div className="col-span-full flex items-center gap-2 mx-1.5 mt-2.5 mb-0.5 text-xs text-text-dim after:flex-1 after:h-px after:bg-slate">
                像是已卸载，只剩残留 · {formatSize(goneSize)}
              </div>
            )}
            {gone.map(render)}
          </>
        )}
      </div>
    </section>
  );
}
