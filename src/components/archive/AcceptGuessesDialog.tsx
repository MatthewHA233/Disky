import { useMemo, useState } from "react";
import type { ArchiveBoxRow } from "../../types";
import { formatSize } from "../../lib/format";
import { shortenHome } from "../../lib/path";
import { guessOf, itemOf, itemSize, type ArchiveIndex, type Guess, type Item } from "../../lib/archiveModel";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";
import { Checkbox } from "../ui/Checkbox";
import { ItemIcon } from "./ItemViews";

interface Props {
  guesses: { key: string; box: string }[];
  ix: ArchiveIndex;
  sizes: Map<string, number>;
  icons: ReadonlyMap<string, string | null>;
  boxes: Map<string, ArchiveBoxRow>;
  /** 只交回勾着的 */
  onConfirm: (picked: { key: string; box: string }[]) => void;
  onClose: () => void;
}

interface Row {
  item: Item;
  guess: Guess | null;
  size: number;
}

interface Group {
  box: ArchiveBoxRow;
  rows: Row[];
  size: number;
}

function sourceLabel(guess: Guess | null): string {
  if (!guess) return "";
  return guess.source === "ai" ? `AI ${Math.round((guess.confidence ?? 0) * 100)}%` : "按名字";
}

/** 「都按猜的放」之前先过目：按要放进的箱子分组，不想放的取消勾选，留在待整理 */
export function AcceptGuessesDialog({ guesses, ix, sizes, icons, boxes, onConfirm, onClose }: Props) {
  const groups = useMemo(() => {
    const byBox = new Map<string, Row[]>();
    for (const g of guesses) {
      const item = itemOf(ix, g.key);
      if (!item || !boxes.has(g.box)) continue;
      const row = { item, guess: guessOf(ix, g.key), size: itemSize(item, sizes) };
      byBox.set(g.box, [...(byBox.get(g.box) ?? []), row]);
    }
    const out: Group[] = [...byBox].map(([id, rows]) => ({
      box: boxes.get(id)!,
      rows: [...rows].sort((a, b) => b.size - a.size),
      size: rows.reduce((s, r) => s + r.size, 0),
    }));
    return out.sort((a, b) => b.size - a.size);
  }, [guesses, ix, sizes, boxes]);

  // 记取消勾选的，默认全放
  const [off, setOff] = useState<ReadonlySet<string>>(() => new Set());
  const toggle = (keys: string[], on: boolean) => setOff((prev) => {
    const next = new Set(prev);
    keys.forEach((k) => (on ? next.delete(k) : next.add(k)));
    return next;
  });

  const all = groups.flatMap((g) => g.rows);
  const picked = all.filter((r) => !off.has(r.item.key));
  const pickedSize = picked.reduce((s, r) => s + r.size, 0);

  return (
    <Dialog
      title="按猜的放进箱子"
      subtitle={`AI 和名字规则猜出了 ${all.length} 项的去处。不想放的取消勾选，会留在待整理。`}
      width="max-w-2xl"
      onClose={onClose}
      footer={
        <>
          <span className="mr-auto text-xs text-text-dim">已选 {picked.length} 项 · {formatSize(pickedSize)}</span>
          <Button onClick={onClose}>取消</Button>
          <Button
            variant="primary"
            disabled={picked.length === 0}
            onClick={() => onConfirm(guesses.filter((g) => !off.has(g.key)))}
          >
            放进去（{picked.length}）
          </Button>
        </>
      }
    >
      {groups.map(({ box, rows, size }) => {
        const keys = rows.map((r) => r.item.key);
        const onCount = keys.filter((k) => !off.has(k)).length;
        return (
          <section key={box.id} className="pb-2">
            <div className="flex items-center gap-2 px-2 pt-2 pb-1">
              <Checkbox
                checked={onCount === keys.length ? true : onCount === 0 ? false : "mixed"}
                onChange={() => toggle(keys, onCount < keys.length)}
                label={`全选放进${box.name}的`}
              />
              <i className="w-2 h-2 shrink-0 rounded-full" style={{ background: box.color }} />
              <b className="font-semibold">{box.name}</b>
              <span className="text-xs text-text-faint">{rows.length} 项 · {formatSize(size)}</span>
            </div>
            {rows.map(({ item, guess, size: s }) => {
              const on = !off.has(item.key);
              // AI 给了理由就看理由；文件夹没理由时给出位置，免得几个同名的分不清
              const note = guess?.reason || (item.type === "dir" ? shortenHome(item.node.path) : "");
              return (
                <div
                  key={item.key}
                  title={item.type === "dir" ? item.node.path : item.name}
                  className="flex items-center gap-2.5 py-1.5 pl-2 pr-2 rounded hover:bg-hover select-none"
                  onClick={() => toggle([item.key], !on)}
                >
                  <Checkbox checked={on} onChange={() => toggle([item.key], !on)} label={`放 ${item.name}`} />
                  <ItemIcon
                    item={item}
                    icon={item.type === "app" && item.app.icon ? icons.get(item.app.icon) : null}
                    color={box.color}
                    size={24}
                  />
                  <div className={on ? "flex-1 min-w-0" : "flex-1 min-w-0 opacity-50"}>
                    <div className="truncate">{item.name}</div>
                    {note && <div className="truncate text-xs text-text-faint">{note}</div>}
                  </div>
                  <span className="shrink-0 text-[11px] text-text-dim">{sourceLabel(guess)}</span>
                  <span className="shrink-0 w-16 text-right text-xs text-text-dim">{formatSize(s)}</span>
                </div>
              );
            })}
          </section>
        );
      })}
    </Dialog>
  );
}
