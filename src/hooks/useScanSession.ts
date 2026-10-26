import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DirEntry, DriveInfo, ScanProgressEvent, ScanTreeEvent, SnapshotInfo } from "../types";
import { listDrives, listSnapshots, loadSnapshot, scanDisk, startWatch, stopWatch, setWatchPath } from "../lib/invoke";
import { normalizePath } from "../lib/path";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

interface DriveSnapshot {
  status: ScanStatus;
  progress: ScanProgressEvent | null;
  liveChildren: DirEntry[];
  currentDir: string;
  rootPath: string | null;
  errorMsg: string;
  restoredAt: number | null;
}

const sameRoot = (a: string, b: string) => normalizePath(a).toLowerCase() === normalizePath(b).toLowerCase();
/** 扫描完到存档写好大概要多久（压缩在后台做） */
const SNAPSHOT_SETTLE_MS = 30_000;

export interface ScanSession {
  drives: DriveInfo[];
  selectedDrive: string | null;
  setSelectedDrive: (drive: string | null) => void;
  status: ScanStatus;
  progress: ScanProgressEvent | null;
  liveChildren: DirEntry[];
  currentDir: string;
  rootPath: string | null;
  errorMsg: string;
  startScan: () => void;
  /** 选中指定磁盘并立即扫描（欢迎页的磁盘卡片用） */
  scanDrive: (drive: string) => void;
  loadFromHistory: (rootPath: string, children: DirEntry[]) => void;
  refreshDrives: () => void;
  setNavPath: (path: string) => void;
  liveUpdateTick: number;
  /** 各盘最近一次整盘扫描存下来的结果 */
  snapshots: SnapshotInfo[];
  /** 当前显示的是存下来的扫描（不是这次启动后扫的）：那次扫描的时间 */
  restoredAt: number | null;
  /** 打开某块盘存下来的扫描 */
  openSnapshot: (drive: string) => Promise<void>;
  /** 正在打开存下来的扫描的那块盘（C 盘那份要读好几秒） */
  opening: string | null;
}

