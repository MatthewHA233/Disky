import { useState } from "react";
import { CheckCircle2, ExternalLink, RefreshCw } from "lucide-react";
import { checkFullDiskAccess, openFullDiskAccessSettings } from "../lib/invoke";
import { Dialog } from "./ui/Dialog";
import { Button } from "./ui/Button";

interface Props {
  onClose: () => void;
}

/**
 * macOS 完全磁盘访问权限引导弹窗。
 * 未授权时 ~/Library/Mail 等 TCC 保护目录读不到，扫描结果会偏小。
 */
export function FullDiskAccessDialog({ onClose }: Props) {
  const [granted, setGranted] = useState(false);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const recheck = async () => {
    setChecking(true);
    setError(null);
    try {
      setGranted(await checkFullDiskAccess());
    } catch (e) {
      setError(`检测失败：${String(e)}`);
    } finally {
      setChecking(false);
    }
  };

  const openSettings = () => {
    setError(null);
    openFullDiskAccessSettings().catch((e) => setError(`没能打开系统设置：${String(e)}`));
  };

  return (
    <Dialog
      title="需要完全磁盘访问权限"
      onClose={onClose}
      width="max-w-md"
      footer={<>
        <Button onClick={onClose}>暂不</Button>
        <Button onClick={recheck} disabled={checking}>
          <RefreshCw className={checking ? "w-3.5 h-3.5 animate-spin" : "w-3.5 h-3.5"} />重新检测
        </Button>
        <Button variant="primary" onClick={openSettings}>
          <ExternalLink className="w-3.5 h-3.5" />打开系统设置
        </Button>
      </>}
    >
      {granted ? (
        <p className="flex items-center gap-2 text-success">
          <CheckCircle2 className="w-4 h-4 shrink-0" />已经授权，重启 Disky 后生效。
        </p>
      ) : (
        <p className="text-text-dim leading-relaxed">
          没有授权时，邮件、信息、Safari 等受保护的目录读不到，扫描出的总量会偏小。
          打开系统设置，在「完全磁盘访问权限」里<span className="text-textMain">添加并勾选 Disky</span>，然后<span className="text-textMain">重启 Disky</span>。
        </p>
      )}
      {error && <p className="mt-3 text-xs text-danger">{error}</p>}
    </Dialog>
  );
}
