import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AiAnalysis, AnalyzePathInput } from "../types";
import { analyzePaths, saveAiAnalysis, loadAllAiAnalyses } from "../lib/invoke";

export interface UseAiAnalysisReturn {
  analyses: Map<string, AiAnalysis>;
  analyzing: boolean;
  error: string | null;
  streamLog: string;
  clearStreamLog: () => void;
  analyzeItems: (items: AnalyzePathInput[]) => Promise<void>;
  getAnalysis: (path: string) => AiAnalysis | undefined;
}

export function useAiAnalysis(): UseAiAnalysisReturn {
  const [analyses, setAnalyses] = useState<Map<string, AiAnalysis>>(new Map());
  const [analyzing, setAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [streamLog, setStreamLog] = useState("");
  const unlistenRef = useRef<(() => void) | null>(null);

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

  // Listen to analyze-stream events from Rust
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ delta: string; done: boolean; error: string | null }>("analyze-stream", (event) => {
      const { delta } = event.payload;
      if (delta) {
        setStreamLog((prev) => prev + delta);
      }
    }).then((fn) => {
      unlisten = fn;
      unlistenRef.current = fn;
    });
    return () => { unlisten?.(); };
  }, []);

  const clearStreamLog = useCallback(() => setStreamLog(""), []);

  const analyzeItems = useCallback(async (items: AnalyzePathInput[]) => {
    if (items.length === 0) return;
    setAnalyzing(true);
    setError(null);
    setStreamLog("");

    const BATCH_SIZE = 5;
    const CONCURRENCY = 3;

    // Split into batches of BATCH_SIZE
    const batches: AnalyzePathInput[][] = [];
    for (let i = 0; i < items.length; i += BATCH_SIZE) {
      batches.push(items.slice(i, i + BATCH_SIZE));
    }

    const errors: string[] = [];
    let failedCount = 0;

    // Merge new results into existing analyses map and persist
    const applyResults = async (results: { path: string; description: string; priority: number }[]) => {
      if (results.length === 0) return;
      await Promise.all(results.map((r) => saveAiAnalysis(r.path, r.description, r.priority)));
      setAnalyses((prev) => {
        const next = new Map(prev);
        const now = new Date().toLocaleString();
        for (const r of results) {
          next.set(r.path, { path: r.path, description: r.description, priority: r.priority, created_at: now });
        }
        return next;
      });
    };

    try {
      // Process batches in groups of CONCURRENCY so UI updates progressively
      for (let i = 0; i < batches.length; i += CONCURRENCY) {
        const group = batches.slice(i, i + CONCURRENCY);
        const settled = await Promise.allSettled(group.map((batch) => analyzePaths(batch)));
        for (const result of settled) {
          if (result.status === "fulfilled") {
            await applyResults(result.value);
          } else {
            errors.push(String(result.reason));
            failedCount += group[settled.indexOf(result)]?.length ?? 0;
          }
        }
      }

      if (failedCount > 0) {
        setError(`${failedCount} 个项目分析失败：${errors[0]}`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setAnalyzing(false);
    }
  }, []);

  const getAnalysis = useCallback(
    (path: string) => analyses.get(path),
    [analyses],
  );

  return { analyses, analyzing, error, streamLog, clearStreamLog, analyzeItems, getAnalysis };
}
