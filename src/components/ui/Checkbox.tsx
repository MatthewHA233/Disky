import { cn } from "../../lib/utils";

interface Props {
  /** "mixed"：组里只选了一部分 */
  checked: boolean | "mixed";
  onChange: () => void;
  label: string;
  className?: string;
}

/** Fluent 复选框：16px、3px 圆角，选中时强调色底 + 对勾，部分选中显示短横。点击不冒泡，可放在可点击的行里 */
export function Checkbox({ checked, onChange, label, className }: Props) {
  return (
    <span
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      tabIndex={0}
      className={cn(
        "w-4 h-4 shrink-0 rounded-[3px] border grid place-items-center transition-colors cursor-default",
        checked ? "bg-photon border-photon" : "border-text-faint hover:border-textMain",
        className
      )}
      onClick={(e) => { e.stopPropagation(); onChange(); }}
      onKeyDown={(e) => { if (e.key === " " || e.key === "Enter") { e.preventDefault(); e.stopPropagation(); onChange(); } }}
    >
      {checked === "mixed" && <span className="w-2 h-0.5 rounded-full bg-on-accent" aria-hidden="true" />}
      {checked === true && (
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
          <polyline points="1.5,5 4,7.5 8.5,2" stroke="var(--fl-on-accent)" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
    </span>
  );
}
