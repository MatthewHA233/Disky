import { useEffect, useState } from "react";
import type { AiAnalysis } from "../types";
import { StarRating } from "./StarRating";
import { Dialog } from "./ui/Dialog";
import { Button } from "./ui/Button";

interface Props {
  duplicates: AiAnalysis[];
  onConfirm: (pathsToReplace: string[]) => void;
  onClose: () => void;
}

/** 重新分析前逐个确认：已经有结果的路径要不要用新的覆盖 */
export function AnalyzeConfirmDialog({ duplicates, onConfirm, onClose }: Props) {
  const [currentIndex, setCurrentIndex] = useState(0);
  const [pathsToReplace, setPathsToReplace] = useState<string[]>([]);

  // 没有冲突就直接继续，不弹窗
  useEffect(() => {
    if (duplicates.length === 0) onConfirm([]);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  if (duplicates.length === 0) return null;

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

  const replaceAll = () => onConfirm([...pathsToReplace, ...duplicates.slice(currentIndex).map((d) => d.path)]);
  const skipAll = () => onConfirm(pathsToReplace);

  return (
    <Dialog
      title="这里已经分析过了"
      subtitle={duplicates.length > 1 ? `第 ${currentIndex + 1} 个，共 ${duplicates.length} 个已有结果` : undefined}
      onClose={onClose}
      width="max-w-lg"
      footer={<>
        {remaining > 1 && (
          <>
            <Button variant="subtle" className="mr-auto px-2" onClick={skipAll}>其余都保留</Button>
            <Button variant="subtle" className="px-2" onClick={replaceAll}>其余都重新分析</Button>
          </>
        )}
        <Button onClick={() => advance(false)}>保留原结果</Button>
        <Button variant="primary" onClick={() => advance(true)}>重新分析</Button>
      </>}
    >
      <div className="rounded border border-slate bg-abyss p-3">
        <div className="text-xs text-text-dim">位置</div>
        <div className="mt-0.5 break-all">{current.path}</div>
        <div className="mt-3 text-xs text-text-dim">原来的结论</div>
        <div className="mt-0.5">{current.description}</div>
        <div className="mt-3 flex items-center gap-2 text-xs text-text-dim">
          清理优先级 <StarRating priority={current.priority} />
        </div>
      </div>
      <p className="mt-3 text-text-dim">重新分析会用新的结论替换上面的内容。</p>
    </Dialog>
  );
}
