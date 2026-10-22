// 各页签箱子的默认摆法，以及布局、视图在本机的存取。

import type { ArchiveCategory } from "../../types";
import type { TileLayout } from "../../lib/tileLayout";

/** 内置箱子的默认摆法；用户新建的箱子由 reconcileLayout 补进去 */
export const DEFAULT_LAYOUTS: Record<ArchiveCategory, TileLayout<string>> = {
  software: [
    { w: 1.3, cells: [{ id: "game", h: 1.15 }, { id: "chat", h: 0.85 }] },
    { w: 1.3, cells: [{ id: "dev", h: 1.25 }, { id: "office", h: 0.75 }] },
    { w: 1, cells: [{ id: "create", h: 1.1 }, { id: "media", h: 0.6 }] },
    { w: 1, cells: [{ id: "tool", h: 1.2 }, { id: "system", h: 0.6 }] },
  ],
  personal: [
    { w: 1.2, cells: [{ id: "p-code", h: 1 }, { id: "p-media", h: 1 }] },
    { w: 1, cells: [{ id: "p-docs", h: 1 }, { id: "p-chat", h: 1 }] },
    { w: 1, cells: [{ id: "p-game", h: 1 }, { id: "p-down", h: 1 }] },
  ],
  devenv: [
    { w: 1.2, cells: [{ id: "env-cli", h: 1 }, { id: "env-ext", h: 1 }] },
    { w: 1, cells: [{ id: "env-node", h: 1 }, { id: "env-python", h: 1 }] },
    { w: 1, cells: [{ id: "env-lang", h: 1 }] },
  ],
};

export const layoutKey = (c: ArchiveCategory) => `disky.archive.boxes.v3.${c}`;
export const VIEWS_KEY = "disky.archive.views.v1";
export const INBOX_KEY = "disky.archive.inbox-width.v1";
export const TAB_KEY = "disky.archive.tab.v1";

export function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function writeJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 存储不可用时只在本次会话有效
  }
}

export function removeKey(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    // 存储不可用时本来也没存下什么
  }
}

export function isLayout(v: unknown): v is TileLayout<string> {
  return (
    Array.isArray(v) && v.length > 0 &&
    v.every((c) => c && typeof c.w === "number" && Array.isArray(c.cells) && c.cells.every((x: { id?: unknown; h?: unknown }) => typeof x?.id === "string" && typeof x.h === "number"))
  );
}

export function loadLayouts(): Record<ArchiveCategory, TileLayout<string>> {
  const load = (c: ArchiveCategory) => {
    const raw = readJson<unknown>(layoutKey(c), null);
    return isLayout(raw) ? raw : DEFAULT_LAYOUTS[c];
  };
  return { software: load("software"), personal: load("personal"), devenv: load("devenv") };
}
