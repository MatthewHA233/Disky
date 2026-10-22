import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/utils";

export type ButtonVariant = "primary" | "secondary" | "danger" | "subtle";

const VARIANT: Record<ButtonVariant, string> = {
  primary: "bg-photon text-on-accent font-medium hover:bg-photon/90 active:bg-photon/80",
  secondary: "bg-card border border-slate border-b-[color:var(--fl-stroke-strong)] text-textMain hover:bg-hover active:bg-press",
  danger: "bg-critical text-white font-medium hover:bg-critical/90 active:bg-critical/80",
  subtle: "text-textMain hover:bg-hover active:bg-press",
};

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

/** Fluent 按钮：32px 高、4px 圆角，只有颜色过渡 */
export function Button({ variant = "secondary", className, type = "button", ...rest }: Props) {
  return (
    <button
      type={type}
      className={cn(
        "h-8 px-4 inline-flex items-center justify-center gap-1.5 rounded text-[13px] transition-colors",
        "disabled:opacity-40 disabled:pointer-events-none",
        VARIANT[variant],
        className
      )}
      {...rest}
    />
  );
}
