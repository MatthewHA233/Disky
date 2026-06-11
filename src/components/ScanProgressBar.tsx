interface Props {
  pct: number;   // 0-100, 0 = indeterminate
  visible: boolean;
}

export function ScanProgressBar({ pct, visible }: Props) {
  if (!visible) return null;

  const hasPct = pct > 0;

  return (
    <div
      className="relative w-full shrink-0"
      style={{
        height: 2,
        zIndex: 60,
        background: "rgba(255,255,255,0.04)",
        overflow: "hidden",
        clipPath: "inset(0)",  // clips box-shadow overflow that overflow:hidden misses
      }}
    >
      {hasPct && (
        <div
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            height: "100%",
            width: `${pct}%`,
            transition: "width 0.6s cubic-bezier(0.4,0,0.2,1)",
            background: "linear-gradient(90deg,#00e5ff,#a855f7,#f59e0b,#00e5ff)",
            backgroundSize: "300% 100%",
            animation: "sb-shift 2.5s linear infinite",
          }}
        >
          <div style={{
            position: "absolute",
            right: 0,
            top: "50%",
            transform: "translateY(-50%)",
            width: 4,
            height: 4,
            borderRadius: "50%",
            background: "#fff",
            boxShadow: "0 0 6px 2px #00e5ffcc, 0 0 12px 4px #a855f799",
          }} />
        </div>
      )}

      <style>{`
        @keyframes sb-shift {
          0%   { background-position: 0% 0%; }
          100% { background-position: 300% 0%; }
        }
      `}</style>
    </div>
  );
}
