import { useState } from "react";
import type { DeleteResult } from "../types";
import { deleteItems } from "../lib/invoke";
import { formatSize } from "../lib/format";

interface Props {
  selected: Map<string, number>;
  onDone: () => void;
  onClose: () => void;
}

type Step = "review" | "confirm" | "done";

export function CleanupDialog({ selected, onDone, onClose }: Props) {
  const [step, setStep] = useState<Step>("review");
  const [toTrash, setToTrash] = useState(true);
  const [results, setResults] = useState<DeleteResult[]>([]);

  const paths = [...selected.keys()];
  const totalSize = [...selected.values()].reduce((a, b) => a + b, 0);

  const handleDelete = async () => {
    const res = await deleteItems(paths, toTrash);
    setResults(res);
    setStep("done");
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>清理文件</h2>
          <button className="modal-close" onClick={onClose}>&times;</button>
        </div>
        <div className="modal-body">
          {paths.length === 0 ? (
            <div className="status-msg">未选择任何项目。请在目录树中勾选要清理的文件。</div>
          ) : (
            <>
              {step === "review" && (
                <>
                  <div className="cleanup-summary">
                    已选择 {paths.length} 项 — {formatSize(totalSize)}
                  </div>
                  <ul className="cleanup-list">
                    {paths.map((p) => (
                      <li key={p}>{p} <span className="text-dim">({formatSize(selected.get(p) ?? 0)})</span></li>
                    ))}
                  </ul>
                  <label className="cleanup-option">
                    <input type="checkbox" checked={toTrash} onChange={(e) => setToTrash(e.target.checked)} />
                    移到回收站（更安全）
                  </label>
                  <div className="modal-actions">
                    <button className="btn" onClick={onClose}>取消</button>
                    <button className="btn btn-danger" onClick={() => setStep("confirm")}>删除</button>
                  </div>
                </>
              )}
              {step === "confirm" && (
                <>
                  <p>确定要删除 {paths.length} 项（{formatSize(totalSize)}）吗？</p>
                  <p>{toTrash ? "文件将移到回收站。" : "文件将被永久删除，无法恢复！"}</p>
                  <div className="modal-actions">
                    <button className="btn" onClick={() => setStep("review")}>取消</button>
                    <button className="btn btn-danger" onClick={handleDelete}>确认删除</button>
                  </div>
                </>
              )}
              {step === "done" && (
                <>
                  <div className="cleanup-summary">
                    已删除 {results.filter((r) => r.success).length}/{results.length} 项
                  </div>
                  <ul className="cleanup-list">
                    {results.map((r) => (
                      <li key={r.path} className={r.success ? "success" : "error"}>
                        {r.success ? "\u2713" : "\u2717"} {r.path} {r.error && <span className="text-dim">({r.error})</span>}
                      </li>
                    ))}
                  </ul>
                  <div className="modal-actions">
                    <button className="btn btn-primary" onClick={onDone}>完成</button>
                  </div>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
