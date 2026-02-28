import { useEffect, useState, useRef } from "react";
import type { AiSettings } from "../types";
import { loadAiSettings, saveAiSettings } from "../lib/invoke";
import gsap from "gsap";
import { Settings, Save, XCircle, Key, Globe, Cpu, Link } from "lucide-react";

interface Props {
  onClose: () => void;
}

export function AiSettingsDialog({ onClose }: Props) {
  const [settings, setSettings] = useState<AiSettings>({
    api_key: "",
    base_url: "https://api.anthropic.com",
    model_id: "claude-sonnet-4-6",
    url_mode: "append",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (dialogRef.current) {
      gsap.fromTo(dialogRef.current,
        { opacity: 0, y: 30, scale: 0.95 },
        { opacity: 1, y: 0, scale: 1, duration: 0.4, ease: "power3.out" }
      );
    }
    loadAiSettings()
      .then((s) => setSettings(s))
      .catch(() => { });
  }, []);

  const closeDialog = () => {
    if (dialogRef.current) {
      gsap.to(dialogRef.current, {
        opacity: 0, y: 20, scale: 0.95, duration: 0.3, ease: "power2.in", onComplete: onClose
      });
    } else {
      onClose();
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await saveAiSettings(settings);
      closeDialog();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const update = <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => {
    setSettings({ ...settings, [key]: value });
  };

  return (
    <div className="fixed inset-0 bg-[#0D0D12]/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-lg rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-6 border-b border-[#2A2A35] flex items-center justify-between bg-[#13131A]">
          <div className="flex items-center gap-3">
            <Settings className="w-5 h-5 text-[#C9A84C]" />
            <h2 className="text-lg font-semibold text-[#FAF8F5]">AI 设置</h2>
          </div>
          <button className="text-[#888899] hover:text-[#FAF8F5] transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="p-8 flex flex-col gap-6">
          <label className="flex flex-col gap-2 group">
            <span className="flex items-center gap-2 text-xs font-mono text-[#888899] uppercase tracking-widest group-focus-within:text-[#C9A84C] transition-colors">
              <Key className="w-3 h-3" /> API 密钥
            </span>
            <input
              type="password"
              className="w-full bg-[#0D0D12] text-[#FAF8F5] font-mono text-sm border border-[#2A2A35] focus:border-[#C9A84C] rounded-xl px-4 py-3 outline-none transition-colors"
              value={settings.api_key}
              onChange={(e) => update("api_key", e.target.value)}
              placeholder="sk-..."
            />
          </label>

          <label className="flex flex-col gap-2 group">
            <span className="flex items-center gap-2 text-xs font-mono text-[#888899] uppercase tracking-widest group-focus-within:text-[#C9A84C] transition-colors">
              <Globe className="w-3 h-3" /> 基础 URL
            </span>
            <input
              type="text"
              className="w-full bg-[#0D0D12] text-[#FAF8F5] font-mono text-sm border border-[#2A2A35] focus:border-[#C9A84C] rounded-xl px-4 py-3 outline-none transition-colors"
              value={settings.base_url}
              onChange={(e) => update("base_url", e.target.value)}
              placeholder="https://api.anthropic.com"
            />
          </label>

          <div className="grid grid-cols-2 gap-4">
            <label className="flex flex-col gap-2 group">
              <span className="flex items-center gap-2 text-xs font-mono text-[#888899] uppercase tracking-widest group-focus-within:text-[#C9A84C] transition-colors">
                <Cpu className="w-3 h-3" /> 模型
              </span>
              <input
                type="text"
                className="w-full bg-[#0D0D12] text-[#FAF8F5] font-mono text-sm border border-[#2A2A35] focus:border-[#C9A84C] rounded-xl px-4 py-3 outline-none transition-colors"
                value={settings.model_id}
                onChange={(e) => update("model_id", e.target.value)}
                placeholder="claude-sonnet-4-6"
              />
            </label>

            <label className="flex flex-col gap-2 group">
              <span className="flex items-center gap-2 text-xs font-mono text-[#888899] uppercase tracking-widest group-focus-within:text-[#C9A84C] transition-colors">
                <Link className="w-3 h-3" /> 端点模式
              </span>
              <select
                className="w-full bg-[#0D0D12] text-[#FAF8F5] font-mono text-sm border border-[#2A2A35] focus:border-[#C9A84C] rounded-xl px-4 py-3 outline-none transition-colors cursor-pointer appearance-none"
                value={settings.url_mode}
                onChange={(e) => update("url_mode", e.target.value)}
              >
                <option value="append">追加 /v1/messages</option>
                <option value="raw">直接使用基础 URL</option>
              </select>
            </label>
          </div>

          {error && (
            <div className="bg-[#E74C3C]/10 border border-[#E74C3C]/30 text-[#E74C3C] text-xs font-mono p-3 rounded-lg text-center">
              错误: {error}
            </div>
          )}

          <div className="flex justify-end gap-4 mt-4 pt-6 border-t border-[#2A2A35]/50">
            <button className="magnetic-btn px-6 py-2 rounded-full border border-[#2A2A35] text-[#888899] hover:text-[#FAF8F5] hover:bg-[#2A2A35]/50 flex-1" onClick={closeDialog}>取消</button>
            <button className="magnetic-btn px-6 py-2 rounded-full bg-[#C9A84C] text-[#0D0D12] hover:bg-[#D4B55C] font-semibold flex items-center justify-center gap-2 flex-[2] disabled:opacity-50 transition-colors" onClick={handleSave} disabled={saving}>
              {saving ? (
                <span className="flex items-center gap-2 animate-pulse"><Save className="w-4 h-4" /> 保存中...</span>
              ) : (
                <span className="flex items-center gap-2"><Save className="w-4 h-4" /> 保存设置</span>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
