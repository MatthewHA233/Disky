import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check } from "lucide-react";
import type { ClaimKind } from "../../types";
import { cn } from "../../lib/utils";

/** 能指定给一块位置的成分，按常用程度排 */
export const CLAIM_KINDS: { kind: ClaimKind; name: string; desc: string }[] = [
  { kind: "data", name: "数据", desc: "设置、存档、聊天记录这类软件自己的东西" },
  { kind: "cache", name: "缓存", desc: "删了软件会自己重新生成" },
  { kind: "regen", name: "可重新下载", desc: "游戏本体、模型、依赖包，删了能再下载" },
  { kind: "prog", name: "程序本体", desc: "软件自己的程序文件" },
  { kind: "residue", name: "残留", desc: "卸载后剩下的、装完就没用的安装包" },
];

export const claimKindName = (k: string) => CLAIM_KINDS.find((c) => c.kind === k)?.name ?? k;

interface Props {
  /** 弹出的位置（视口坐标） */
  x: number;
  y: number;
  title: string;
  current?: string | null;
  /** 成分下面的其他操作（不是它的、恢复成规则认的） */
  extra?: { label: string; run: () => void }[];
  onPick: (kind: ClaimKind) => void;
  onClose: () => void;
}

const GAP = 8;

/** 选一块位置算软件的哪种成分：松手时弹出，也用在详情里改成分 */
export function ClaimMenu({ x, y, title, current, extra = [], onPick, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(GAP, Math.min(x, window.innerWidth - width - GAP)),
      top: Math.max(GAP, Math.min(y, window.innerHeight - height - GAP)),
    });
  }, [x, y]);
  // 父组件每次渲染都会给新的 onClose（AI 进度、提示条都会触发），放进 ref，监听只装一次，焦点也不会被抢回第一项
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    const down = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node)) closeRef.current(); };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // 只关菜单，不让详情抽屉（监听在 window 上）也跟着关
        e.stopPropagation();
        return closeRef.current();
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const items = [...(ref.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
      const i = items.indexOf(document.activeElement as HTMLButtonElement);
      items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    };
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, []);

  const item = "w-full flex items-start gap-2 px-2.5 py-1.5 rounded text-left hover:bg-hover focus-visible:bg-hover outline-none";
  return (
    <div
      ref={ref}
      role="menu"
      aria-label={title}
      className="fixed z-[150] w-[260px] p-1 rounded-md bg-card border border-slate dialog-in"
      style={{ left: pos.left, top: pos.top, boxShadow: "var(--fl-shadow-dialog)" }}
    >
      <div className="px-2.5 pt-1.5 pb-1 text-xs text-text-dim">{title}</div>
      {CLAIM_KINDS.map((c) => (
        <button key={c.kind} type="button" role="menuitem" className={item} onClick={() => onPick(c.kind)}>
          <Check className={cn("w-3.5 h-3.5 mt-0.5 shrink-0 text-photon", current !== c.kind && "invisible")} />
          <span className="min-w-0">
            <span className="block text-[13px]">{c.name}</span>
            <span className="block text-[11px] leading-4 text-text-faint">{c.desc}</span>
          </span>
        </button>
      ))}
      {extra.length > 0 && <div className="my-1 h-px bg-[color:var(--fl-stroke)]" />}
      {extra.map((e) => (
        <button key={e.label} type="button" role="menuitem" className={cn(item, "pl-8 text-[13px]")} onClick={e.run}>{e.label}</button>
      ))}
    </div>
  );
}
