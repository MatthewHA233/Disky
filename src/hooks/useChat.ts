import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChatMessage, ChatStreamEvent } from "../types";
import { listChatMessages, sendChatMessage, clearChatHistory } from "../lib/invoke";

export interface UseChatReturn {
  messages: ChatMessage[];
  streaming: boolean;
  error: string | null;
  send: (text: string) => void;
  clear: () => void;
}

export function useChat(): UseChatReturn {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamBuffer = useRef("");

  // Load history on mount
  useEffect(() => {
    listChatMessages().then(setMessages).catch(() => {});
  }, []);

  // Listen to chat-stream events
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<ChatStreamEvent>("chat-stream", (event) => {
      const { delta, done, error: errMsg } = event.payload;

      if (errMsg) {
        setError(errMsg);
        setStreaming(false);
        return;
      }

      if (delta) {
        streamBuffer.current += delta;
        const content = streamBuffer.current;
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last && last.role === "assistant" && last.id === -1) {
            return [
              ...prev.slice(0, -1),
              { ...last, content },
            ];
          }
          return prev;
        });
      }

      if (done) {
        setStreaming(false);
        // Reload from DB to get proper IDs
        listChatMessages().then(setMessages).catch(() => {});
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const send = useCallback(
    (text: string) => {
      if (streaming || !text.trim()) return;

      setError(null);
      setStreaming(true);
      streamBuffer.current = "";

      // Optimistic: add user message
      const userMsg: ChatMessage = {
        id: -2,
        role: "user",
        content: text,
        created_at: new Date().toISOString(),
      };

      // Placeholder for assistant streaming
      const assistantMsg: ChatMessage = {
        id: -1,
        role: "assistant",
        content: "",
        created_at: new Date().toISOString(),
      };

      setMessages((prev) => [...prev, userMsg, assistantMsg]);

      sendChatMessage(text).catch((err) => {
        setError(String(err));
        setStreaming(false);
      });
    },
    [streaming]
  );

  const clear = useCallback(() => {
    clearChatHistory()
      .then(() => setMessages([]))
      .catch(() => {});
  }, []);

  return { messages, streaming, error, send, clear };
}
