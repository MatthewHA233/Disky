import { useState } from "react";
import { Plus, X } from "lucide-react";
import { cn } from "../lib/utils";
import type { Tag, FileTag } from "../types";

const PALETTE = [
  "#E74C3C", "#E67E22", "#F1C40F", "#2ECC71",
  "#3498DB", "#9B59B6", "#1ABC9C", "#FF6D00",
  "#95A5A6", "#E91E63",
];

interface Props {
  tags: Tag[];
  fileTags: FileTag[];
  onToggle: (tagId: number) => void;
  onCreate: (name: string, color: string) => void;
  onDeleteTag?: (id: number) => void;
  onClose: () => void;
}

export function TagPicker({ tags, fileTags, onToggle, onCreate, onDeleteTag, onClose }: Props) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(PALETTE[0]);
  const [confirmDelete, setConfirmDelete] = useState<Tag | null>(null);

  const activeIds = new Set(fileTags.map((ft) => ft.tag_id));

  const handleCreate = () => {
    const name = newName.trim();
    if (!name) return;
    onCreate(name, newColor);
    setNewName("");
    setNewColor(PALETTE[0]);
    setCreating(false);
  };

  // Delete confirmation view
  if (confirmDelete && onDeleteTag) {
    return (
      <div className="p-3 min-w-[220px]">
        <div className="text-[11px] text-textMain font-semibold mb-1">
          确认删除标签「{confirmDelete.name}」？
        </div>
        <div className="text-[10px] text-text-dim mb-3 leading-relaxed">
          将从所有文件/文件夹中移除此标签，标签定义也会被永久删除。
        </div>
        <div className="flex gap-2">
          <button
            className="flex-1 px-2 py-1 text-[11px] rounded-md bg-slate text-text-dim hover:bg-hover transition-colors"
            onClick={() => setConfirmDelete(null)}
          >
            取消
          </button>
          <button
            className="flex-1 px-2 py-1 text-[11px] rounded-md bg-critical text-white hover:bg-critical/90 transition-colors"
            onClick={() => {
              onDeleteTag(confirmDelete.id);
              setConfirmDelete(null);
            }}
          >
            确认删除
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="p-3 min-w-[220px]">
      <div className="flex items-center justify-between mb-2">
        <span className="text-[11px] font-semibold text-textMain">标记标签</span>
        <button
          className="p-0.5 text-text-dim hover:text-textMain transition-colors"
          onClick={onClose}
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      <div className="flex flex-wrap gap-1.5 mb-2">
        {tags.map((tag) => {
          const active = activeIds.has(tag.id);
          return (
            <div key={tag.id} className="group/pill relative inline-flex">
              <button
                className={cn(
                  "px-2.5 py-1 rounded-full text-[11px] font-medium transition-all border",
                  active
                    ? "text-on-accent border-transparent"
                    : "bg-transparent border-slate text-text-dim hover:border-text-faint"
                )}
                style={
                  active
                    ? { backgroundColor: tag.color, borderColor: tag.color }
                    : undefined
                }
                onClick={() => onToggle(tag.id)}
              >
                {tag.name}
              </button>
              {onDeleteTag && !tag.is_preset && (
                <button
                  className="absolute -top-1 -right-1 w-3.5 h-3.5 rounded-full bg-slate text-text-dim hover:bg-critical hover:text-white flex items-center justify-center opacity-0 group-hover/pill:opacity-100 transition-all"
                  onClick={(e) => { e.stopPropagation(); setConfirmDelete(tag); }}
                  title={`删除标签「${tag.name}」`}
                >
                  <X className="w-2.5 h-2.5" />
                </button>
              )}
            </div>
          );
        })}
      </div>

      {creating ? (
        <div className="space-y-2 pt-1 border-t border-slate">
          <input
            type="text"
            className="w-full bg-fiber border border-slate rounded-md px-2.5 py-1 text-[11px] text-textMain focus:outline-none focus:border-photon placeholder-[#555]"
            placeholder="标签名称..."
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            autoFocus
          />
          <div className="flex flex-wrap gap-1">
            {PALETTE.map((c) => (
              <button
                key={c}
                className={cn(
                  "w-5 h-5 rounded-full border-2 transition-transform",
                  newColor === c ? "border-textMain scale-110" : "border-transparent hover:scale-105"
                )}
                style={{ backgroundColor: c }}
                onClick={() => setNewColor(c)}
              />
            ))}
          </div>
          <div className="flex gap-1.5">
            <button
              className="flex-1 px-2 py-1 text-[11px] rounded-md bg-slate text-text-dim hover:bg-hover transition-colors"
              onClick={() => setCreating(false)}
            >
              取消
            </button>
            <button
              className="flex-1 px-2 py-1 text-[11px] rounded-md bg-photon text-on-accent hover:bg-photon/90 transition-colors font-medium"
              onClick={handleCreate}
              disabled={!newName.trim()}
            >
              创建
            </button>
          </div>
        </div>
      ) : (
        <button
          className="flex items-center gap-1.5 text-[11px] text-text-dim hover:text-photon transition-colors pt-1 border-t border-slate w-full"
          onClick={() => setCreating(true)}
        >
          <Plus className="w-3 h-3" />
          新建标签
        </button>
      )}
    </div>
  );
}
