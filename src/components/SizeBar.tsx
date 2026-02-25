interface Props {
  ratio: number;
  color?: string;
  width?: number;
}

export function SizeBar({ ratio, color = "var(--accent)", width = 100 }: Props) {
  return (
    <span
      className="size-bar"
      style={{ width: `${Math.max(ratio * width, 2)}px`, background: color }}
    />
  );
}
