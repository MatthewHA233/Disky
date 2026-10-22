import { useState } from "react";
import { Loader2 } from "lucide-react";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";

interface Props {
  onConfirm: () => Promise<void>;
  onClose: () => void;
}

/** 重新开始整理：说清楚清掉什么、留下什么，确认后才清 */
export function ResetDialog({ onConfirm, onClose }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="重新开始整理"
      onClose={onClose}
      closable={!busy}
      width="max-w-md"
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>取消</Button>
          <Button variant="danger" onClick={confirm} disabled={busy}>
            {busy && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            清空并重新开始
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2.5 text-[13px] leading-6">
        <p>会清掉：放进箱子的结果、AI 的归类和核对结果、你和 AI 指定的归属、「AI 记住的」那些话；箱子恢复成默认的那几个。</p>
        <p className="text-text-dim">会留下：各盘的扫描结果、AI 设置、侧栏里的聊天记录（想让 AI 彻底忘掉，再在侧栏里删掉对话）。清空之前会先把整个数据库备份一份。</p>
        <p className="text-text-dim">清完会重新走一遍新手引导。</p>
        {error && <p className="p-2.5 rounded bg-danger/10 text-xs text-danger">{error}</p>}
      </div>
    </Dialog>
  );
}
