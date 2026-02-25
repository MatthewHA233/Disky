import { useEffect, useState } from "react";
import { useScanSession } from "./hooks/useScanSession";
import { useChat } from "./hooks/useChat";
import { Header } from "./components/Header";
import { SplitPane } from "./components/SplitPane";
import { StatusBar } from "./components/StatusBar";
import { DirectoryTree } from "./components/DirectoryTree";
import { TreeMap } from "./components/TreeMap";
import { CleanupDialog } from "./components/CleanupDialog";
import { HistoryDialog } from "./components/HistoryDialog";
import { ChatPanel } from "./components/ChatPanel";
import { AiSettingsDialog } from "./components/AiSettingsDialog";

type Dialog = "cleanup" | "history" | "ai-settings" | null;

export default function App() {
  const scan = useScanSession();
  const chat = useChat();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [selected, setSelected] = useState<Map<string, number>>(new Map());
  const [chatOpen, setChatOpen] = useState(false);

  const scanning = scan.status === "scanning";

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
              />
            }
            bottom={
              <TreeMap
                rootPath={scan.rootPath}
                liveChildren={scan.liveChildren}
                scanning={scanning}
              />
            }
          />
          <StatusBar scan={scan} />
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
    </div>
  );
}
