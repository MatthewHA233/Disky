import { useState } from "react";
import type { AiAnalysis } from "../types";
import { StarRating } from "./StarRating";

interface Props {
  duplicates: AiAnalysis[];
  onConfirm: (pathsToReplace: string[]) => void;
  onClose: () => void;
}

export function AnalyzeConfirmDialog({ duplicates, onConfirm, onClose }: Props) {
  const [currentIndex, setCurrentIndex] = useState(0);
  const [pathsToReplace, setPathsToReplace] = useState<string[]>([]);

  if (duplicates.length === 0) {
    onConfirm([]);
    return null;
  }

  const current = duplicates[currentIndex];
  const remaining = duplicates.length - currentIndex;

  const advance = (replace: boolean) => {
    const next = replace ? [...pathsToReplace, current.path] : pathsToReplace;
    if (currentIndex + 1 >= duplicates.length) {
      onConfirm(next);
    } else {
      setPathsToReplace(next);
      setCurrentIndex(currentIndex + 1);
    }
  };

  const handleReplaceAll = () => {
    const remaining = duplicates.slice(currentIndex).map((d) => d.path);
    onConfirm([...pathsToReplace, ...remaining]);
  };

  const handleSkipAll = () => {
    onConfirm(pathsToReplace);
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>已有分析结果 ({currentIndex + 1}/{duplicates.length})</h2>
          <button className="modal-close" onClick={onClose}>&times;</button>
        </div>
        <div className="modal-body">
          <div className="confirm-path">{current.path}</div>
          <div className="confirm-existing">
            <div className="confirm-label">现有描述：</div>
            <div className="confirm-desc">{current.description}</div>
            <div className="confirm-stars">
              <span className="confirm-label">优先级：</span>
              <StarRating priority={current.priority} />
              <span className="confirm-priority-num">{current.priority.toFixed(1)}</span>
            </div>
          </div>
          <div className="confirm-question">是否替换此项的分析结果？</div>
          <div className="modal-actions">
            {remaining > 1 && (
              <>
                <button className="btn" onClick={handleSkipAll}>全部跳过</button>
                <button className="btn btn-primary" onClick={handleReplaceAll}>全部替换</button>
              </>
            )}
            <button className="btn" onClick={() => advance(false)}>跳过</button>
            <button className="btn btn-primary" onClick={() => advance(true)}>替换</button>
          </div>
        </div>
      </div>
    </div>
  );
}
