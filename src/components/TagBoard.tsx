import { useEffect, useState, useCallback, useMemo } from "react";
import { Plus, X, Settings, FolderOpen, File, Pencil, Check, FolderMinus } from "lucide-react";
import { cn } from "../lib/utils";
import { emptyFolder, getItemsInfo } from "../lib/invoke";
import { formatSize } from "../lib/format";
import { StarRating } from "./StarRating";
import type { AiAnalysis, DirEntry, Tag, TaggedPath } from "../types";

const PALETTE = [
  "#E74C3C", "#E67E22", "#F1C40F", "#2ECC71",
  "#3498DB", "#9B59B6", "#1ABC9C", "#C9A84C",
  "#95A5A6", "#E91E63",
];

// Column width = 220 * 1.4 ≈ 308px
const COL_W = 308;

interface Props {
  tags: Tag[];
  taggedPaths: TaggedPath[];
  analyses?: Map<string, AiAnalysis>;
  onNavigateToPath: (path: string) => void;
  onContextMenu: (entry: DirEntry, x: number, y: number) => void;
  onCreateTag: (name: string, color: string) => void;
  onRenameTag: (id: number, name: string) => void;
  onDeleteTag: (id: number) => void;
  onRefresh: () => void;
  onDataChanged: () => void;
}

