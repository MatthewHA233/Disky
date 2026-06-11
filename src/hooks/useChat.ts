import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChatMessage, ChatSegment, ChatSession, ChatStreamEvent } from "../types";
import {
  listChatMessages,
  sendChatMessage,
  listChatSessions,
  createChatSession,
  deleteChatSession,
  regenerateChatTitle,
} from "../lib/invoke";

export interface UseChatReturn {
  sessions: ChatSession[];
  activeSessionId: number | null;
  messages: ChatMessage[];
  streaming: boolean;
  error: string | null;
  /** 流式进行中的思考/工具调用片段（Claude Code provider） */
  liveSegments: ChatSegment[];
  /** CLI 状态行，如「已连接 Claude Code (sonnet)」 */
  status: string | null;
  send: (text: string) => void;
  switchSession: (id: number) => void;
  newSession: () => void;
  deleteSession: (id: number) => void;
  regenerateTitle: (id: number) => void;
  /** 正在重新生成标题的会话 id 集合（按钮转圈/禁点用） */
  regeneratingIds: Set<number>;
}

export function useChat(): UseChatReturn {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [liveSegments, setLiveSegments] = useState<ChatSegment[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [regeneratingIds, setRegeneratingIds] = useState<Set<number>>(new Set());
  const streamBuffer = useRef("");
  const segmentsRef = useRef<ChatSegment[]>([]);
  // 当前流式归属的会话——避免在 A 会话流式时切到 B 把增量写错地方
  const activeRef = useRef<number | null>(null);
  activeRef.current = activeSessionId;

  const loadMessages = useCallback((sessionId: number) => {
    listChatMessages(sessionId).then(setMessages).catch(() => setMessages([]));
  }, []);

  // 初始化：加载会话列表，没有则建一个，激活最近的
  useEffect(() => {
    listChatSessions()
      .then(async (list) => {
        if (list.length === 0) {
          const s = await createChatSession();
          setSessions([s]);
          setActiveSessionId(s.id);
          setMessages([]);
        } else {
          setSessions(list);
          setActiveSessionId(list[0].id);
          loadMessages(list[0].id);
        }
      })
      .catch(() => {});
  }, [loadMessages]);

  // 标题更新事件 → 刷新会话列表
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    listen<number>("chat-session-updated", () => {
      listChatSessions().then(setSessions).catch(() => {});
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  // chat-stream 事件
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let disposed = false;

    listen<ChatStreamEvent>("chat-stream", (event) => {
      const payload = event.payload;
      const { delta, done, error: errMsg, kind } = payload;

      if (errMsg) {
        setError(errMsg);
        setStreaming(false);
        setStatus(null);
        return;
      }

      switch (kind) {
        case "status":
          setStatus(payload.status ?? null);
          break;
        case "thinking": {
          const segs = segmentsRef.current;
          const last = segs[segs.length - 1];
          if (last && last.type === "thinking") {
            segmentsRef.current = [...segs.slice(0, -1), { ...last, text: last.text + delta }];
          } else {
            segmentsRef.current = [...segs, { type: "thinking", text: delta }];
          }
          setLiveSegments(segmentsRef.current);
          break;
        }
        case "tool_use":
          segmentsRef.current = [
            ...segmentsRef.current,
            {
              type: "tool",
              id: payload.tool_id ?? "",
              name: payload.tool_name ?? "?",
              input: payload.tool_input ?? "",
              is_error: false,
            },
          ];
          setLiveSegments(segmentsRef.current);
          break;
        case "tool_result":
          segmentsRef.current = segmentsRef.current.map((seg) =>
            seg.type === "tool" && seg.id === payload.tool_id
              ? { ...seg, result: payload.tool_result ?? "", is_error: payload.tool_error ?? false }
              : seg
          );
          setLiveSegments(segmentsRef.current);
          break;
        default:
          if (delta) {
            streamBuffer.current += delta;
            const content = streamBuffer.current;
            setMessages((prev) => {
              const last = prev[prev.length - 1];
              if (last && last.role === "assistant" && last.id === -1) {
                return [...prev.slice(0, -1), { ...last, content }];
              }
              return prev;
            });
          }
      }

      if (done) {
        setStreaming(false);
        setStatus(null);
        segmentsRef.current = [];
        setLiveSegments([]);
        const sid = activeRef.current;
        if (sid != null) loadMessages(sid);
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [loadMessages]);

  const send = useCallback(
    (text: string) => {
      if (streaming || !text.trim() || activeSessionId == null) return;

      setError(null);
      setStreaming(true);
      setStatus(null);
      streamBuffer.current = "";
      segmentsRef.current = [];
      setLiveSegments([]);

      const userMsg: ChatMessage = {
        id: -2,
        role: "user",
        content: text,
        created_at: new Date().toISOString(),
      };
      const assistantMsg: ChatMessage = {
        id: -1,
        role: "assistant",
        content: "",
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, userMsg, assistantMsg]);

      sendChatMessage(text, activeSessionId).catch((err) => {
        setError(String(err));
        setStreaming(false);
        setStatus(null);
      });
    },
    [streaming, activeSessionId]
  );

  const switchSession = useCallback(
    (id: number) => {
      if (id === activeSessionId || streaming) return;
      setActiveSessionId(id);
      setError(null);
      loadMessages(id);
    },
    [activeSessionId, streaming, loadMessages]
  );

  const newSession = useCallback(() => {
    if (streaming) return;
    createChatSession()
      .then((s) => {
        setSessions((prev) => [s, ...prev]);
        setActiveSessionId(s.id);
        setMessages([]);
        setError(null);
      })
      .catch(() => {});
  }, [streaming]);

  const deleteSession = useCallback(
    (id: number) => {
      if (streaming) return;
      deleteChatSession(id)
        .then(async () => {
          const remaining = sessions.filter((s) => s.id !== id);
          if (remaining.length === 0) {
            const s = await createChatSession();
            setSessions([s]);
            setActiveSessionId(s.id);
            setMessages([]);
          } else {
            setSessions(remaining);
            if (id === activeSessionId) {
              setActiveSessionId(remaining[0].id);
              loadMessages(remaining[0].id);
            }
          }
        })
        .catch(() => {});
    },
    [streaming, sessions, activeSessionId, loadMessages]
  );

  const regenerateTitle = useCallback((id: number) => {
    setRegeneratingIds((prev) => {
      if (prev.has(id)) return prev; // 已在生成中，忽略重复点击
      const next = new Set(prev);
      next.add(id);
      return next;
    });
    regenerateChatTitle(id)
      .catch(() => {})
      .finally(() => {
        setRegeneratingIds((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
        // 失败保留原标题时后端不发事件，这里兜底刷一次列表
        listChatSessions().then(setSessions).catch(() => {});
      });
  }, []);

  return {
    sessions,
    activeSessionId,
    messages,
    streaming,
    error,
    liveSegments,
    status,
    send,
    switchSession,
    newSession,
    deleteSession,
    regenerateTitle,
    regeneratingIds,
  };
}
