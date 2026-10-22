import type { InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "../../lib/utils";

/** 表单一行：上方标签，下方控件与说明 */
export function Field({ label, hint, children, className }: { label: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-[13px]">{label}</span>
      {children}
      {hint && <span className="text-xs text-text-dim">{hint}</span>}
    </label>
  );
}

// Fluent TextBox：底边稍深，聚焦时底边变成 2px 强调色
const CONTROL = cn(
  "w-full h-8 px-2.5 rounded text-[13px] bg-card border border-slate border-b-[color:var(--fl-stroke-strong)] outline-none transition-colors",
  "hover:bg-hover focus:bg-card focus:border-b-photon focus:shadow-[inset_0_-1px_0_var(--fl-accent)]",
  "placeholder:text-text-faint disabled:opacity-50"
);

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn(CONTROL, className)} {...rest} />;
}

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="relative block">
      <select className={cn(CONTROL, "appearance-none pr-8 cursor-default", className)} {...rest}>{children}</select>
      <ChevronDown className="absolute right-2.5 top-2 w-4 h-4 text-text-dim pointer-events-none" />
    </span>
  );
}

export interface RadioOption<T extends string> {
  value: T;
  label: string;
  description?: string;
}

/** Fluent RadioButtons：竖排，每项可带一行说明 */
export function RadioGroup<T extends string>({ label, value, options, onChange, inline = false }: {
  label: string;
  value: T;
  options: RadioOption<T>[];
  onChange: (value: T) => void;
  inline?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-col gap-1.5">
      <span className="text-[13px]">{label}</span>
      <div className={cn("flex", inline ? "flex-row flex-wrap gap-x-6 gap-y-1" : "flex-col gap-1")}>
        {options.map((o) => {
          const on = o.value === value;
          return (
            <button
              key={o.value}
              type="button"
              role="radio"
              aria-checked={on}
              className="group flex items-start gap-2.5 py-1 text-left"
              onClick={() => onChange(o.value)}
            >
              <span
                className={cn(
                  "mt-0.5 w-4 h-4 shrink-0 rounded-full grid place-items-center border transition-colors",
                  on ? "bg-photon border-photon" : "border-text-faint group-hover:border-textMain"
                )}
              >
                {on && <span className="w-1.5 h-1.5 rounded-full bg-on-accent" />}
              </span>
              <span>
                <span className="block text-[13px]">{o.label}</span>
                {o.description && <span className="block text-xs text-text-dim">{o.description}</span>}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
