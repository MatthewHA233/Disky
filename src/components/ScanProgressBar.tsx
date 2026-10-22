interface Props {
  pct: number;   // 0-100, 0 = indeterminate
  visible: boolean;
}

/** 命令栏下方的扫描进度（Fluent ProgressBar：3px 强调色细条，未知进度时来回滑动） */
export function ScanProgressBar({ pct, visible }: Props) {
  if (!visible) return null;
  return (
    <div className="relative w-full h-[3px] shrink-0 overflow-hidden bg-slate/60" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct > 0 ? Math.round(pct) : undefined}>
      {pct > 0 ? (
        <div className="h-full rounded-r-full bg-photon transition-[width] duration-500 ease-out" style={{ width: `${pct}%` }} />
      ) : (
        <div className="absolute inset-y-0 w-1/3 rounded-full bg-photon animate-[indeterminate_1.6s_ease-in-out_infinite]" />
      )}
    </div>
  );
}