export function TagBoard({
  tags,
  taggedPaths,
  analyses,
  onNavigateToPath,
  onContextMenu,
  onCreateTag,
  onRenameTag,
  onDeleteTag,
  onRefresh,
  onDataChanged,
}: Props) {
  const [managing, setManaging] = useState(false);
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(PALETTE[0]);
  const [confirmDelete, setConfirmDelete] = useState<Tag | null>(null);
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");

  // Sizes fetched from file system
  const [sizeMap, setSizeMap] = useState<Map<string, number>>(new Map());

  // Empty folder confirmation state
  const [emptyTarget, setEmptyTarget] = useState<TaggedPath | null>(null);
  const [emptyLoading, setEmptyLoading] = useState(false);
  const [emptyResult, setEmptyResult] = useState<string | null>(null);

  useEffect(() => { onRefresh(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Batch-fetch sizes whenever taggedPaths changes
  useEffect(() => {
    if (taggedPaths.length === 0) { setSizeMap(new Map()); return; }
    const paths = [...new Set(taggedPaths.map((tp) => tp.path))];
    getItemsInfo(paths)
      .then((infos) => setSizeMap(new Map(infos.map((i) => [i.path, i.size]))))
      .catch(() => {});
  }, [taggedPaths]);

  const handleCreate = useCallback(() => {
    const name = newName.trim();
    if (!name) return;
    onCreateTag(name, newColor);
    setNewName("");
    setNewColor(PALETTE[0]);
  }, [newName, newColor, onCreateTag]);

  const startRename = (tag: Tag) => {
    setRenamingId(tag.id);
    setRenameValue(tag.name);
  };

  const commitRename = () => {
    if (renamingId === null) return;
    const name = renameValue.trim();
    if (name && name !== tags.find((t) => t.id === renamingId)?.name) {
      onRenameTag(renamingId, name);
    }
    setRenamingId(null);
    setRenameValue("");
  };

  const tagFileCount = (tagId: number) =>
    taggedPaths.filter((tp) => tp.tag_id === tagId).length;

  const handleEmptyFolder = useCallback(async (toTrash: boolean) => {
    if (!emptyTarget) return;
    setEmptyLoading(true);
    setEmptyResult(null);
    try {
      const result = await emptyFolder(emptyTarget.path, toTrash);
      if (result.errors.length > 0) {
        setEmptyResult(`已删除 ${result.deleted} 项，${result.errors.length} 项失败`);
        setEmptyLoading(false);
        setTimeout(() => { setEmptyTarget(null); setEmptyResult(null); onDataChanged(); }, 1500);
      } else {
        setEmptyTarget(null);
        setEmptyResult(null);
        onDataChanged();
      }
    } catch (e) {
      setEmptyResult(String(e));
      setEmptyLoading(false);
    }
  }, [emptyTarget, onDataChanged]);

  const taggedPathToDirEntry = (item: TaggedPath): DirEntry => ({
    name: item.name,
    path: item.path,
    is_dir: item.is_dir,
    files: 0,
    subdirs: 0,
    logical_size: sizeMap.get(item.path) ?? 0,
  });

  const columns = useMemo(() =>
    tags.map((tag) => ({
      tag,
      items: taggedPaths.filter((tp) => tp.tag_id === tag.id),
    })),
    [tags, taggedPaths]
  );

  return (
    <div className="flex flex-col h-full bg-[#0D0D12] rounded-2xl border border-[#2A2A35] overflow-hidden shadow-2xl">
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-3 bg-[#13131A] border-b border-[#2A2A35] shrink-0">
        <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-[#888899] flex-1">标签看板</span>
        <button
          className={cn("p-1 rounded transition-colors",
            managing ? "text-[#C9A84C] bg-[#C9A84C]/10" : "text-[#888899] hover:text-[#FAF8F5]"
          )}
          onClick={() => setManaging((v) => !v)}
          title="管理标签"
        >
          <Settings className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Manage panel */}
      {managing && (
        <div className="px-4 py-3 bg-[#13131A] border-b border-[#2A2A35] space-y-2">
          <div className="flex items-center gap-2">
            <input
              type="text"
              className="flex-1 bg-[#0D0D12] border border-[#2A2A35] rounded-md px-2.5 py-1 text-[11px] text-[#FAF8F5] focus:outline-none focus:border-[#C9A84C] placeholder-[#555]"
              placeholder="新标签名称..."
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            />
            <div className="flex gap-0.5">
              {PALETTE.map((c) => (
                <button
                  key={c}
                  className={cn("w-4 h-4 rounded-full border transition-transform",
                    newColor === c ? "border-[#FAF8F5] scale-110" : "border-transparent"
                  )}
                  style={{ backgroundColor: c }}
                  onClick={() => setNewColor(c)}
                />
              ))}
            </div>
            <button
              className="px-2 py-1 text-[11px] rounded-md bg-[#C9A84C] text-[#0D0D12] hover:bg-[#D4B55C] transition-colors font-medium disabled:opacity-40"
              onClick={handleCreate}
              disabled={!newName.trim()}
            >
              <Plus className="w-3 h-3" />
            </button>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {tags.map((tag) => (
              <div
                key={tag.id}
                className="group flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] border border-[#2A2A35]"
              >
                <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: tag.color }} />
                {renamingId === tag.id ? (
                  <input
                    type="text"
                    className="bg-transparent text-[#FAF8F5] outline-none w-16 text-[11px]"
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename();
                      if (e.key === "Escape") { setRenamingId(null); setRenameValue(""); }
                    }}
                    onBlur={commitRename}
                    autoFocus
                  />
                ) : (
                  <span className="text-[#EAE6DF]">{tag.name}</span>
                )}
                {renamingId === tag.id ? (
                  <button className="text-[#2ECC71] hover:text-[#27AE60] transition-colors" onClick={commitRename} title="确认">
                    <Check className="w-3 h-3" />
                  </button>
                ) : (
                  <>
                    <button
                      className="text-[#888899] hover:text-[#C9A84C] transition-colors opacity-0 group-hover:opacity-100"
                      onClick={() => startRename(tag)} title="重命名"
                    >
                      <Pencil className="w-3 h-3" />
                    </button>
                    {!tag.is_preset && (
                      <button
                        className="text-[#888899] hover:text-[#E74C3C] transition-colors opacity-0 group-hover:opacity-100"
                        onClick={() => setConfirmDelete(tag)} title="删除标签"
                      >
                        <X className="w-3 h-3" />
                      </button>
                    )}
                  </>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Delete tag confirmation */}
      {confirmDelete && (
        <div className="px-4 py-3 bg-[#E74C3C]/5 border-b border-[#E74C3C]/20">
          <div className="text-[11px] text-[#FAF8F5] font-semibold mb-1">确认删除标签「{confirmDelete.name}」？</div>
          <div className="text-[10px] text-[#888899] mb-2">
            将从 {tagFileCount(confirmDelete.id)} 个文件/文件夹中移除此标签，标签定义也会被删除。
          </div>
          <div className="flex gap-2">
            <button className="px-3 py-1 text-[11px] rounded-md bg-[#2A2A35] text-[#888899] hover:bg-[#333340] transition-colors" onClick={() => setConfirmDelete(null)}>取消</button>
            <button
              className="px-3 py-1 text-[11px] rounded-md bg-[#E74C3C] text-white hover:bg-[#C0392B] transition-colors"
              onClick={() => { onDeleteTag(confirmDelete.id); setConfirmDelete(null); }}
            >确认删除</button>
          </div>
        </div>
      )}

      {/* Empty folder confirmation */}
      {emptyTarget && (
        <div className="px-4 py-3 bg-[#E74C3C]/5 border-b border-[#E74C3C]/20">
          <div className="text-[11px] text-[#FAF8F5] font-semibold mb-1">清空文件夹「{emptyTarget.name}」？</div>
          <div className="text-[10px] text-[#888899] mb-2">将删除该文件夹内的所有文件和子目录。</div>
          {emptyResult && <div className="text-[10px] text-[#E74C3C] mb-2">{emptyResult}</div>}
          <div className="flex gap-2">
            <button
              className="px-3 py-1 text-[11px] rounded-md bg-[#2A2A35] text-[#888899] hover:bg-[#333340] transition-colors"
              disabled={emptyLoading}
              onClick={() => { setEmptyTarget(null); setEmptyResult(null); }}
            >取消</button>
            <button
              className="px-3 py-1 text-[11px] rounded-md bg-[#E74C3C]/20 text-[#E74C3C] border border-[#E74C3C]/30 hover:bg-[#E74C3C]/30 transition-colors"
              disabled={emptyLoading}
              onClick={() => handleEmptyFolder(false)}
            >永久清空</button>
            <button
              className="px-3 py-1 text-[11px] rounded-md bg-[#E74C3C] text-white hover:bg-[#C0392B] transition-colors"
              disabled={emptyLoading}
              onClick={() => handleEmptyFolder(true)}
            >{emptyLoading ? "处理中..." : "移至回收站"}</button>
          </div>
        </div>
      )}

      {/* Columns */}
      <div className="flex-1 overflow-x-auto overflow-y-hidden custom-scrollbar">
        <div className="flex h-full min-w-max">
          {columns.map(({ tag, items }) => (
            <div
              key={tag.id}
              className="flex flex-col border-r border-[#2A2A35] last:border-r-0"
              style={{ width: COL_W, minWidth: COL_W }}
            >
              {/* Column header */}
              <div
                className="px-3 py-2.5 flex items-center gap-2 shrink-0 border-b"
                style={{ borderColor: `${tag.color}30` }}
              >
                <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ backgroundColor: tag.color }} />
                <span className="text-[11px] font-semibold text-[#FAF8F5] truncate">{tag.name}</span>
                <span className="text-[10px] text-[#555] ml-auto font-mono">{items.length}</span>
              </div>

              {/* Column items */}
              <div className="flex-1 overflow-y-auto py-1.5 px-1.5 custom-scrollbar space-y-1.5">
                {items.length === 0 ? (
                  <div className="px-3 py-6 text-center text-[10px] text-[#555]">无标记文件</div>
                ) : (
                  items.map((item) => {
                    const size = sizeMap.get(item.path);
                    const analysis = analyses?.get(item.path);
                    return (
                      <div
                        key={`${item.tag_id}-${item.path}`}
                        className="group relative rounded-xl bg-[#13131A] border border-[#2A2A35] hover:border-[#3A3A45] hover:bg-[#1A1A22] cursor-pointer transition-all"
                        onClick={() => onNavigateToPath(item.path)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          onContextMenu(taggedPathToDirEntry(item), e.clientX, e.clientY);
                        }}
                      >
                        <div className="px-3 py-2.5">
                          {/* Name row */}
                          <div className="flex items-start gap-1.5 pr-6">
                            {item.is_dir
                              ? <FolderOpen className="w-3.5 h-3.5 text-[#C9A84C] shrink-0 mt-0.5" />
                              : <File className="w-3.5 h-3.5 text-[#EAE6DF]/70 shrink-0 mt-0.5" />
                            }
                            <span className="text-[12px] text-[#EAE6DF] font-semibold leading-snug break-all">
                              {item.name}
                            </span>
                          </div>

                          {/* Path */}
                          <div className="mt-1 text-[10px] text-[#555] break-all leading-snug pl-5">
                            {item.path}
                          </div>

                          {/* Size + AI rating */}
                          {(size !== undefined || analysis) && (
                            <div className="mt-1.5 flex items-center gap-2 pl-5">
                              {size !== undefined && (
                                <span className="text-[10px] text-[#888899] font-mono">{formatSize(size)}</span>
                              )}
                              {analysis && <StarRating priority={analysis.priority} maxStars={5} />}
                            </div>
                          )}

                          {/* AI description */}
                          {analysis?.description && (
                            <div className="mt-1 text-[10px] text-[#888899] leading-relaxed break-words pl-5">
                              {analysis.description}
                            </div>
                          )}
                        </div>

                        {/* Empty folder button (hover, dirs only) */}
                        {item.is_dir && (
                          <button
                            className="absolute top-2 right-2 p-1 rounded-md text-[#555] hover:text-[#E74C3C] opacity-0 group-hover:opacity-100 transition-all hover:bg-[#E74C3C]/10"
                            onClick={(e) => { e.stopPropagation(); setEmptyTarget(item); }}
                            title="清空文件夹"
                          >
                            <FolderMinus className="w-3 h-3" />
                          </button>
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
