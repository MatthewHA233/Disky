import type { CSSProperties } from "react";

interface Props {
  className?: string;
  style?: CSSProperties;
  strokeWidth?: number;
}

/**
 * Disky 的 AI 图标：脑袋本身是个带尾巴的对话气泡，头顶一根天线、两只眼睛——它会来问你。
 * 按 lucide 的规格画（24×24、圆头圆角线条），和其他图标放在一起粗细一致。
 */
export function AiIcon({ className, style, strokeWidth = 2 }: Props) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      style={style}
      aria-hidden="true"
    >
      <path d="M12 5V3" />
      <path d="M6.5 5h11A3.5 3.5 0 0 1 21 8.5v6a3.5 3.5 0 0 1-3.5 3.5H11l-4 3v-3h-.5A3.5 3.5 0 0 1 3 14.5v-6A3.5 3.5 0 0 1 6.5 5Z" />
      <path d="M9 10.5v1.5M15 10.5v1.5" />
    </svg>
  );
}
