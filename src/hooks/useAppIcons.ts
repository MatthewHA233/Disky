import { useEffect, useMemo, useState } from "react";
import { archiveIcons } from "../lib/invoke";

/** 已取到的图标（取不到记 null，不再重复取）；整个会话共用 */
const cache = new Map<string, string | null>();
const pending = new Set<string>();
/** 取到一批就通知所有正在用图标的组件重新渲染（不跟某次 effect 绑定，换了数据也不会漏掉） */
const listeners = new Set<() => void>();
const BATCH = 40;

async function fetchIcons(sources: string[]) {
  sources.forEach((s) => pending.add(s));
  for (let i = 0; i < sources.length; i += BATCH) {
    const batch = sources.slice(i, i + BATCH);
    const urls = await archiveIcons(batch).catch(() => batch.map(() => null));
    batch.forEach((s, j) => {
      cache.set(s, urls[j] ?? null);
      pending.delete(s);
    });
    listeners.forEach((notify) => notify());
  }
}

/** 按需取一批文件的图标，返回 文件 → 图标 data URL（还没取到的不在表里） */
export function useAppIcons(sources: readonly (string | null)[]): ReadonlyMap<string, string | null> {
  const [, bump] = useState(0);

  useEffect(() => {
    const notify = () => bump((n) => n + 1);
    listeners.add(notify);
    return () => { listeners.delete(notify); };
  }, []);

  const missing = useMemo(
    () => [...new Set(sources.filter((s): s is string => !!s && !cache.has(s) && !pending.has(s)))],
    [sources],
  );
  useEffect(() => {
    if (missing.length > 0) fetchIcons(missing);
  }, [missing]);

  return cache;
}
