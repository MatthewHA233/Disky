import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle } from "lucide-react";
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
import { FullDiskAccessDialog } from "./components/FullDiskAccessDialog";
import { checkFullDiskAccess, loadAiSettings, saveSnapshot } from "./lib/invoke";
import { ScanProgressBar } from "./components/ScanProgressBar";
import { WelcomePanel } from "./components/WelcomePanel";
import { ArchiveView } from "./components/archive/ArchiveView";
import { Onboarding } from "./components/onboarding/Onboarding";
import { useFootprints } from "./hooks/useFootprints";
import { useArchive } from "./hooks/useArchive";
import { getChildren, getItemsInfo, saveCleanupLog, removeScanNode, refreshScanNode } from "./lib/invoke";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useMemo } from "react";
import { basename, normalizePath, parentPath, pathChain, isSameOrInside } from "./lib/path";
import type { AiAnalysis, AnalyzePathInput, ArchiveMode, ContextMenuTarget } from "./types";

/** 新手引导结束后替用户发的第一句，整理助手据此开场 */
const ASSISTANT_KICKOFF = "我刚设置好 Disky。请先看一遍档案馆现状，然后从最值得问的地方开始问我。";

type Dialog = "cleanup" | "history" | "ai-settings" | null;
type ViewMode = "tree" | "tags" | "archive";

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
  const [viewMode, setViewMode] = useState<ViewMode>("archive");

  // Confirm dialog state
  const [confirmDuplicates, setConfirmDuplicates] = useState<AiAnalysis[] | null>(null);
  const [pendingItems, setPendingItems] = useState<AnalyzePathInput[]>([]);

  const [cleanedBytes, setCleanedBytes] = useState(0);
  const [scanSessionTs, setScanSessionTs] = useState(() => new Date().toISOString());

  // AI 提交的清理计划（聊天里用户同意后 agent 写入 requests/，这里弹确认窗）
  const [aiCleanup, setAiCleanup] = useState<Map<string, number> | null>(null);
  // 档案馆详情里勾选的缓存 / 残留；empty 里的只清空里面（临时文件夹）
  const [archiveCleanup, setArchiveCleanup] = useState<{ items: Map<string, number>; empty: Set<string> } | null>(null);
  // 这次清理删掉的位置，清理对话框关闭后同步到档案馆的各盘摘要
  const deletedRef = useRef<[string, number][]>([]);

  // 档案馆跨盘汇总：扫描完成、打开扫描记录后重新读取
  const [archiveKey, setArchiveKey] = useState(0);
  const archive = useArchive(archiveKey);

  // macOS：启动时检测完全磁盘访问权限，缺失则弹引导窗（Windows 恒为已授权）
  const [showFdaDialog, setShowFdaDialog] = useState(false);
  useEffect(() => {
    checkFullDiskAccess()
      .then((ok) => { if (!ok) setShowFdaDialog(true); })
      .catch(() => {});
  }, []);

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
    // 档案馆里能删别的盘上的东西：记账记到路径所在的盘
    const owner = scan.drives.find((d) => isSameOrInside(normalizePath(d.mount_point), path));
    const drive = owner?.mount_point ?? scan.selectedDrive ?? scan.rootPath ?? "";
    if (drive) {
      const desc = aiAnalysis.analyses.get(path)?.description ?? "";
      saveCleanupLog(drive, path, action, bytes, scanSessionTs, desc).catch(() => {});
    }
  }, [scan.drives, scan.selectedDrive, scan.rootPath, scan.refreshDrives, scanSessionTs, aiAnalysis.analyses]);

  // 清理对话框删掉一项：记账，并从内存树摘除节点（祖先大小随之回减，不用重扫）
  const handleItemDeleted = useCallback(async (bytes: number, path: string, action: "delete" | "empty") => {
    if (bytes > 0) handleCleanupDone(bytes, path, action);
    deletedRef.current.push([path, bytes]);
    // 节点不在树里（比如已被监听刷新移除、或在别的盘上）时没有可摘的，忽略即可；清空的文件夹还在，重新统计
    if (action === "empty") await refreshScanNode(path).catch(() => {});
    else await removeScanNode(path).catch(() => {});
  }, [handleCleanupDone]);

  // 清理对话框关闭：删掉的移出选择，刷新档案馆 / 目录 / 矩形图
  const handleCleanupFinished = useCallback((removed: string[]) => {
    setSelected((prev) => {
      const next = new Map(prev);
      removed.forEach((p) => next.delete(p));
      return next;
    });
    setDialog(null);
    setAiCleanup(null);
    setArchiveCleanup(null);
    setRefreshKey((k) => k + 1);
    scan.refreshDrives();
    const deleted = deletedRef.current.splice(0);
    archive.forget(deleted);
    // 存下来的扫描也跟着更新，免得下次打开时删掉的东西又冒出来
    if (deleted.length > 0) saveSnapshot().catch(() => {});
  }, [scan.refreshDrives, archive.forget]); // eslint-disable-line react-hooks/exhaustive-deps

  const scanning = scan.status === "scanning";
  // 档案馆要等扫描完成才能归类，扫描中统一显示实时目录
  const view: ViewMode = scanning ? "tree" : viewMode;
  const footprints = useFootprints(scan.rootPath, !scanning, refreshKey);
  const hasArchive = (archive.model?.drives.length ?? 0) > 0;

  // 扫描结束、换了扫描记录：各盘摘要可能更新了
  const archiveSeen = useRef(false);
  useEffect(() => {
    if (!archiveSeen.current) {
      archiveSeen.current = true;
      return;
    }
    if (!scanning) setArchiveKey((k) => k + 1);
  }, [scanning, scan.rootPath]);

  // ---- 整理助手开场：新手引导结束、模板模式下点「让 AI 复查」 ----
  const startAssistant = useCallback(() => {
    setChatOpen(true);
    chat.startWith(ASSISTANT_KICKOFF).catch(() => {});
  }, [chat]);

  // 模板模式要复查：还没接入 AI 就先打开 AI 设置
  const reviewWithAi = useCallback(() => {
    loadAiSettings()
      .then(async (s) => {
        if (s.provider !== "claude-code" && !s.api_key.trim()) return setDialog("ai-settings");
        await archive.setMode("ai");
        startAssistant();
      })
      .catch(() => setDialog("ai-settings"));
  }, [archive, startAssistant]);

  const finishOnboarding = useCallback(async (mode: ArchiveMode) => {
    await archive.setMode(mode);
    setViewMode("archive");
    if (mode === "ai") startAssistant();
  }, [archive, startAssistant]);

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

  // Clear selections, reset cleanedBytes and start a new session when a new scan starts
  useEffect(() => {
    if (scanning) {
      setSelected(new Map());
      setCleanedBytes(0);
      setScanSessionTs(new Date().toISOString());
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
          viewMode={view}
          hasArchive={hasArchive}
          onToggleViewMode={() => setViewMode((v) => (v === "tags" ? "archive" : "tags"))}
          onSelectView={setViewMode}
          analyzing={aiAnalysis.analyzing}
          analyzeLogOpen={analyzeLogOpen}
          onToggleAnalyzeLog={() => setAnalyzeLogOpen((v) => !v)}
        />

        <ScanProgressBar pct={scanPct} visible={scanning} />

        {/* Main SplitPane Container */}
        <div className="flex-1 flex flex-col min-h-0 relative px-3 pb-2">
          {view === "archive" && !(archive.model && !hasArchive && !scan.rootPath) ? (
            <ArchiveView
              archive={archive}
              drives={scan.drives}
              scanning={scanning}
              onScanDrive={scan.scanDrive}
              onCleanup={(items, empty) => setArchiveCleanup({ items, empty })}
              onOpenAiSettings={() => setDialog("ai-settings")}
              onStartAssistant={reviewWithAi}
            />
          ) : !scanning && !scan.rootPath ? (
            <WelcomePanel
              drives={scan.drives}
              snapshots={scan.snapshots}
              onScan={scan.scanDrive}
              onOpen={(d) => { void scan.openSnapshot(d); }}
              opening={scan.opening}
              onHistory={() => setDialog("history")}
            />
          ) : (
          <SplitPane
            top={
              view === "tags" ? (
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
                footprints={footprints.items}
                key={`map-${refreshKey}`}
              />
            }
          />
          )}
        </div>

        <StatusBar scan={scan} />

        {aiAnalysis.error && (
          <div role="alert" className="absolute top-14 left-1/2 -translate-x-1/2 z-50 max-w-[70%] flex items-start gap-2 px-4 py-2.5 rounded-lg bg-card border border-slate text-[13px] shadow-[var(--fl-shadow)]">
            <AlertCircle className="w-4 h-4 mt-0.5 shrink-0 text-danger" />
            <span className="break-words">{aiAnalysis.error}</span>
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

      {/* 还没走完新手引导（第一次打开、重新开始整理之后） */}
      {archive.mode === null && (
        <Onboarding
          drives={scan.drives}
          scanned={archive.model?.drives ?? []}
          scanning={scanning}
          scanningDrive={scan.selectedDrive}
          scanPct={scanPct}
          liveChildren={scan.liveChildren}
          progress={scan.progress}
          onScanDrive={scan.scanDrive}
          recognized={archive.model?.apps.length ?? 0}
          onFinish={finishOnboarding}
        />
      )}

      {/* Dialogs */}
      {showFdaDialog && (
        <FullDiskAccessDialog onClose={() => setShowFdaDialog(false)} />
      )}
      {dialog === "cleanup" && (
        <CleanupDialog
          selected={selected}
          onDone={handleCleanupFinished}
          onClose={() => setDialog(null)}
          onCleanupDone={handleItemDeleted}
        />
      )}
      {aiCleanup && (
        <CleanupDialog
          selected={aiCleanup}
          onDone={handleCleanupFinished}
          onClose={() => setAiCleanup(null)}
          onCleanupDone={handleItemDeleted}
        />
      )}
      {archiveCleanup && (
        <CleanupDialog
          selected={archiveCleanup.items}
          emptyPaths={archiveCleanup.empty}
          onDone={handleCleanupFinished}
          onClose={() => setArchiveCleanup(null)}
          onCleanupDone={handleItemDeleted}
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
