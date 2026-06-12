import { useCallback, useEffect, useRef, useState } from "react";
import { useScanSession } from "./hooks/useScanSession";
import { useChat } from "./hooks/useChat";
import { useAiAnalysis } from "./hooks/useAiAnalysis";
import { useTags } from "./hooks/useTags";
import { Header } from "./components/Header";
import { SplitPane } from "./components/SplitPane";
import { StatusBar } from "./components/StatusBar";
import { DirectoryTree } from "./components/DirectoryTree";
import { TreeMap } from "./components/TreeMap";
import { TagBoard } from "./components/TagBoard";
import { CleanupDialog } from "./components/CleanupDialog";
import { HistoryDialog } from "./components/HistoryDialog";
import { ChatPanel } from "./components/ChatPanel";
import { AiSettingsDialog } from "./components/AiSettingsDialog";
import { AnalyzeConfirmDialog } from "./components/AnalyzeConfirmDialog";
import { ContextMenu } from "./components/ContextMenu";
import { AnalyzeStreamViewer } from "./components/AnalyzeStreamViewer";
import { ScanProgressBar } from "./components/ScanProgressBar";
import { getChildren, getItemsInfo, saveCleanupLog, removeScanNode } from "./lib/invoke";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useMemo } from "react";
import { basename, normalizePath, parentPath, pathChain, isSameOrInside } from "./lib/path";
import type { AiAnalysis, AnalyzePathInput, ContextMenuTarget } from "./types";

type Dialog = "cleanup" | "history" | "ai-settings" | null;
type ViewMode = "tree" | "tags";

/** Build the full ancestor chain from rootPath down to targetPath */
function buildAncestry(targetPath: string, rootPath: string): string[] {
  return pathChain(rootPath, targetPath);
}

