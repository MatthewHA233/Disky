import { useEffect, useState } from "react";
import type { Footprint } from "../types";
import { classifyFootprints } from "../lib/invoke";

export interface FootprintsState {
  items: Footprint[] | null;
  error: string | null;
}

const EMPTY: FootprintsState = { items: null, error: null };

/**
 * 扫描树就绪后按「性质」归类一次。
 * 换盘或重扫时清空结果；删除、监听到变化等刷新（refreshKey 变化）时保留旧结果直到新结果回来，
 * 避免档案馆闪回「正在归类」、矩形图丢失颜色。
 */
export function useFootprints(rootPath: string | null, ready: boolean, refreshKey: number): FootprintsState {
  const [state, setState] = useState<FootprintsState>(EMPTY);

  useEffect(() => { setState(EMPTY); }, [rootPath, ready]);

  useEffect(() => {
    if (!ready || !rootPath) return;
    let cancelled = false;
    classifyFootprints()
      .then((items) => { if (!cancelled) setState({ items, error: null }); })
      .catch((e) => { if (!cancelled) setState({ items: null, error: String(e) }); });
    return () => { cancelled = true; };
  }, [rootPath, ready, refreshKey]);

  return state;
}
