import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { UseChatReturn } from "../hooks/useChat";
import { Bot, Send, Trash2, Settings, Sparkles } from "lucide-react";
import { cn } from "../lib/utils";

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
        <pre key={i} className="block w-full bg-[#0D0D12] rounded-lg p-3 my-2 overflow-x-auto border border-[#2A2A35]">
          <code className="font-mono text-xs text-[#EAE6DF]">{code}</code>
        </pre>
      );
    }
    return <span key={i} className="whitespace-pre-wrap leading-relaxed">{part}</span>;
  });
}

export function ChatPanel({ chat, onOpenSettings }: Props) {
  const [input, setInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat.messages, chat.streaming]);

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
    <div className="w-[420px] flex-shrink-0 flex flex-col bg-[#0A0A12] border-l border-[#2A2A35] h-screen shadow-[-10px_0_30px_rgba(0,0,0,0.5)] z-40 relative">
      <div className="flex items-center justify-between px-6 py-4 border-b border-[#2A2A35]/50 bg-[#13131A] shrink-0">
        <div className="flex items-center gap-3">
          <Bot className="w-5 h-5 text-[#C9A84C]" />
          <span className="font-semibold text-[#FAF8F5] tracking-wide">AI 助手</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            className="magnetic-btn p-2 rounded hover:bg-[#2A2A35]/50 text-[#888899] hover:text-[#E74C3C] transition-colors"
            onClick={chat.clear}
            disabled={chat.streaming}
            title="清空对话"
          >
            <Trash2 className="w-4 h-4" />
          </button>
          <button
            className="magnetic-btn p-2 rounded hover:bg-[#2A2A35]/50 text-[#888899] hover:text-[#FAF8F5] transition-colors"
            onClick={onOpenSettings}
            title="AI 设置"
          >
            <Settings className="w-4 h-4" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto custom-scrollbar p-6 flex flex-col gap-6">
        {chat.messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-[#888899] font-mono text-sm opacity-50">
            <Sparkles className="w-8 h-8 mb-4 opacity-50" />
            等待输入...
          </div>
        )}

        {chat.messages.map((msg, i) => (
          <div key={i} className={cn(
            "flex flex-col max-w-[90%] animate-in fade-in slide-in-from-bottom-2 duration-300",
            msg.role === "user" ? "self-end items-end" : "self-start items-start"
          )}>
            <div className={cn(
              "text-[10px] font-mono mb-1.5 uppercase tracking-widest",
              msg.role === "user" ? "text-[#C9A84C]/70" : "text-[#888899]"
            )}>
              {msg.role === "user" ? "我" : "AI"}
            </div>

            <div className={cn(
              "px-4 py-3 rounded-2xl text-[13px] shadow-sm",
              msg.role === "user"
                ? "bg-[#C9A84C] text-[#0D0D12] rounded-tr-sm font-medium"
                : "bg-[#13131A] text-[#FAF8F5] border border-[#2A2A35] rounded-tl-sm"
            )}>
              <div className="inline">
                {renderContent(msg.content)}
                {msg.id === -1 && chat.streaming && msg.content === "" && (
                  <span className="inline-flex gap-1 ml-2">
                    <span className="w-1.5 h-1.5 rounded-full bg-[#C9A84C] animate-bounce" style={{ animationDelay: "0ms" }} />
                    <span className="w-1.5 h-1.5 rounded-full bg-[#C9A84C] animate-bounce" style={{ animationDelay: "150ms" }} />
                    <span className="w-1.5 h-1.5 rounded-full bg-[#C9A84C] animate-bounce" style={{ animationDelay: "300ms" }} />
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}

        {chat.error && (
          <div className="self-center bg-[#E74C3C]/10 border border-[#E74C3C]/30 text-[#E74C3C] px-4 py-2 rounded-lg text-xs font-mono w-full text-center">
            {chat.error}
          </div>
        )}

        <div ref={bottomRef} className="h-4" />
      </div>

      <div className="p-4 bg-[#13131A] border-t border-[#2A2A35]/50 flex-shrink-0">
        <div className="relative flex items-end bg-[#0D0D12] border border-[#2A2A35] focus-within:border-[#C9A84C] rounded-2xl transition-colors shadow-inner">
          <textarea
            ref={textareaRef}
            className="w-full bg-transparent text-[#FAF8F5] text-[13px] px-4 py-3 focus:outline-none resize-none custom-scrollbar min-h-[44px] max-h-[120px]"
            placeholder="输入消息..."
            value={input}
            onChange={(e) => handleInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
            disabled={chat.streaming}
          />
          <button
            className="magnetic-btn shrink-0 w-10 h-10 m-1 flex items-center justify-center rounded-xl bg-[#2A2A35] hover:bg-[#C9A84C] text-[#FAF8F5] hover:text-[#0D0D12] disabled:opacity-30 disabled:hover:bg-[#2A2A35] disabled:hover:text-[#FAF8F5] disabled:hover:scale-100 transition-all"
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
