import { useCallback, useEffect, useState } from "react";
import type { AiAnalysis, AnalyzePathInput } from "../types";
import { analyzePaths, saveAiAnalysis, loadAllAiAnalyses } from "../lib/invoke";

export interface UseAiAnalysisReturn {
  analyses: Map<string, AiAnalysis>;
  analyzing: boolean;
  error: string | null;
  analyzeItems: (items: AnalyzePathInput[]) => Promise<void>;
  getAnalysis: (path: string) => AiAnalysis | undefined;
}

export function useAiAnalysis(): UseAiAnalysisReturn {
  const [analyses, setAnalyses] = useState<Map<string, AiAnalysis>>(new Map());
  const [analyzing, setAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load all analyses on mount
  useEffect(() => {
    loadAllAiAnalyses()
      .then((list) => {
        const map = new Map<string, AiAnalysis>();
        for (const item of list) {
          map.set(item.path, item);
        }
        setAnalyses(map);
      })
      .catch(() => {});
  }, []);

  const analyzeItems = useCallback(async (items: AnalyzePathInput[]) => {
    if (items.length === 0) return;
    setAnalyzing(true);
    setError(null);

    let remaining = [...items];
    const maxRetries = 10;
    let retryCount = 0;

    try {
      while (remaining.length > 0 && retryCount < maxRetries) {
        const results = await analyzePaths(remaining);

        // 逐个保存已返回的结果
        for (const r of results) {
          await saveAiAnalysis(r.path, r.description, r.priority);
        }

        // 刷新缓存（让 UI 立即看到已分析的结果）
        const all = await loadAllAiAnalyses();
        const map = new Map<string, AiAnalysis>();
        for (const item of all) {
          map.set(item.path, item);
        }
        setAnalyses(map);

        // 计算剩余未分析的项目
        const resultPaths = new Set(results.map((r) => r.path));
        remaining = remaining.filter((item) => !resultPaths.has(item.path));

        // 如果本轮没有任何进展，停止重试
        if (results.length === 0) break;

        retryCount++;
      }

      if (remaining.length > 0) {
        setError(`${remaining.length} 个项目未能分析完成`);
      }
    } catch (e) {
      // 网络错误等致命错误，停止重试
      setError(e instanceof Error ? e.message : String(e));
      // 即使出错也刷新一次缓存，展示已保存的结果
      try {
        const all = await loadAllAiAnalyses();
        const map = new Map<string, AiAnalysis>();
        for (const item of all) {
          map.set(item.path, item);
        }
        setAnalyses(map);
      } catch {}
    } finally {
      setAnalyzing(false);
    }
  }, []);

  const getAnalysis = useCallback(
    (path: string) => analyses.get(path),
    [analyses],
  );

  return { analyses, analyzing, error, analyzeItems, getAnalysis };
}
