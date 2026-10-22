import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "../../lib/utils";

interface Props {
  title: ReactNode;
  /** 标题下方的一行说明 */
  subtitle?: ReactNode;
  onClose: () => void;
  /** 进行中的操作（删除、保存）期间禁止关闭 */
  closable?: boolean;
  /** 底部操作区，按钮右对齐 */
  footer?: ReactNode;
  /** 面板宽度（tailwind 类），默认 max-w-xl */
  width?: string;
  children: ReactNode;
}

/**
 * Fluent ContentDialog：半透明遮罩 + 居中实色面板，标题 20px，
 * 底部操作区用更深一层的底色隔开。Esc、点遮罩、右上角都可关闭。
 */
export function Dialog({ title, subtitle, onClose, closable = true, footer, width = "max-w-xl", children }: Props) {
  const panelRef = useRef<HTMLDivElement>(null);
  const downOnSmoke = useRef(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && closable) onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [closable, onClose]);

  useEffect(() => { panelRef.current?.focus(); }, []);

  return (
    <div
      className="smoke-in fixed inset-0 z-[100] flex items-center justify-center p-6"
      style={{ background: "var(--fl-smoke)" }}
      // 只有按下和松开都在遮罩上才关闭，避免在面板里拖选文字时误关
      onPointerDown={(e) => { downOnSmoke.current = e.target === e.currentTarget; }}
      onClick={(e) => { if (closable && downOnSmoke.current && e.target === e.currentTarget) onClose(); }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        tabIndex={-1}
        className={cn("dialog-in w-full max-h-full flex flex-col rounded-lg bg-card border border-slate overflow-hidden", width)}
        // 面板只是为了接住焦点（Esc 生效、Tab 从这里开始），自身不画焦点框
        style={{ boxShadow: "var(--fl-shadow-dialog)", outline: "none" }}
      >
        <div className="shrink-0 flex items-start gap-3 px-6 pt-6 pb-3">
          <div className="flex-1 min-w-0">
            <h2 className="text-xl font-semibold leading-7">{title}</h2>
            {subtitle && <p className="mt-1 text-[13px] text-text-dim">{subtitle}</p>}
          </div>
          <button
            type="button"
            aria-label="关闭"
            className="-mr-2 -mt-1 w-8 h-8 grid place-items-center rounded text-text-dim hover:bg-hover hover:text-textMain disabled:opacity-40 disabled:pointer-events-none transition-colors"
            disabled={!closable}
            onClick={onClose}
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto px-6 pb-6 text-[13px]">{children}</div>

        {footer && (
          <div className="shrink-0 flex items-center justify-end gap-2 px-6 py-5 bg-fiber border-t border-slate">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
