import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { UseChatReturn } from "../hooks/useChat";

interface Props {
  chat: UseChatReturn;
  onOpenSettings: () => void;
}

function renderContent(text: string) {
  const parts = text.split("```");
  return parts.map((part, i) => {
    if (i % 2 === 1) {
      // Code block — strip optional language tag on first line
      const lines = part.split("\n");
      const code = lines[0]?.match(/^[a-zA-Z]*$/) ? lines.slice(1).join("\n") : part;
      return (
        <pre key={i} className="chat-code-block">
          <code>{code}</code>
        </pre>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

export function ChatPanel({ chat, onOpenSettings }: Props) {
  const [input, setInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat.messages, chat.streaming]);

  const handleSend = () => {
    const text = input.trim();
    if (!text || chat.streaming) return;
    setInput("");
    chat.send(text);
    // Reset textarea height
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
    // Auto-grow textarea
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height =
        Math.min(textareaRef.current.scrollHeight, 120) + "px";
    }
  };

  return (
    <div className="chat-panel">
      <div className="chat-header">
        <span className="chat-title">AI 助手</span>
        <div className="chat-header-actions">
          <button
            className="btn chat-header-btn"
            onClick={chat.clear}
            disabled={chat.streaming}
            title="清除对话"
          >
            清除
          </button>
          <button
            className="btn chat-header-btn"
            onClick={onOpenSettings}
            title="AI 设置"
          >
            设置
          </button>
        </div>
      </div>

      <div className="chat-messages">
        {chat.messages.length === 0 && (
          <div className="chat-empty">
            扫描磁盘后，可以向 AI 助手提问关于磁盘占用的问题。
          </div>
        )}
        {chat.messages.map((msg, i) => (
          <div key={i} className={`chat-bubble chat-${msg.role}`}>
            <div className="chat-bubble-content">
              {renderContent(msg.content)}
              {msg.id === -1 && chat.streaming && msg.content === "" && (
                <span className="chat-typing">思考中...</span>
              )}
            </div>
          </div>
        ))}
        {chat.error && (
          <div className="chat-error">{chat.error}</div>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="chat-input-area">
        <textarea
          ref={textareaRef}
          className="chat-textarea"
          placeholder="输入消息... (Enter 发送, Shift+Enter 换行)"
          value={input}
          onChange={(e) => handleInput(e.target.value)}
          onKeyDown={handleKeyDown}
          rows={1}
          disabled={chat.streaming}
        />
        <button
          className="btn btn-primary chat-send-btn"
          onClick={handleSend}
          disabled={chat.streaming || !input.trim()}
        >
          发送
        </button>
      </div>
    </div>
  );
}
