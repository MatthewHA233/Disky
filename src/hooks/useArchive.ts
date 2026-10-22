import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
  ArchiveAiPlacement, ArchiveAttributeResult, ArchiveBoxRow, ArchiveCacheCheck, ArchiveCategory, ArchiveCheckItem, ArchiveClassifyItem,
  ArchiveMode, ArchiveModel, ArchiveNote, ClaimKind,
} from "../types";
import {
  archiveAiAttribute, archiveAiCheck, archiveAiClassify, archiveAssign, archiveCreateBox, archiveDeleteBox, archiveDeleteNote, archiveForget,
  archiveMode, archiveNotes, archivePlace, archiveRenameBox, archiveReset, archiveSetMode, archiveUnassign, getArchive,
} from "../lib/invoke";
import { withAiPlacements, withChecks, withUserPlacement } from "../lib/archiveModel";

export interface ArchiveSession {
  model: ArchiveModel | null;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
  place: (keys: string[], box: string | null) => Promise<void>;
  createBox: (name: string, color: string, category: ArchiveCategory) => Promise<ArchiveBoxRow>;
  renameBox: (id: string, name: string) => Promise<void>;
  deleteBox: (id: string) => Promise<void>;
  classify: (items: ArchiveClassifyItem[], category: ArchiveCategory) => Promise<ArchiveAiPlacement[]>;
  classifying: boolean;
  /** 让 AI 核对「像是缓存 / 残留」的位置 */
  check: (items: ArchiveCheckItem[]) => Promise<ArchiveCacheCheck[]>;
  checking: boolean;
  /** 删除了一些位置后同步各盘摘要并刷新 */
  forget: (items: [string, number][]) => Promise<void>;
  /** 指定某个位置属于哪个软件的哪种成分（appKey 为 null：不属于任何软件），重新识别一遍 */
  assign: (path: string, appKey: string | null, kind: ClaimKind) => Promise<void>;
  /** 撤掉指定的归属，回到内置规则的判断 */
  unassign: (path: string) => Promise<void>;
  /** 让 AI 用内置工具查这些文件夹属于哪个软件，查完重新识别 */
  attribute: (paths: string[]) => Promise<ArchiveAttributeResult>;
  attributing: boolean;
  /** AI 正在做什么（查归属期间） */
  agentStatus: string | null;
  /** 整理模式；undefined：还没读到，null：还没走完新手引导 */
  mode: ArchiveMode | null | undefined;
  setMode: (mode: ArchiveMode) => Promise<void>;
  /** 整理助手记下的用户回答和偏好 */
  notes: ArchiveNote[];
  deleteNote: (id: number) => Promise<void>;
  /** 重新开始整理，返回备份文件的位置 */
  reset: () => Promise<string>;
}

/**
 * 档案馆数据：跨盘的软件清单 + 箱子 + 装箱结果。
 * 装箱、改箱子先改本地再写库，失败时回到库里的真实状态。
 * `refreshKey` 变化（扫描完成、打开扫描记录）时重新读取，读取期间保留旧数据。
 */