export function useScanSession(): ScanSession {
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [selectedDrive, setSelectedDriveRaw] = useState<string | null>(null);
  const [status, setStatus] = useState<ScanStatus>("idle");
  const [progress, setProgress] = useState<ScanProgressEvent | null>(null);
  const [liveChildren, setLiveChildren] = useState<DirEntry[]>([]);
  const [currentDir, setCurrentDir] = useState("");
  const [rootPath, setRootPath] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState("");
  const [liveUpdateTick, setLiveUpdateTick] = useState(0);
  const [snapshots, setSnapshots] = useState<SnapshotInfo[]>([]);
  const [restoredAt, setRestoredAt] = useState<number | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const snapshotsRef = useRef<SnapshotInfo[]>([]);
  snapshotsRef.current = snapshots;
  useEffect(() => { listSnapshots().then(setSnapshots).catch(() => {}); }, []);

  const unlistenProgressRef = useRef<UnlistenFn | null>(null);
  const unlistenTreeRef = useRef<UnlistenFn | null>(null);
  const unlistenLiveRef = useRef<UnlistenFn | null>(null);
  const skipResetRef = useRef(false);
  // 同一时间只扫一块盘：后端只留一棵树，进度事件也不分盘
  const scanningRef = useRef<string | null>(null);
  // 开扫、切盘、打开存下的扫描都换一个号。打开要读好几秒，读完号变了说明已经在做别的，读出来的不用了
  const genRef = useRef(0);
  const selectedDriveRef = useRef<string | null>(null);
  selectedDriveRef.current = selectedDrive;
  const supersede = useCallback(() => {
    genRef.current += 1;
    setOpening(null);
    return genRef.current;
  }, []);

  // Per-drive state cache
  const cacheRef = useRef<Map<string, DriveSnapshot>>(new Map());
  const prevDriveRef = useRef<string | null>(null);
  // 后端只保留最近一次扫描（或加载）的那棵树；别的盘的缓存快照指向的树已经不在了
  const loadedRootRef = useRef<string | null>(null);

  const refreshDrives = useCallback(() => {
    listDrives().then(setDrives).catch(() => {});
  }, []);

  useEffect(() => {
    refreshDrives();
    // Refresh when window regains focus (user may have deleted files externally)
    window.addEventListener("focus", refreshDrives);
    return () => {
      window.removeEventListener("focus", refreshDrives);
      unlistenProgressRef.current?.();
      unlistenTreeRef.current?.();
      unlistenLiveRef.current?.();
      stopWatch().catch(() => {});
    };
  }, [refreshDrives]);

  // Wrap setSelectedDrive to save/restore cache
  const setSelectedDrive = useCallback((drive: string | null) => {
    setSelectedDriveRaw(drive);
  }, []);

  // Handle drive switching: save old state, restore or reset
  useEffect(() => {
    if (skipResetRef.current) {
      skipResetRef.current = false;
      prevDriveRef.current = selectedDrive;
      return;
    }
    // 换盘了：还在读的存档不要了
    supersede();

    const prev = prevDriveRef.current;

    // Save current state for the previous drive
    if (prev) {
      cacheRef.current.set(prev, {
        status,
        progress,
        liveChildren,
        currentDir,
        rootPath,
        errorMsg,
        restoredAt,
      });
    }

    // Try to restore from cache for the new drive
    if (selectedDrive) {
      const cached = cacheRef.current.get(selectedDrive);
      const treeGone = cached?.rootPath != null && cached.rootPath !== loadedRootRef.current;
      if (treeGone) cacheRef.current.delete(selectedDrive);
      if (cached && !treeGone) {
        setStatus(cached.status);
        setProgress(cached.progress);
        setLiveChildren(cached.liveChildren);
        setCurrentDir(cached.currentDir);
        setRootPath(cached.rootPath);
        setErrorMsg(cached.errorMsg);
        setRestoredAt(cached.restoredAt);
        prevDriveRef.current = selectedDrive;
        return;
      }
    }

    // No cache — reset to idle
    setStatus("idle");
    setProgress(null);
    setLiveChildren([]);
    setCurrentDir("");
    setRootPath(null);
    setErrorMsg("");
    setRestoredAt(null);
    prevDriveRef.current = selectedDrive;
    // 这块盘之前整盘扫过、存了结果：直接打开，不用重扫（要马上扫的除外）
    if (selectedDrive && pendingScanRef.current !== selectedDrive && snapshotsRef.current.some((s) => sameRoot(s.root, selectedDrive))) {
      void openSnapshotRef.current?.(selectedDrive);
    }
  }, [selectedDrive]); // eslint-disable-line react-hooks/exhaustive-deps

  const startScan = useCallback(async () => {
    if (!selectedDrive || scanningRef.current) return;
    const drive = selectedDrive;
    scanningRef.current = drive;
    supersede();

    setStatus("scanning");
    setProgress(null);
    setLiveChildren([]);
    setCurrentDir("");
    setRootPath(null);
    setErrorMsg("");
    setRestoredAt(null);
    loadedRootRef.current = null;

    // Cleanup previous listeners
    unlistenProgressRef.current?.();
    unlistenTreeRef.current?.();

    try {
      // Listen for scan-progress events
      unlistenProgressRef.current = await listen<ScanProgressEvent>(
        "scan-progress",
        (event) => {
          setProgress(event.payload);
        }
      );

      // Listen for scan-tree events (live children updates)
      unlistenTreeRef.current = await listen<ScanTreeEvent>(
        "scan-tree",
        (event) => {
          setLiveChildren(event.payload.children);
          setCurrentDir(event.payload.current_dir);
        }
      );

      await scanDisk(drive);
      const newRootPath = normalizePath(drive);
      loadedRootRef.current = newRootPath;
      setStatus("done");
      setRootPath(newRootPath);
      refreshDrives();

      // Start fs watcher and listen for live updates
      unlistenLiveRef.current?.();
      unlistenLiveRef.current = await listen<DirEntry[]>("live-update", (event) => {
        setLiveChildren(event.payload);
        setLiveUpdateTick((t) => t + 1);
      });
      startWatch(newRootPath).catch(() => {});
      setCurrentDir("");

      // Update cache immediately after scan completes
      cacheRef.current.set(drive, {
        status: "done",
        progress: null,
        liveChildren: [],  // will be set by the state already
        currentDir: "",
        rootPath: newRootPath,
        errorMsg: "",
        restoredAt: null,
      });
      // 整盘扫描的结果后台正在存，先记上这块盘有存档；存完再读一次真实的列表
      setSnapshots((prev) => [
        ...prev.filter((s) => !sameRoot(s.root, newRootPath)),
        { root: newRootPath, scanned_at: Math.floor(Date.now() / 1000), bytes: 0 },
      ]);
      window.setTimeout(() => { listSnapshots().then(setSnapshots).catch(() => {}); }, SNAPSHOT_SETTLE_MS);
    } catch (e) {
      setStatus("error");
      setErrorMsg(String(e));
    } finally {
      unlistenProgressRef.current?.();
      unlistenProgressRef.current = null;
      unlistenTreeRef.current?.();
      unlistenTreeRef.current = null;
      scanningRef.current = null;
    }
  }, [selectedDrive, supersede]);

  // 切换磁盘会触发上面的「恢复/重置」effect，所以先切盘、等那个 effect 跑完再开扫，
  // 否则刚设的 scanning 状态会被重置回 idle
  const pendingScanRef = useRef<string | null>(null);
  const scanDrive = useCallback((drive: string) => {
    if (scanningRef.current) return;
    if (drive === selectedDrive) {
      startScan();
      return;
    }
    pendingScanRef.current = drive;
    setSelectedDriveRaw(drive);
  }, [selectedDrive, startScan]);

  useEffect(() => {
    if (pendingScanRef.current && pendingScanRef.current === selectedDrive) {
      pendingScanRef.current = null;
      startScan();
    }
  }, [selectedDrive, startScan]);

  const setNavPath = useCallback((path: string) => {
    setWatchPath(path).catch(() => {});
  }, []);

  // 打开存下来的扫描：后端把它放进内存当作当前的树，前端和扫完一样显示，并接着监听变化
  const openSnapshotRef = useRef<((drive: string) => Promise<void>) | null>(null);
  const openSnapshot = useCallback(async (drive: string) => {
    if (scanningRef.current) return;
    const gen = supersede();
    setOpening(drive);
    try {
      const res = await loadSnapshot(normalizePath(drive));
      if (genRef.current !== gen) return;
      setOpening(null);
      if (!sameRoot(drive, selectedDriveRef.current ?? "")) skipResetRef.current = true;
      loadedRootRef.current = res.root_path;
      setSelectedDriveRaw(drive);
      setStatus("done");
      setRootPath(res.root_path);
      setLiveChildren(res.children);
      setCurrentDir("");
      setProgress(null);
      setErrorMsg("");
      setRestoredAt(res.scanned_at);
      unlistenLiveRef.current?.();
      unlistenLiveRef.current = await listen<DirEntry[]>("live-update", (event) => {
        setLiveChildren(event.payload);
        setLiveUpdateTick((t) => t + 1);
      });
      startWatch(res.root_path).catch(() => {});
    } catch (e) {
      if (genRef.current !== gen) return;
      setOpening(null);
      setStatus("error");
      setErrorMsg(`没能打开上次的扫描：${String(e)}`);
    }
  }, [supersede]);
  openSnapshotRef.current = openSnapshot;

  const loadFromHistory = useCallback((root: string, children: DirEntry[]) => {
    // Find the matching drive in the dropdown list
    const matchDrive = drives.find(
      (d) => normalizePath(d.mount_point) === normalizePath(root)
    )?.mount_point;

    skipResetRef.current = true;
    loadedRootRef.current = root;
    setSelectedDriveRaw(matchDrive ?? root);
    setStatus("done");
    setRootPath(root);
    setLiveChildren(children);
    setCurrentDir("");
    setProgress(null);
    setErrorMsg("");
  }, [drives]);

  return {
    drives,
    selectedDrive,
    setSelectedDrive,
    status,
    progress,
    liveChildren,
    currentDir,
    rootPath,
    errorMsg,
    startScan,
    scanDrive,
    loadFromHistory,
    refreshDrives,
    setNavPath,
    liveUpdateTick,
    snapshots,
    restoredAt,
    openSnapshot,
    opening,
  };
}
