import { X } from "lucide-react";
import type { ArchiveNote } from "../../types";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";

interface Props {
  notes: ArchiveNote[];
  onDelete: (id: number) => void;
  onClose: () => void;
}

/** 整理助手记下的你的回答和偏好：每次开场都会交代给它，不对的可以删掉 */
export function NotesDialog({ notes, onDelete, onClose }: Props) {
  return (
    <Dialog
      title="AI 记住的"
      subtitle="整理助手每次开场都会先看这些。说错了或者不想让它记的，删掉就行。"
      onClose={onClose}
      width="max-w-lg"
      footer={<Button onClick={onClose}>关闭</Button>}
    >
      {notes.length === 0 ? (
        <p className="text-[13px] text-text-dim">还没有记下什么。</p>
      ) : (
        <ul className="flex flex-col">
          {notes.map((n) => (
            <li key={n.id} className="group flex items-start gap-2 py-2 border-b border-slate last:border-0">
              <span className="flex-1 min-w-0 text-[13px] leading-6 break-words">{n.note}</span>
              <span className="shrink-0 pt-1 text-[11px] text-text-faint">{n.created_at.slice(0, 10)}</span>
              <button
                type="button"
                className="w-6 h-6 shrink-0 grid place-items-center rounded text-text-dim hover:bg-hover hover:text-textMain"
                aria-label={`删掉：${n.note}`}
                onClick={() => onDelete(n.id)}
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
