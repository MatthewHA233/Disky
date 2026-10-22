import { useState, type ReactNode } from "react";
import type { DeleteResult } from "../types";
import { deleteItems, emptyFolder } from "../lib/invoke";
import { formatSize } from "../lib/format";
import { basename, parentPath, TRASH_NAME } from "../lib/path";
import { AlertTriangle, CheckCircle2, Loader2, XCircle } from "lucide-react";
import { Dialog } from "./ui/Dialog";
import { Button } from "./ui/Button";
import { Checkbox } from "./ui/Checkbox";

interface Props {
  selected: Map<string, number>;
  /** 关闭结果页；removed 是成功删掉的路径（没删掉的留在选择里，方便重试） */
  onDone: (removed: string[]) => void;
  onClose: () => void;
  /** 每删掉一项调用一次：记账并从内存扫描树里摘除，等它完成再显示结果 */
  onCleanupDone: (bytes: number, path: string, action: "delete" | "empty") => void | Promise<void>;
  /** 这些只清空里面的文件，文件夹本身留着（临时文件夹） */
  emptyPaths?: ReadonlySet<string>;
}

type Step = "review" | "confirm" | "done";

/** 一行路径：名称在上、所在目录在下，右侧是大小或结果 */
function PathRow({ path, right, dim }: { path: string; right: ReactNode; dim?: boolean }) {
  return (
    <div className="flex items-center gap-3 min-h-[44px] px-3 py-1.5 border-b border-slate last:border-b-0">
      <span className="flex-1 min-w-0">
        <span className={dim ? "block truncate text-text-dim line-through" : "block truncate"} title={path}>{basename(path)}</span>
        <span className="block truncate text-[11px] text-text-faint">{parentPath(path) ?? path}</span>
      </span>
      <span className="shrink-0 text-xs text-text-dim">{right}</span>
    </div>
  );
}

export function CleanupDialog({ selected, onDone, onClose, onCleanupDone, emptyPaths }: Props) {
  const [step, setStep] = useState<Step>("review");
  const [toTrash, setToTrash] = useState(true);
  const [results, setResults] = useState<DeleteResult[]>([]);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const paths = [...selected.keys()];
  const totalSize = [...selected.values()].reduce((a, b) => a + b, 0);
  const succeeded = results.filter((r) => r.success);
  const failed = results.filter((r) => !r.success);

  const handleDelete = async () => {
    if (deleting) return;
    setDeleting(true);
    setError(null);
    try {
      const toDelete = paths.filter((p) => !emptyPaths?.has(p));
      const toEmpty = paths.filter((p) => emptyPaths?.has(p));
      const res = toDelete.length ? await deleteItems(toDelete, toTrash) : [];
      await Promise.all(res.filter((r) => r.success).map((r) => onCleanupDone(selected.get(r.path) ?? 0, r.path, "delete")));
      for (const p of toEmpty) {
        try {
          const r = await emptyFolder(p, toTrash);
          // 正被占用的文件清不掉很常见，清掉了一部分也算成功
          const ok = r.errors.length === 0 || r.bytes_freed > 0;
          res.push({ path: p, success: ok, error: r.errors.length ? `有 ${r.errors.length} 项正被占用，没有清掉` : null });
          if (r.bytes_freed > 0) await onCleanupDone(r.bytes_freed, p, "empty");
        } catch (e) {
          res.push({ path: p, success: false, error: String(e) });
        }
      }
      setResults(res);
      setStep("done");
    } catch (e) {
      setError(`删除没有完成：${String(e)}`);
    } finally {
      setDeleting(false);
    }
  };

  if (paths.length === 0) {
    return (
      <Dialog title="清理" onClose={onClose} footer={<Button variant="primary" onClick={onClose}>知道了</Button>}>
        <p className="text-text-dim">还没有选择要清理的项目。在档案馆或目录里勾选后再来。</p>
      </Dialog>
    );
  }

  if (step === "review") {
    return (
      <Dialog
        title={`清理 ${paths.length} 个项目`}
        subtitle={<>共 <b className="font-semibold text-textMain">{formatSize(totalSize)}</b>，下一步确认后才会删除</>}
        onClose={onClose}
        footer={<>
          <Button onClick={onClose}>取消</Button>
          <Button variant="primary" onClick={() => setStep("confirm")}>下一步</Button>
        </>}
      >
        <div className="rounded border border-slate bg-abyss max-h-[40vh] overflow-y-auto">
          {paths.map((p) => <PathRow key={p} path={p} right={emptyPaths?.has(p) ? `清空里面 · ${formatSize(selected.get(p) ?? 0)}` : formatSize(selected.get(p) ?? 0)} />)}
        </div>
        <div className="mt-4 flex items-start gap-3" onClick={() => setToTrash((v) => !v)}>
          <Checkbox className="mt-0.5" checked={toTrash} onChange={() => setToTrash((v) => !v)} label={`移到${TRASH_NAME}`} />
          <span>
            <span className="block">移到{TRASH_NAME}（推荐）</span>
            <span className="block text-xs text-text-dim">不勾选会直接永久删除，无法恢复。</span>
          </span>
        </div>
      </Dialog>
    );
  }

  if (step === "confirm") {
    return (
      <Dialog
        title={toTrash ? `把 ${paths.length} 个项目移到${TRASH_NAME}？` : `永久删除 ${paths.length} 个项目？`}
        onClose={onClose}
        closable={!deleting}
        width="max-w-md"
        footer={<>
          <Button onClick={() => setStep("review")} disabled={deleting}>返回</Button>
          <Button variant="danger" onClick={handleDelete} disabled={deleting}>
            {deleting && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            {deleting ? "正在删除" : toTrash ? `移到${TRASH_NAME}` : "永久删除"}
          </Button>
        </>}
      >
        <p>将释放约 <b className="font-semibold">{formatSize(totalSize)}</b>。</p>
        {toTrash ? (
          <p className="mt-2 text-text-dim">之后可以在{TRASH_NAME}里还原。</p>
        ) : (
          <div className="mt-3 flex items-start gap-2 p-3 rounded bg-danger/10 text-danger">
            <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" />
            <span>文件不会进{TRASH_NAME}，删除后无法恢复。</span>
          </div>
        )}
        {deleting && <p className="mt-3 text-xs text-text-dim">大目录可能需要几分钟，请不要关闭窗口。</p>}
        {error && <p className="mt-3 text-xs text-danger">{error}</p>}
      </Dialog>
    );
  }

  const finish = () => onDone(succeeded.map((r) => r.path));

  return (
    <Dialog
      title={failed.length === 0 ? `已清理 ${succeeded.length} 个项目` : `完成 ${succeeded.length} 个，${failed.length} 个没删掉`}
      subtitle={failed.length > 0 ? "没删掉的仍然保留在选择里。通常是正被程序占用，关闭相关软件后再试。" : undefined}
      onClose={finish}
      footer={<Button variant="primary" onClick={finish}>完成</Button>}
    >
      <div className="rounded border border-slate bg-abyss max-h-[40vh] overflow-y-auto">
        {[...failed, ...succeeded].map((r) => (
          <div key={r.path}>
            <PathRow
              path={r.path}
              dim={r.success}
              right={r.success
                ? <CheckCircle2 className="w-4 h-4 text-success" aria-label="已删除" />
                : <XCircle className="w-4 h-4 text-danger" aria-label="失败" />}
            />
            {r.error && <div className="px-3 pb-2 -mt-1 text-[11px] text-danger break-all">{r.error}</div>}
          </div>
        ))}
      </div>
    </Dialog>
  );
}
