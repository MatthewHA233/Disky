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
import { getChildren, getItemsInfo } from "./lib/invoke";
import type { AiAnalysis, AnalyzePathInput, ContextMenuTarget } from "./types";

type Dialog = "cleanup" | "history" | "ai-settings" | null;
type ViewMode = "tree" | "tags";

export default function App() {
  const scan = useScanSession();
  const chat = useChat();
  const aiAnalysis = useAiAnalysis();
  const tagSystem = useTags();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [chatOpen, setChatOpen] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenuTarget | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>("tree");

  // Confirm dialog state
  const [confirmDuplicates, setConfirmDuplicates] = useState<AiAnalysis[] | null>(null);
  const [pendingItems, setPendingItems] = useState<AnalyzePathInput[]>([]);

  const scanning = scan.status === "scanning";

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
  }, []);

  // Load tags for visible paths when liveChildren change
  useEffect(() => {
    if (scan.liveChildren.length > 0) {
      const paths = scan.liveChildren.map((c) => c.path);
      tagSystem.loadTagsForPaths(paths).catch(() => {});
    }
  }, [scan.liveChildren]); // eslint-disable-line react-hooks/exhaustive-deps

  // Clear selections only when a new scan starts
  useEffect(() => {
    if (scanning) setSelected(new Map());
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
      name: info.path.split("\\").pop() || info.path,
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
    <div className="flex h-screen w-full bg-[#0D0D12] overflow-hidden text-[#FAF8F5] relative">
      {/* Sidebar / Main Content Area */}
      <div className="flex-1 flex flex-col min-w-0 h-full relative">
        <Header
          scan={scan}
          onHistory={() => setDialog("history")}
          onClean={() => setDialog("cleanup")}
          cleanCount={selected.size}
          onToggleChat={() => setChatOpen((v) => !v)}
          chatOpen={chatOpen}
          viewMode={viewMode}
          onToggleViewMode={() => setViewMode((v) => (v === "tree" ? "tags" : "tree"))}
        />

        {/* Main SplitPane Container */}
        <div className="flex-1 flex flex-col min-h-0 relative px-4 pb-0">
          <SplitPane
            top={
              viewMode === "tags" ? (
                <TagBoard
                  tags={tagSystem.tags}
                  taggedPaths={tagSystem.taggedPaths}
                  onNavigateToPath={(path) => {
                    setViewMode("tree");
                    handleNavigate(path);
                  }}
                  onRemoveTag={async (path, tagId) => {
                    await tagSystem.toggleTag(path, tagId);
                    tagSystem.loadTaggedPaths();
                  }}
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
                  onRemoveTag={(path, tagId) => tagSystem.toggleTag(path, tagId)}
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
          <div className="absolute top-20 left-1/2 -translate-x-1/2 z-50 bg-[#E74C3C]/90 text-[#FAF8F5] px-6 py-2 rounded-full shadow-lg font-mono text-sm border border-[#E74C3C]">
            {aiAnalysis.error}
          </div>
        )}
      </div>

      {chatOpen && (
        <ChatPanel
          chat={chat}
          onOpenSettings={() => setDialog("ai-settings")}
        />
      )}

      {/* Dialogs */}
      {dialog === "cleanup" && (
        <CleanupDialog
          selected={selected}
          onDone={() => { setSelected(new Map()); setDialog(null); }}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === "history" && (
        <HistoryDialog
          drive={scan.rootPath}
          onClose={() => setDialog(null)}
          onLoad={(rootPath, children) => {
            scan.loadFromHistory(rootPath, children);
            setDialog(null);
          }}
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
          tags={tagSystem.tags}
          pathTags={tagSystem.pathTags}
          onToggleTag={tagSystem.toggleTag}
          onCreateTag={tagSystem.createTag}
          onDeleteTag={tagSystem.deleteTag}
        />
      )}
    </div>
  );
}
