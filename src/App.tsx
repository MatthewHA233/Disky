import { useCallback, useEffect, useState } from "react";
import { useScanSession } from "./hooks/useScanSession";
import { useChat } from "./hooks/useChat";
import { useAiAnalysis } from "./hooks/useAiAnalysis";
import { Header } from "./components/Header";
import { SplitPane } from "./components/SplitPane";
import { StatusBar } from "./components/StatusBar";
import { DirectoryTree } from "./components/DirectoryTree";
import { TreeMap } from "./components/TreeMap";
import { CleanupDialog } from "./components/CleanupDialog";
import { HistoryDialog } from "./components/HistoryDialog";
import { ChatPanel } from "./components/ChatPanel";
import { AiSettingsDialog } from "./components/AiSettingsDialog";
import { AnalyzeConfirmDialog } from "./components/AnalyzeConfirmDialog";
import { getChildren, getItemsInfo } from "./lib/invoke";
import type { AiAnalysis, AnalyzePathInput } from "./types";

type Dialog = "cleanup" | "history" | "ai-settings" | null;

export default function App() {
  const scan = useScanSession();
  const chat = useChat();
  const aiAnalysis = useAiAnalysis();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [chatOpen, setChatOpen] = useState(false);

  // Confirm dialog state
  const [confirmDuplicates, setConfirmDuplicates] = useState<AiAnalysis[] | null>(null);
  const [pendingItems, setPendingItems] = useState<AnalyzePathInput[]>([]);

  const scanning = scan.status === "scanning";

  // Shared navigation path (breadcrumb stack)
  const [navPath, setNavPath] = useState<string[]>([]);

  // Reset navPath when rootPath changes
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

  // Clear selections when a new scan starts or drive changes
  useEffect(() => {
    setSelected(new Map());
  }, [scan.selectedDrive, scanning]);

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
    <div className="app-outer">
      <div className="app-main">
        <div className="app-root">
          <Header
            scan={scan}
            onHistory={() => setDialog("history")}
            onClean={() => setDialog("cleanup")}
            cleanCount={selected.size}
            onToggleChat={() => setChatOpen((v) => !v)}
            chatOpen={chatOpen}
          />
          <SplitPane
            top={
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
              />
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
              />
            }
          />
          <StatusBar scan={scan} />
          {aiAnalysis.error && (
            <div className="ai-analysis-error">{aiAnalysis.error}</div>
          )}
        </div>
      </div>
      {chatOpen && (
        <ChatPanel
          chat={chat}
          onOpenSettings={() => setDialog("ai-settings")}
        />
      )}
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
    </div>
  );
}
