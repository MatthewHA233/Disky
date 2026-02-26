import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DirEntry, DriveInfo, ScanProgressEvent, ScanTreeEvent } from "../types";
import { listDrives, scanDisk } from "../lib/invoke";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

interface DriveSnapshot {
  status: ScanStatus;
  progress: ScanProgressEvent | null;
  liveChildren: DirEntry[];
  currentDir: string;
  rootPath: string | null;
  errorMsg: string;
}

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
  loadFromHistory: (rootPath: string, children: DirEntry[]) => void;
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

  const unlistenProgressRef = useRef<UnlistenFn | null>(null);
  const unlistenTreeRef = useRef<UnlistenFn | null>(null);
  const skipResetRef = useRef(false);

  // Per-drive state cache
  const cacheRef = useRef<Map<string, DriveSnapshot>>(new Map());
  const prevDriveRef = useRef<string | null>(null);

  useEffect(() => {
    listDrives()
      .then(setDrives)
      .catch((e) => setErrorMsg(String(e)));
    return () => {
      unlistenProgressRef.current?.();
      unlistenTreeRef.current?.();
    };
  }, []);

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
      });
    }

    // Try to restore from cache for the new drive
    if (selectedDrive) {
      const cached = cacheRef.current.get(selectedDrive);
      if (cached) {
        setStatus(cached.status);
        setProgress(cached.progress);
        setLiveChildren(cached.liveChildren);
        setCurrentDir(cached.currentDir);
        setRootPath(cached.rootPath);
        setErrorMsg(cached.errorMsg);
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
    prevDriveRef.current = selectedDrive;
  }, [selectedDrive]); // eslint-disable-line react-hooks/exhaustive-deps

  const startScan = useCallback(async () => {
    if (!selectedDrive || status === "scanning") return;

    setStatus("scanning");
    setProgress(null);
    setLiveChildren([]);
    setCurrentDir("");
    setRootPath(null);
    setErrorMsg("");

    // Cleanup previous listeners
    unlistenProgressRef.current?.();
    unlistenTreeRef.current?.();

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

    try {
      await scanDisk(selectedDrive);
      const newRootPath = selectedDrive.replace(/\\$/, "");
      setStatus("done");
      setRootPath(newRootPath);
      setCurrentDir("");

      // Update cache immediately after scan completes
      cacheRef.current.set(selectedDrive, {
        status: "done",
        progress: null,
        liveChildren: [],  // will be set by the state already
        currentDir: "",
        rootPath: newRootPath,
        errorMsg: "",
      });
    } catch (e) {
      setStatus("error");
      setErrorMsg(String(e));
    } finally {
      unlistenProgressRef.current?.();
      unlistenProgressRef.current = null;
      unlistenTreeRef.current?.();
      unlistenTreeRef.current = null;
    }
  }, [selectedDrive, status]);

  const loadFromHistory = useCallback((root: string, children: DirEntry[]) => {
    // Find the matching drive in the dropdown list
    const matchDrive = drives.find(
      (d) => d.mount_point.replace(/\\$/, "") === root
    )?.mount_point;

    skipResetRef.current = true;
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
    loadFromHistory,
  };
}
