import { useEffect, useRef, useState, type KeyboardEvent, type ReactElement, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { UseChatReturn } from "../hooks/useChat";
import type { ChatSegment } from "../types";
import { Bot, Send, Trash2, Settings, Sparkles, Brain, Wrench, Check, X, Plus, MessagesSquare, RefreshCw } from "lucide-react";
import { cn } from "../lib/utils";
import { IS_WINDOWS } from "../lib/path";

interface Props {
  chat: UseChatReturn;
  onOpenSettings: () => void;
  /** 点击 AI 回复中的磁盘路径 → 左侧目录树/TreeMap 跳转 */
  onNavigatePath?: (path: string) => void;
}

const WIN_PATH_RE = /[A-Za-z]:\\[^\s"'`<>|?*，。；：！？()（）[\]]+/g;
// macOS：限定常见根目录开头，避免把普通含斜杠文本（如 "输入/输出"）误判成路径
const MAC_PATH_RE = /\/(?:Users|Applications|Library|System|Volumes|private|opt|usr|var|tmp|etc)\/[^\s"'`<>|?*，。；：！？()（）[\]]+/g;
const PATH_RE = IS_WINDOWS ? WIN_PATH_RE : MAC_PATH_RE;
// 工具输入摘要是否是一个可跳转的绝对路径
const ABS_PATH_TEST = IS_WINDOWS ? /^[A-Za-z]:\\/ : /^\//;

/** 把文本里的磁盘绝对路径渲染成可点击跳转的链接 */
function linkifyPaths(text: string, key: number, onNavigate?: (p: string) => void) {
  if (!onNavigate) {
    return <span key={key} className="whitespace-pre-wrap leading-relaxed">{text}</span>;
  }
  const nodes: (string | ReactElement)[] = [];
  const re = new RegExp(PATH_RE.source, "g");
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) nodes.push(text.slice(last, m.index));
    // 去掉误吞的结尾标点
    const raw = m[0];
    const path = raw.replace(/[.,;:。，；：、]+$/, "");
    // 用真正的行内 span（button 是 inline-block，跨行时基线错位会把列表序号挤乱）
    nodes.push(
      <span
        key={`${key}-${m.index}`}
        role="button"
        tabIndex={0}
        className="text-photon hover:underline decoration-photon/50 break-words cursor-pointer"
        title="在目录树中查看"
        onClick={() => onNavigate(path)}
      >
        {path}
      </span>
    );
    if (path.length < raw.length) nodes.push(raw.slice(path.length));
    last = m.index + raw.length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return (
    <span key={key} className="whitespace-pre-wrap leading-relaxed">
      {nodes}
    </span>
  );
}

/** 递归处理 markdown 元素的子节点，把其中的字符串做路径 linkify */
function linkifyChildren(children: ReactNode, onNavigate?: (p: string) => void): ReactNode {
  if (!onNavigate) return children;
  let seq = 0;
  const walk = (node: ReactNode): ReactNode => {
    if (typeof node === "string") return linkifyPaths(node, seq++, onNavigate);
    if (Array.isArray(node)) return node.map(walk);
    return node;
  };
  return walk(children);
}

/** AI 回复的 markdown 渲染组件映射（含 GFM 表格），暗色样式 + 路径可点击 */
function markdownComponents(onNavigate?: (p: string) => void): Components {
  const wrap = (children: ReactNode) => linkifyChildren(children, onNavigate);
  return {
    p: ({ children }) => <p className="my-1.5 leading-relaxed break-words">{wrap(children)}</p>,
    ul: ({ children }) => <ul className="list-disc pl-5 my-1.5 space-y-1">{children}</ul>,
    ol: ({ children }) => <ol className="list-decimal pl-5 my-1.5 space-y-1">{children}</ol>,
    li: ({ children }) => <li className="break-words">{wrap(children)}</li>,
    h1: ({ children }) => <div className="text-sm font-bold text-textMain mt-3 mb-1.5">{wrap(children)}</div>,
    h2: ({ children }) => <div className="text-sm font-bold text-textMain mt-3 mb-1.5">{wrap(children)}</div>,
    h3: ({ children }) => <div className="text-[13px] font-semibold text-textMain mt-2.5 mb-1">{wrap(children)}</div>,
    h4: ({ children }) => <div className="text-[13px] font-semibold text-text-dim mt-2 mb-1">{wrap(children)}</div>,
    // Notion 风简单表格：只有横向分隔线；路径等长内容不压缩列宽，超宽时整表横向滚动
    table: ({ children }) => (
      <div className="overflow-x-auto my-2 custom-scrollbar">
        <table className="text-xs border-collapse w-full">{children}</table>
      </div>
    ),
    th: ({ children }) => (
      <th className="px-3 py-1.5 text-left font-medium text-text-dim border-b border-slate whitespace-nowrap">
        {wrap(children)}
      </th>
    ),
    td: ({ children }) => (
      <td className="px-3 py-1.5 align-top border-b border-slate/40">{wrap(children)}</td>
    ),
    pre: ({ children }) => (
      <pre className="block w-full bg-fiber rounded-lg p-3 my-2 overflow-x-auto border border-slate text-xs font-mono custom-scrollbar">
        {children}
      </pre>
    ),
    code: ({ children, className }) =>
      className ? (
        // 代码块内的 <code>（带语言 class），样式交给外层 pre
        <code className="font-mono text-border">{children}</code>
      ) : (
        // break-words 而非 break-all：不影响表格列的固有宽度（避免列被压成一字一行），
        // 段落里超长路径仍会在溢出处折行
        <code className="bg-fiber px-1 py-0.5 rounded font-mono text-[12px] text-photon/90 break-words">
          {wrap(children)}
        </code>
      ),
    strong: ({ children }) => <strong className="font-semibold text-textMain">{wrap(children)}</strong>,
    blockquote: ({ children }) => (
      <blockquote className="border-l-2 border-photon/40 pl-3 my-1.5 text-text-dim">{children}</blockquote>
    ),
    hr: () => <hr className="border-slate/50 my-2" />,
    // Tauri webview 里不让外链接管窗口，纯文本展示
    a: ({ children }) => <span className="text-photon/90 underline decoration-photon/30">{wrap(children)}</span>,
  };
}

function parseMetaSegments(meta: string | null | undefined): ChatSegment[] {
  if (!meta) return [];
  try {
    const parsed = JSON.parse(meta);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function toolInputSummary(input: string): string {
  try {
    const v = JSON.parse(input);
    for (const key of ["file_path", "path", "pattern", "command", "query"]) {
      if (typeof v[key] === "string") return v[key];
    }
  } catch { /* 非 JSON 输入直接展示原文 */ }
  return input.length > 120 ? input.slice(0, 120) + "…" : input;
}

function SegmentList({
  segments,
  streaming,
  onNavigate,
}: {
  segments: ChatSegment[];
  streaming?: boolean;
  onNavigate?: (p: string) => void;
}) {
  if (segments.length === 0) return null;
  return (
    <div className="flex flex-col gap-1.5 mb-2 min-w-0 w-full">
      {segments.map((seg, i) =>
        seg.type === "thinking" ? (
          <details key={i} className="bg-fiber/60 border border-slate/50 rounded-lg px-3 py-1.5 min-w-0">
            <summary className="flex items-center gap-1.5 text-[11px] font-mono text-text-dim cursor-pointer select-none list-none">
              <Brain className="w-3 h-3 shrink-0" /> 思考过程
            </summary>
            <div className="mt-1.5 text-[11px] text-text-dim/80 italic whitespace-pre-wrap break-words leading-relaxed max-h-40 overflow-y-auto custom-scrollbar">
              {seg.text}
            </div>
          </details>
        ) : (
          <div key={i} className="flex items-center gap-2 bg-fiber/60 border border-slate/50 rounded-lg px-3 py-1.5 text-[11px] font-mono min-w-0">
            <Wrench className="w-3 h-3 shrink-0 text-photon/70" />
            <span className="text-photon/90 shrink-0">{seg.name}</span>
            {(() => {
              const summary = toolInputSummary(seg.input);
              const isPath = ABS_PATH_TEST.test(summary);
              return isPath && onNavigate ? (
                <button
                  type="button"
                  className="text-text-dim truncate flex-1 min-w-0 text-left hover:text-photon hover:underline"
                  title="在目录树中查看"
                  onClick={() => onNavigate(summary)}
                >
                  {summary}
                </button>
              ) : (
                <span className="text-text-dim truncate flex-1 min-w-0" title={summary}>
                  {summary}
                </span>
              );
            })()}
            {seg.result !== undefined && seg.result !== null ? (
              seg.is_error ? (
                <X className="w-3 h-3 shrink-0 text-danger" />
              ) : (
                <Check className="w-3 h-3 shrink-0 text-emerald-400" />
              )
            ) : streaming ? (
              <span className="w-2 h-2 shrink-0 rounded-full bg-photon animate-pulse" />
            ) : null}
          </div>
        )
      )}
    </div>
  );
}

function renderContent(text: string, onNavigate?: (p: string) => void) {
  const parts = text.split("```");
  return parts.map((part, i) => {
    if (i % 2 === 1) {
      // Code block — strip optional language tag on first line
      const lines = part.split("\n");
      const code = lines[0]?.match(/^[a-zA-Z]*$/) ? lines.slice(1).join("\n") : part;
      return (
        <pre key={i} className="block w-full bg-fiber rounded-lg p-3 my-2 overflow-x-auto border border-slate">
          <code className="font-mono text-xs text-border">{code}</code>
        </pre>
      );
    }
    return linkifyPaths(part, i, onNavigate);
  });
}

export function ChatPanel({ chat, onOpenSettings, onNavigatePath }: Props) {
  const [input, setInput] = useState("");
  const [showSessions, setShowSessions] = useState(false);
  // 删除会话的两段式确认：第一次点击进入确认态（变红），3 秒内再点才执行
  const [confirmDeleteId, setConfirmDeleteId] = useState<number | null>(null);
  const confirmTimer = useRef<number | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const activeSession = chat.sessions.find((s) => s.id === chat.activeSessionId);
  const activeTitle = activeSession?.title?.trim() || "新对话";

  const requestDelete = (id: number) => {
    if (confirmTimer.current) window.clearTimeout(confirmTimer.current);
    if (confirmDeleteId === id) {
      setConfirmDeleteId(null);
      chat.deleteSession(id);
      return;
    }
    setConfirmDeleteId(id);
    confirmTimer.current = window.setTimeout(() => setConfirmDeleteId(null), 3000);
  };

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat.messages, chat.streaming, chat.liveSegments]);

  const handleSend = () => {
    const text = input.trim();
    if (!text || chat.streaming) return;
    setInput("");
    chat.send(text);
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInput = (value: string) => {
    setInput(value);
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height =
        Math.min(textareaRef.current.scrollHeight, 120) + "px";
    }
  };

  return (
    <div className="w-[420px] flex-shrink-0 flex flex-col bg-[#0A0A12] border-l border-slate h-screen shadow-[-10px_0_30px_rgba(0,0,0,0.5)] z-40 relative">
      <div className="flex items-center justify-between px-6 py-4 border-b border-slate/50 bg-abyss shrink-0 relative">
        <button
          className="flex items-center gap-2.5 min-w-0 group"
          onClick={() => setShowSessions((v) => !v)}
          title="切换会话"
        >
          <Bot className="w-5 h-5 text-photon shrink-0" />
          <span className="font-semibold text-textMain tracking-wide truncate max-w-[200px] group-hover:text-photon transition-colors">
            {activeTitle}
          </span>
          <MessagesSquare className="w-3.5 h-3.5 text-text-dim shrink-0" />
        </button>
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            className="magnetic-btn p-2 rounded hover:bg-slate/50 text-text-dim hover:text-photon transition-colors"
            onClick={() => { chat.newSession(); setShowSessions(false); }}
            disabled={chat.streaming}
            title="新对话"
          >
            <Plus className="w-4 h-4" />
          </button>
          <button
            className={cn(
              "magnetic-btn p-2 rounded transition-colors",
              confirmDeleteId != null && confirmDeleteId === chat.activeSessionId
                ? "bg-danger/20 text-danger animate-pulse"
                : "hover:bg-slate/50 text-text-dim hover:text-danger"
            )}
            onClick={() => chat.activeSessionId != null && requestDelete(chat.activeSessionId)}
            disabled={chat.streaming}
            title={
              confirmDeleteId != null && confirmDeleteId === chat.activeSessionId
                ? "再点一次确认删除当前会话"
                : "删除当前会话"
            }
          >
            <Trash2 className="w-4 h-4" />
          </button>
          <button
            className="magnetic-btn p-2 rounded hover:bg-slate/50 text-text-dim hover:text-textMain transition-colors"
            onClick={onOpenSettings}
            title="AI 设置"
          >
            <Settings className="w-4 h-4" />
          </button>
        </div>

        {showSessions && (
          <>
            <div className="fixed inset-0 z-40" onClick={() => setShowSessions(false)} />
            <div className="absolute left-4 top-full mt-1 w-[340px] max-h-[60vh] overflow-y-auto custom-scrollbar bg-abyss border border-slate rounded-xl shadow-2xl z-50 py-1.5">
              <div className="px-3 py-1.5 text-[10px] font-mono text-text-dim uppercase tracking-widest">会话历史</div>
              {chat.sessions.length === 0 && (
                <div className="px-3 py-2 text-xs text-text-dim/60">暂无会话</div>
              )}
              {chat.sessions.map((s) => (
                <div
                  key={s.id}
                  className={cn(
                    "group flex items-center gap-2 px-3 py-2 cursor-pointer transition-colors",
                    s.id === chat.activeSessionId ? "bg-photon/10" : "hover:bg-[#FFFFFF]/5"
                  )}
                  onClick={() => { chat.switchSession(s.id); setShowSessions(false); }}
                >
                  <MessagesSquare className={cn("w-3.5 h-3.5 shrink-0", s.id === chat.activeSessionId ? "text-photon" : "text-text-dim")} />
                  <span className={cn("flex-1 min-w-0 truncate text-[13px]", s.id === chat.activeSessionId ? "text-textMain" : "text-text-dim")}>
                    {s.title?.trim() || "新对话"}
                  </span>
                  {(() => {
                    const regen = chat.regeneratingIds.has(s.id);
                    return (
                      <button
                        className={cn(
                          "p-1 rounded shrink-0 transition-all",
                          regen
                            ? "opacity-100 text-photon cursor-default"
                            : "opacity-0 group-hover:opacity-100 hover:bg-slate/50 text-text-dim hover:text-photon"
                        )}
                        onClick={(e) => { e.stopPropagation(); if (!regen) chat.regenerateTitle(s.id); }}
                        disabled={regen}
                        title={regen ? "标题生成中..." : "重新生成标题"}
                      >
                        <RefreshCw className={cn("w-3 h-3", regen && "animate-spin")} />
                      </button>
                    );
                  })()}
                  {confirmDeleteId === s.id ? (
                    <button
                      className="px-1.5 py-0.5 rounded bg-danger/20 text-danger text-[10px] font-mono shrink-0 animate-pulse"
                      onClick={(e) => { e.stopPropagation(); requestDelete(s.id); }}
                      title="再点一次确认删除"
                    >
                      确认?
                    </button>
                  ) : (
                    <button
                      className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-slate/50 text-text-dim hover:text-danger transition-all shrink-0"
                      onClick={(e) => { e.stopPropagation(); requestDelete(s.id); }}
                      title="删除会话"
                    >
                      <X className="w-3 h-3" />
                    </button>
                  )}
                </div>
              ))}
            </div>
          </>
        )}
      </div>

      <div className="flex-1 overflow-y-auto overflow-x-hidden custom-scrollbar p-6 flex flex-col gap-6">
        {chat.messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-text-dim font-mono text-sm opacity-50">
            <Sparkles className="w-8 h-8 mb-4 opacity-50" />
            等待输入...
          </div>
        )}

        {chat.messages.map((msg, i) => (
          <div key={i} className={cn(
            "flex flex-col max-w-[90%] min-w-0 animate-in fade-in slide-in-from-bottom-2 duration-300",
            msg.role === "user" ? "self-end items-end" : "self-start items-start"
          )}>
            <div className={cn(
              "text-[10px] font-mono mb-1.5 uppercase tracking-widest",
              msg.role === "user" ? "text-photon/70" : "text-text-dim"
            )}>
              {msg.role === "user" ? "我" : "AI"}
            </div>

            <div className={cn(
              "px-4 py-3 rounded-2xl text-[13px] shadow-sm max-w-full min-w-0 overflow-hidden",
              msg.role === "user"
                ? "bg-photon text-fiber rounded-tr-sm font-medium"
                : "bg-abyss text-textMain border border-slate rounded-tl-sm w-full"
            )}>
              {msg.role === "assistant" && (
                <SegmentList
                  segments={msg.id === -1 ? chat.liveSegments : parseMetaSegments(msg.meta)}
                  streaming={msg.id === -1 && chat.streaming}
                  onNavigate={onNavigatePath}
                />
              )}
              {msg.role === "assistant" ? (
                <div className="break-words">
                  <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents(onNavigatePath)}>
                    {msg.content}
                  </ReactMarkdown>
                  {msg.id === -1 && chat.streaming && msg.content === "" && (
                    <span className="inline-flex gap-1">
                      <span className="w-1.5 h-1.5 rounded-full bg-photon animate-bounce" style={{ animationDelay: "0ms" }} />
                      <span className="w-1.5 h-1.5 rounded-full bg-photon animate-bounce" style={{ animationDelay: "150ms" }} />
                      <span className="w-1.5 h-1.5 rounded-full bg-photon animate-bounce" style={{ animationDelay: "300ms" }} />
                    </span>
                  )}
                </div>
              ) : (
                <div className="inline">{renderContent(msg.content)}</div>
              )}
              {msg.id === -1 && chat.streaming && chat.status && (
                <div className="mt-1.5 text-[10px] font-mono text-text-dim/70 animate-pulse">
                  {chat.status}
                </div>
              )}
            </div>
          </div>
        ))}

        {chat.error && (
          <div className="self-center bg-danger/10 border border-danger/30 text-danger px-4 py-2 rounded-lg text-xs font-mono w-full text-center">
            {chat.error}
          </div>
        )}

        <div ref={bottomRef} className="h-4" />
      </div>

      <div className="p-4 bg-abyss border-t border-slate/50 flex-shrink-0">
        <div className="relative flex items-end bg-fiber border border-slate focus-within:border-photon rounded-2xl transition-colors shadow-inner">
          <textarea
            ref={textareaRef}
            className="w-full bg-transparent text-textMain text-[13px] px-4 py-3 focus:outline-none resize-none custom-scrollbar min-h-[44px] max-h-[120px]"
            placeholder="输入消息..."
            value={input}
            onChange={(e) => handleInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
            disabled={chat.streaming}
          />
          <button
            className="magnetic-btn shrink-0 w-10 h-10 m-1 flex items-center justify-center rounded-xl bg-slate hover:bg-photon text-textMain hover:text-fiber disabled:opacity-30 disabled:hover:bg-slate disabled:hover:text-textMain disabled:hover:scale-100 transition-all"
            onClick={handleSend}
            disabled={chat.streaming || !input.trim()}
          >
            <Send className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
}
