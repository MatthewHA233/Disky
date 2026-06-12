import { useEffect, useRef, useState } from "react";
import gsap from "gsap";
import { ShieldAlert, ExternalLink, RefreshCw, CheckCircle2 } from "lucide-react";
import { checkFullDiskAccess, openFullDiskAccessSettings } from "../lib/invoke";

interface Props {
  onClose: () => void;
}

/**
 * macOS 完全磁盘访问权限引导弹窗。
 * 未授权时 ~/Library/Mail 等 TCC 保护目录读不到，扫描结果会偏小。
 */
export function FullDiskAccessDialog({ onClose }: Props) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [granted, setGranted] = useState(false);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    if (dialogRef.current) {
      gsap.fromTo(dialogRef.current,
        { opacity: 0, y: 30, scale: 0.95 },
        { opacity: 1, y: 0, scale: 1, duration: 0.4, ease: "power3.out" }
      );
    }
  }, []);

  const recheck = async () => {
    setChecking(true);
    try {
      setGranted(await checkFullDiskAccess());
    } catch { /* ignore */ }
    setChecking(false);
  };

  return (
    <div className="fixed inset-0 z-[9998] flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div
        ref={dialogRef}
        className="bg-abyss border border-slate rounded-2xl shadow-2xl p-6 max-w-[440px] mx-4"
      >
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-full bg-photon/10 flex items-center justify-center shrink-0">
            <ShieldAlert className="w-5 h-5 text-photon" />
          </div>
          <div className="text-lg font-medium text-textMain">需要完全磁盘访问权限</div>
        </div>

        {granted ? (
          <div className="flex items-center gap-2 text-sm text-success mb-5">
            <CheckCircle2 className="w-4 h-4 shrink-0" />
            已检测到授权 —— 重启 Disky 后生效。
          </div>
        ) : (
          <div className="text-sm text-text-dim leading-relaxed mb-5">
            未授权时邮件、信息、Safari 等受保护目录无法读取，扫描总量会偏小。
            <br />
            点击下方按钮打开系统设置，在「完全磁盘访问权限」列表中
            <span className="text-textMain">添加并勾选 Disky</span>，
            然后<span className="text-textMain">重启应用</span>生效。
          </div>
        )}

        <div className="flex justify-end gap-3">
          <button
            className="magnetic-btn px-4 py-2 rounded-full border border-slate text-text-dim hover:text-textMain hover:bg-slate/50 text-sm"
            onClick={onClose}
          >
            暂不
          </button>
          <button
            className="magnetic-btn px-4 py-2 rounded-full border border-slate text-text-dim hover:text-textMain hover:bg-slate/50 text-sm flex items-center gap-2"
            disabled={checking}
            onClick={recheck}
          >
            <RefreshCw className={`w-3.5 h-3.5 ${checking ? "animate-spin" : ""}`} />
            重新检测
          </button>
          <button
            className="magnetic-btn px-5 py-2 rounded-full bg-photon text-fiber font-semibold text-sm flex items-center gap-2 hover:bg-[#D4B55C]"
            onClick={() => openFullDiskAccessSettings().catch(() => {})}
          >
            <ExternalLink className="w-3.5 h-3.5" />
            打开系统设置
          </button>
        </div>
      </div>
    </div>
  );
}