export default function App() {
  const scan = useScanSession();
  const chat = useChat();
  const aiAnalysis = useAiAnalysis();
  const tagSystem = useTags();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [chatOpen, setChatOpen] = useState(false);
  const [analyzeLogOpen, setAnalyzeLogOpen] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenuTarget | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>("tree");

  // Confirm dialog state
  const [confirmDuplicates, setConfirmDuplicates] = useState<AiAnalysis[] | null>(null);
  const [pendingItems, setPendingItems] = useState<AnalyzePathInput[]>([]);

  const [cleanedBytes, setCleanedBytes] = useState(0);
  const [scanSessionTs, setScanSessionTs] = useState(() => new Date().toISOString());

  // AI 提交的清理计划（聊天里用户同意后 agent 写入 requests/，这里弹确认窗）
  const [aiCleanup, setAiCleanup] = useState<Map<string, number> | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    listen<{ items: { path: string; reason: string }[] }>("agent-cleanup-request", async (e) => {
      const paths = e.payload.items.map((i) => i.path);
      if (paths.length === 0) return;
      try {
        const infos = await getItemsInfo(paths);
        const map = new Map(infos.map((i) => [i.path, i.size]));
        if (map.size > 0) setAiCleanup(map);
      } catch { /* 路径已不存在等情况直接忽略 */ }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  const handleCleanupDone = useCallback((bytes: number, path: string, action: "delete" | "empty") => {
    setCleanedBytes((prev) => prev + bytes);
    scan.refreshDrives();
    const drive = scan.selectedDrive ?? scan.rootPath ?? "";
    if (drive) {
      const desc = aiAnalysis.analyses.get(path)?.description ?? "";
      saveCleanupLog(drive, path, action, bytes, scanSessionTs, desc).catch(() => {});
    }
  }, [scan.selectedDrive, scan.rootPath, scan.refreshDrives, scanSessionTs, aiAnalysis.analyses]);

  const scanning = scan.status === "scanning";

  const scanPct = useMemo(() => {
    if (!scanning || !scan.progress) return 0;
    const drive = scan.drives.find(d =>
      scan.selectedDrive != null &&
      normalizePath(d.mount_point) === normalizePath(scan.selectedDrive)
    );
    const usedSpace = drive ? drive.total_space - drive.available_space : 0;
    return usedSpace > 0 ? Math.min(99, (scan.progress.total_size / usedSpace) * 100) : 0;
  }, [scanning, scan.progress, scan.drives, scan.selectedDrive]);

  // Suppress the webview's native context menu globally
  useEffect(() => {
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    return () => document.removeEventListener("contextmenu", block);
  }, []);

  // Per-drive caches for selections and nav path
  const selectedCacheRef = useRef<Map<string, Map<string, number>>>(new Map());
  const navPathCacheRef = useRef<Map<string, string[]>>(new Map());
  const prevDriveRef = useRef<string | null>(null);

  // Shared navigation path (breadcrumb stack)
  const [navPath, setNavPath] = useState<string[]>([]);

  // The deepest navigated path (used for tree selection highlight)
  const activePath = navPath.length > 1 ? navPath[navPath.length - 1] : null;

  // Filter tagged paths to only those belonging to the current scanned drive
  const filteredTaggedPaths = useMemo(() => {
    if (!scan.rootPath) return tagSystem.taggedPaths;
    return tagSystem.taggedPaths.filter((tp) => isSameOrInside(scan.rootPath!, tp.path));
  }, [tagSystem.taggedPaths, scan.rootPath]);

  // Save/restore selections and navPath on drive switch
  useEffect(() => {
    const prev = prevDriveRef.current;
    const curr = scan.selectedDrive;

    // Save state for previous drive
    if (prev) {
      selectedCacheRef.current.set(prev, selected);
      navPathCacheRef.current.set(prev, navPath);
    }

    // Restore state for new drive
    if (curr) {
      const cachedSel = selectedCacheRef.current.get(curr);
      const cachedNav = navPathCacheRef.current.get(curr);
      if (cachedSel) setSelected(cachedSel);
      else setSelected(new Map());
      if (cachedNav) setNavPath(cachedNav);
      else if (scan.rootPath) setNavPath([scan.rootPath]);
      else setNavPath([]);
    } else {
      setSelected(new Map());
      setNavPath([]);
    }

    prevDriveRef.current = curr;
  }, [scan.selectedDrive]); // eslint-disable-line react-hooks/exhaustive-deps

  // Reset navPath when rootPath changes (new scan or history load)
  useEffect(() => {
    if (scan.rootPath) setNavPath([scan.rootPath]);
    else setNavPath([]);
  }, [scan.rootPath]);

  // AI 回复中的路径点击 → 左侧目录树/TreeMap 跳转（构造完整面包屑栈）
  const navigateToPath = useCallback(
    (target: string) => {
      if (!scan.rootPath) return;
      // 文件路径跳到所在目录
      const looksLikeFile = /\.[A-Za-z0-9]{1,6}$/.test(target);
      const t = normalizePath(looksLikeFile ? parentPath(target) ?? target : target);
      const root = normalizePath(scan.rootPath);
      if (!t || t.toLowerCase() === root.toLowerCase()) {
        setNavPath([scan.rootPath]);
        return;
      }
      if (!isSameOrInside(root, t)) return;
      setNavPath(pathChain(root, t));
    },
    [scan.rootPath]
  );

  // Keep watcher in sync with current nav path
  useEffect(() => {
    const current = navPath[navPath.length - 1];
    if (current && scan.status === "done") {
      scan.setNavPath(current);
    }
  }, [navPath, scan.status]); // eslint-disable-line react-hooks/exhaustive-deps

  // When watcher detects a live update, force DirectoryTree/TreeMap to re-fetch
  useEffect(() => {
    if (scan.liveUpdateTick > 0) {
      setRefreshKey((k) => k + 1);
    }
  }, [scan.liveUpdateTick]);

  const handleNavigate = useCallback((path: string) => {
    setNavPath(prev => {
      const idx = prev.indexOf(path);
      if (idx >= 0) return prev.slice(0, idx + 1);
      return [...prev, path];
    });
  }, []);

  const handleContextMenu = useCallback((entry: import("./types").DirEntry, x: number, y: number) => {
    setContextMenu({ entry, x, y });
  }, []);

  const handleDataChanged = useCallback(() => {
    setRefreshKey(k => k + 1);
    scan.refreshDrives();
  }, [scan.refreshDrives]); // eslint-disable-line react-hooks/exhaustive-deps

  // Load tags for visible paths when liveChildren change
  useEffect(() => {
    if (scan.liveChildren.length > 0) {
      const paths = scan.liveChildren.map((c) => c.path);
      tagSystem.loadTagsForPaths(paths).catch(() => {});
    }
  }, [scan.liveChildren]); // eslint-disable-line react-hooks/exhaustive-deps

  // Clear selections, reset cleanedBytes, start new session, and exit tags view when a new scan starts
  useEffect(() => {
    if (scanning) {
      setSelected(new Map());
      setCleanedBytes(0);
      setScanSessionTs(new Date().toISOString());
      setViewMode("tree");
    }
  }, [scanning]);

  const toggleSelect = (path: string, size: number) => {
    const next = new Map(selected);
    if (next.has(path)) next.delete(path);
    else next.set(path, size);
    setSelected(next);
  };

  const runAnalysis = useCallback(
    async (items: AnalyzePathInput[]) => {
      if (items.length === 0) return;
      setAnalyzeLogOpen(true);
      await aiAnalysis.analyzeItems(items);
    },
    [aiAnalysis],
  );

  const startAnalysisFlow = useCallback(
    (allItems: AnalyzePathInput[]) => {
      const duplicates: AiAnalysis[] = [];
      const newItems: AnalyzePathInput[] = [];
      for (const item of allItems) {
        const existing = aiAnalysis.getAnalysis(item.path);
        if (existing) {
          duplicates.push(existing);
        } else {
          newItems.push(item);
        }
      }
      if (duplicates.length > 0) {
        setPendingItems(allItems);
        setConfirmDuplicates(duplicates);
      } else {
        runAnalysis(newItems);
      }
    },
    [aiAnalysis, runAnalysis],
  );

  const handleAnalyzeTreeMapPath = useCallback(
    async (currentPath: string) => {
      const children = await getChildren(currentPath, 200);
      const items: AnalyzePathInput[] = children.map((c) => ({
        path: c.path,
        name: c.name,
        size: c.logical_size,
        is_dir: c.is_dir,
      }));
      startAnalysisFlow(items);
    },
    [startAnalysisFlow],
  );

  const handleAnalyzeSelected = useCallback(async () => {
    const paths = Array.from(selected.keys());
    if (paths.length === 0) return;
    const infos = await getItemsInfo(paths);
    const items: AnalyzePathInput[] = infos.map((info) => ({
      path: info.path,
      name: basename(info.path),
      size: info.size,
      is_dir: info.is_dir,
    }));
    startAnalysisFlow(items);
  }, [selected, startAnalysisFlow]);

  const handleConfirmAnalyze = useCallback(
    (pathsToReplace: string[]) => {
      const replaceSet = new Set(pathsToReplace);
      // Include new items (no existing analysis) + confirmed replacements
      const itemsToAnalyze = pendingItems.filter((item) => {
        const existing = aiAnalysis.getAnalysis(item.path);
        return !existing || replaceSet.has(item.path);
      });
      setConfirmDuplicates(null);
      setPendingItems([]);
      runAnalysis(itemsToAnalyze);
    },
    [pendingItems, aiAnalysis, runAnalysis],
  );

  return (
    <div className="flex h-screen w-full bg-fiber overflow-hidden text-textMain relative">
      {/* Sidebar / Main Content Area */}
      <div className="flex-1 flex flex-col min-w-0 h-full relative">
        <Header
          scan={scan}
          cleanedBytes={cleanedBytes}
          onHistory={() => setDialog("history")}
          onClean={() => setDialog("cleanup")}
          cleanCount={selected.size}
          onClearSelected={() => setSelected(new Map())}
          onToggleChat={() => setChatOpen((v) => !v)}
          chatOpen={chatOpen}
          viewMode={viewMode}
          onToggleViewMode={() => setViewMode((v) => (v === "tree" ? "tags" : "tree"))}
          analyzing={aiAnalysis.analyzing}
          analyzeLogOpen={analyzeLogOpen}
          onToggleAnalyzeLog={() => setAnalyzeLogOpen((v) => !v)}
        />

        <ScanProgressBar pct={scanPct} visible={scanning} />

        {/* Main SplitPane Container */}
        <div className="flex-1 flex flex-col min-h-0 relative px-4 pb-0">
          <SplitPane
            top={
              viewMode === "tags" ? (
                <TagBoard
                  tags={tagSystem.tags}
                  taggedPaths={filteredTaggedPaths}
                  analyses={aiAnalysis.analyses}
                  onNavigateToPath={(path) => {
                    setViewMode("tree");
                    if (scan.rootPath) {
                      setNavPath(buildAncestry(path, scan.rootPath));
                    } else {
                      handleNavigate(path);
                    }
                  }}
                  onContextMenu={handleContextMenu}
                  onCreateTag={async (name, color) => {
                    await tagSystem.createTag(name, color);
                    tagSystem.loadTaggedPaths();
                  }}
                  onRenameTag={async (id, name) => {
                    await tagSystem.renameTag(id, name);
                    tagSystem.loadTaggedPaths();
                  }}
                  onDeleteTag={async (id) => {
                    await tagSystem.deleteTag(id);
                    tagSystem.loadTaggedPaths();
                  }}
                  onRefresh={() => tagSystem.loadTaggedPaths()}
                  onDataChanged={handleDataChanged}
                  onCleanupDone={handleCleanupDone}
                  key={`tags-${refreshKey}`}
                />
              ) : (
                <DirectoryTree
                  rootPath={scan.rootPath}
                  liveChildren={scan.liveChildren}
                  scanning={scanning}
                  onSelect={toggleSelect}
                  selected={selected}
                  analyses={aiAnalysis.analyses}
                  onAnalyzeSelected={handleAnalyzeSelected}
                  analyzing={aiAnalysis.analyzing}
                  navPath={navPath}
                  onNavigate={handleNavigate}
                  onContextMenu={handleContextMenu}
                  pathTags={tagSystem.pathTags}
                  onRemoveTag={(path, tagId, isDir) => tagSystem.toggleTag(path, tagId, isDir)}
                  onPathsLoaded={(paths) => tagSystem.loadTagsForPaths(paths)}
                  activePath={activePath}
                  key={`tree-${refreshKey}`}
                />
              )
            }
            bottom={
              <TreeMap
                rootPath={scan.rootPath}
                liveChildren={scan.liveChildren}
                scanning={scanning}
                analyses={aiAnalysis.analyses}
                onAnalyzePath={handleAnalyzeTreeMapPath}
                analyzing={aiAnalysis.analyzing}
                navPath={navPath}
                onNavigate={handleNavigate}
                onContextMenu={handleContextMenu}
                key={`map-${refreshKey}`}
              />
            }
          />
        </div>

        <StatusBar scan={scan} />

        {aiAnalysis.error && (
          <div className="absolute top-20 left-1/2 -translate-x-1/2 z-50 bg-danger/90 text-textMain px-6 py-2 rounded-full shadow-lg font-mono text-sm border border-[#E74C3C]">
            {aiAnalysis.error}
          </div>
        )}
      </div>

      {chatOpen && (
        <ChatPanel
          chat={chat}
          onOpenSettings={() => setDialog("ai-settings")}
          onNavigatePath={navigateToPath}
        />
      )}

      {/* Dialogs */}
      {dialog === "cleanup" && (
        <CleanupDialog
          selected={selected}
          onDone={() => { setSelected(new Map()); setDialog(null); }}
          onClose={() => setDialog(null)}
          onCleanupDone={handleCleanupDone}
        />
      )}
      {aiCleanup && (
        <CleanupDialog
          selected={aiCleanup}
          onDone={() => { setAiCleanup(null); handleDataChanged(); }}
          onClose={() => setAiCleanup(null)}
          onCleanupDone={(bytes, path, action) => {
            handleCleanupDone(bytes, path, action);
            // 从内存树摘除已删节点，界面即时一致
            removeScanNode(path).catch(() => {});
          }}
        />
      )}
      {dialog === "history" && (
        <HistoryDialog
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "ai-settings" && (
        <AiSettingsDialog onClose={() => setDialog(null)} />
      )}
      {confirmDuplicates && (
        <AnalyzeConfirmDialog
          duplicates={confirmDuplicates}
          onConfirm={handleConfirmAnalyze}
          onClose={() => { setConfirmDuplicates(null); setPendingItems([]); }}
        />
      )}
      {contextMenu && (
        <ContextMenu
          entry={contextMenu.entry}
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onDataChanged={handleDataChanged}
          onCleanupDone={handleCleanupDone}
          tags={tagSystem.tags}
          pathTags={tagSystem.pathTags}
          onToggleTag={tagSystem.toggleTag}
          onCreateTag={tagSystem.createTag}
          onDeleteTag={tagSystem.deleteTag}
        />
      )}
      {analyzeLogOpen && (
        <AnalyzeStreamViewer
          log={aiAnalysis.streamLog}
          analyzing={aiAnalysis.analyzing}
          onClose={() => setAnalyzeLogOpen(false)}
        />
      )}
    </div>
  );
}