export function useArchive(refreshKey: number): ArchiveSession {
  const [model, setModel] = useState<ArchiveModel | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [classifying, setClassifying] = useState(false);
  const [checking, setChecking] = useState(false);
  const seq = useRef(0);
  /** 每写成功一次库加一：读取期间有写入时，读到的可能是写入前的旧状态，要再读一次 */
  const writes = useRef(0);

  const reload = useCallback(async () => {
    const mine = ++seq.current;
    setLoading(true);
    try {
      let before = writes.current;
      let m = await getArchive();
      for (let tries = 0; tries < 2 && before !== writes.current; tries++) {
        before = writes.current;
        m = await getArchive();
      }
      if (mine === seq.current) {
        setModel(m);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) setError(String(e));
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  }, []);

  useEffect(() => { reload(); }, [reload, refreshKey]);

  /** 乐观更新失败后回到库里的状态 */
  const guarded = useCallback(async (local: (m: ArchiveModel) => ArchiveModel, remote: () => Promise<unknown>) => {
    setModel((m) => (m ? local(m) : m));
    const seqAtStart = seq.current;
    try {
      await remote();
      writes.current += 1;
      // 写入期间有读取完成过：它可能读到写入前的状态、盖掉了本地改动，再读一次
      if (seq.current !== seqAtStart) void reload();
    } catch (e) {
      await reload();
      throw e;
    }
  }, [reload]);

  const place = useCallback((keys: string[], box: string | null) =>
    guarded((m) => withUserPlacement(m, keys, box), () => archivePlace(keys, box)), [guarded]);

  const createBox = useCallback(async (name: string, color: string, category: ArchiveCategory) => {
    const row = await archiveCreateBox(name, color, category);
    writes.current += 1;
    setModel((m) => (m ? { ...m, boxes: [...m.boxes, row] } : m));
    return row;
  }, []);

  const renameBox = useCallback((id: string, name: string) =>
    guarded((m) => ({ ...m, boxes: m.boxes.map((b) => (b.id === id ? { ...b, name } : b)) }), () => archiveRenameBox(id, name)), [guarded]);

  const deleteBox = useCallback((id: string) =>
    guarded((m) => ({
      ...m,
      boxes: m.boxes.filter((b) => b.id !== id),
      user: m.user.map((u) => (u.box_id === id ? { ...u, box_id: null } : u)),
      ai: m.ai.filter((a) => a.box_id !== id),
    }), () => archiveDeleteBox(id)), [guarded]);

  const classify = useCallback(async (items: ArchiveClassifyItem[], category: ArchiveCategory) => {
    setClassifying(true);
    try {
      const rows = await archiveAiClassify(items, category);
      writes.current += 1;
      setModel((m) => (m ? withAiPlacements(m, rows) : m));
      return rows;
    } finally {
      setClassifying(false);
    }
  }, []);

  const check = useCallback(async (items: ArchiveCheckItem[]) => {
    setChecking(true);
    try {
      const rows = await archiveAiCheck(items);
      writes.current += 1;
      setModel((m) => (m ? withChecks(m, rows) : m));
      return rows;
    } finally {
      setChecking(false);
    }
  }, []);

  const forget = useCallback(async (items: [string, number][]) => {
    if (items.length === 0) return;
    await archiveForget(items).catch(() => {});
    await reload();
  }, [reload]);

  // 归属变了要重新识别（外层扣减、找缓存都跟着变），不做本地乐观更新
  const assign = useCallback(async (path: string, appKey: string | null, kind: ClaimKind) => {
    await archiveAssign(path, appKey, kind);
    writes.current += 1;
    await reload();
  }, [reload]);

  const unassign = useCallback(async (path: string) => {
    await archiveUnassign(path);
    writes.current += 1;
    await reload();
  }, [reload]);

  const [attributing, setAttributing] = useState(false);
  const [agentStatus, setAgentStatus] = useState<string | null>(null);
  const attribute = useCallback(async (paths: string[]) => {
    setAttributing(true);
    setAgentStatus("AI 开始查了");
    const unlisten = await listen<{ text: string }>("archive-agent", (e) => setAgentStatus(e.payload.text)).catch(() => null);
    try {
      const result = await archiveAiAttribute(paths);
      writes.current += 1;
      setAgentStatus("正在按 AI 的结论重新识别");
      await reload();
      return result;
    } finally {
      unlisten?.();
      setAttributing(false);
      setAgentStatus(null);
    }
  }, [reload]);

  // 整理模式和记下的话：和模型一起刷新
  const [mode, setModeState] = useState<ArchiveMode | null | undefined>(undefined);
  const [notes, setNotes] = useState<ArchiveNote[]>([]);
  const reloadMeta = useCallback(async () => {
    const [m, n] = await Promise.all([archiveMode(), archiveNotes()]);
    setModeState(m);
    setNotes(n);
  }, []);
  // 读不出来（库一时忙）时保持「还没读到」，不能当成没走过引导、把老用户推回新手引导
  useEffect(() => { reloadMeta().catch(() => {}); }, [reloadMeta, refreshKey]);

  // 侧栏的整理助手改了档案馆（放进箱子、记归属、建箱子、记下用户的话）
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen("archive-changed", () => {
      writes.current += 1;
      void reload();
      void reloadMeta().catch(() => {});
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    }).catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reload, reloadMeta]);

  const setMode = useCallback(async (m: ArchiveMode) => {
    await archiveSetMode(m);
    setModeState(m);
  }, []);

  const deleteNote = useCallback(async (id: number) => {
    await archiveDeleteNote(id);
    setNotes((prev) => prev.filter((n) => n.id !== id));
  }, []);

  const reset = useCallback(async () => {
    const backup = await archiveReset();
    writes.current += 1;
    await Promise.all([reload(), reloadMeta()]);
    return backup;
  }, [reload, reloadMeta]);

  return {
    model, loading, error, reload, place, createBox, renameBox, deleteBox, classify, classifying, check, checking, forget, assign, unassign,
    attribute, attributing, agentStatus, mode, setMode, notes, deleteNote, reset,
  };
}
