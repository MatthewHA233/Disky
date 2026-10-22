import { useState } from "react";
import { formatSize } from "../../lib/format";
import { shortenHome } from "../../lib/path";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";
import { Checkbox } from "../ui/Checkbox";
import type { AttributeCandidate } from "./candidates";

interface Props {
  candidates: AttributeCandidate[];
  onConfirm: (paths: string[]) => void;
  onClose: () => void;
}

/** 交给 AI 查之前先过目：哪些文件夹会被 AI 翻看。像是个人文件的默认不勾 */
export function AttributeConfirmDialog({ candidates, onConfirm, onClose }: Props) {
  const [on, setOn] = useState<ReadonlySet<string>>(() => new Set(candidates.filter((c) => !c.personal).map((c) => c.node.path)));
  const toggle = (path: string) => setOn((prev) => {
    const next = new Set(prev);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    return next;
  });
  const picked = candidates.filter((c) => on.has(c.node.path));

  return (
    <Dialog
      title="让 AI 查这些文件夹是谁的"
      subtitle="AI 会翻看勾选的文件夹：文件名、exe 的厂商信息、配置文件和日志的开头，这些内容会发给你在 AI 设置里配的服务。名字像个人文件的默认没勾。"
      width="max-w-2xl"
      onClose={onClose}
      footer={
        <>
          <span className="mr-auto text-xs text-text-dim">已选 {picked.length} 个 · {formatSize(picked.reduce((s, c) => s + c.size, 0))}</span>
          <Button onClick={onClose}>取消</Button>
          <Button variant="primary" disabled={picked.length === 0} onClick={() => onConfirm(picked.map((c) => c.node.path))}>
            让 AI 查（{picked.length}）
          </Button>
        </>
      }
    >
      {candidates.map(({ node, size, personal }) => {
        const checked = on.has(node.path);
        return (
          <div
            key={node.key}
            title={node.path}
            className="flex items-center gap-2.5 py-1.5 px-2 rounded hover:bg-hover select-none"
            onClick={() => toggle(node.path)}
          >
            <Checkbox checked={checked} onChange={() => toggle(node.path)} label={`查 ${node.name}`} />
            <div className={checked ? "flex-1 min-w-0" : "flex-1 min-w-0 opacity-60"}>
              <div className="truncate text-[13px]">{node.name}</div>
              <div className="truncate text-xs text-text-faint">{shortenHome(node.path)}</div>
            </div>
            {personal && <span className="shrink-0 text-[11px] text-text-faint">像是个人文件</span>}
            <span className="shrink-0 w-16 text-right text-xs text-text-dim">{formatSize(size)}</span>
          </div>
        );
      })}
    </Dialog>
  );
}
