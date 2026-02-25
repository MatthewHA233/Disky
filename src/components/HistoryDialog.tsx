import { useEffect, useState } from "react";
import type { ScanRecord, DiffEntry } from "../types";
import { listScans, saveScan, compareScans } from "../lib/invoke";
import { formatSize } from "../lib/format";

interface Props {
  drive: string | null;
  onClose: () => void;
}

export function HistoryDialog({ drive, onClose }: Props) {
  const [scans, setScans] = useState<ScanRecord[]>([]);
  const [idA, setIdA] = useState<number | null>(null);
  const [idB, setIdB] = useState<number | null>(null);
  const [diffs, setDiffs] = useState<DiffEntry[]>([]);
  const [msg, setMsg] = useState("");

  const refresh = () => listScans().then(setScans).catch((e) => setMsg(String(e)));

  useEffect(() => { refresh(); }, []);

  const handleSave = async () => {
    if (!drive) return;
    setMsg("正在保存...");
    try {
      const id = await saveScan(drive);
      setMsg(`已保存为扫描记录 #${id}`);
      refresh();
    } catch (e) {
      setMsg(`保存失败: ${e}`);
    }
  };

  const handleCompare = async () => {
    if (idA == null || idB == null || !drive) return;
    setMsg("正在对比...");
    try {
      const res = await compareScans(idA, idB, drive);
      setDiffs(res);
      setMsg(`发现 ${res.length} 处差异`);
    } catch (e) {
      setMsg(`对比失败: ${e}`);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-dialog modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>扫描历史</h2>
          <button className="modal-close" onClick={onClose}>&times;</button>
        </div>
        <div className="modal-body">
          {drive && (
            <button className="btn btn-primary" onClick={handleSave}>保存当前扫描</button>
          )}

          <div className="scan-list">
            <h3>历史记录</h3>
            {scans.length === 0 && <div className="text-dim">暂无保存的扫描记录。</div>}
            {scans.map((s) => (
              <div key={s.id} className="scan-row">
                <label>
                  <input type="radio" name="scanA" checked={idA === s.id} onChange={() => setIdA(s.id)} /> A
                </label>
                <label>
                  <input type="radio" name="scanB" checked={idB === s.id} onChange={() => setIdB(s.id)} /> B
                </label>
                <span>#{s.id} {s.drive} — {s.entry_count.toLocaleString()} 项 — {s.created_at}</span>
              </div>
            ))}
          </div>

          {idA != null && idB != null && (
            <button className="btn" onClick={handleCompare}>对比 A 与 B</button>
          )}

          {diffs.length > 0 && (
            <div className="diff-list">
              <h3>变化 (A &rarr; B)</h3>
              {diffs.map((d) => (
                <div key={d.path} className={`diff-row ${d.diff > 0 ? "grew" : "shrank"}`}>
                  <span className="diff-name">{d.name}</span>
                  <span className="diff-sizes">{formatSize(Math.abs(d.old_size))} &rarr; {formatSize(Math.abs(d.new_size))}</span>
                  <span className="diff-delta">{d.diff > 0 ? "+" : ""}{formatSize(Math.abs(d.diff))}</span>
                </div>
              ))}
            </div>
          )}

          {msg && <div className="status-msg">{msg}</div>}
        </div>
      </div>
    </div>
  );
}
