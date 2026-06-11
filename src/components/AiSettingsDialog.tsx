import { useEffect, useState, useRef } from "react";
import type { AiSettings, CliDetection } from "../types";
import { loadAiSettings, saveAiSettings, detectClaudeCli } from "../lib/invoke";
import gsap from "gsap";
import { Settings, Save, XCircle, Key, Globe, Cpu, Link, Layers, Terminal, CheckCircle2, AlertTriangle, Loader2, Plug } from "lucide-react";

const API_PRESETS: Record<string, { base_url: string; model_id: string }> = {
  anthropic: { base_url: "https://api.anthropic.com", model_id: "claude-sonnet-4-6" },
  openai:    { base_url: "https://api.openai.com",    model_id: "gpt-4o" },
};

const CLI_MODELS = [
  { id: "default", label: "默认（CLI 配置的模型）" },
  { id: "sonnet",  label: "Sonnet" },
  { id: "opus",    label: "Opus" },
  { id: "haiku",   label: "Haiku" },
];

interface Props {
  onClose: () => void;
}

export function AiSettingsDialog({ onClose }: Props) {
  const [settings, setSettings] = useState<AiSettings>({
    api_key: "",
    base_url: "https://api.anthropic.com",
    model_id: "claude-sonnet-4-6",
    url_mode: "append",
    api_type: "anthropic",
    provider: "api",
    cli_model: "default",
    proxy_url: "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detection, setDetection] = useState<CliDetection | null>(null);
  const [detecting, setDetecting] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (dialogRef.current) {
      gsap.fromTo(dialogRef.current,
        { opacity: 0, y: 30, scale: 0.95 },
        { opacity: 1, y: 0, scale: 1, duration: 0.4, ease: "power3.out" }
      );
    }
    loadAiSettings()
      .then((s) => {
        setSettings(s);
        if (s.provider === "claude-code") runDetection();
      })
      .catch(() => { });
  }, []);

  const runDetection = () => {
    setDetecting(true);
    setDetection(null);
    detectClaudeCli()
      .then(setDetection)
      .catch((e) => setDetection({ available: false, path: null, version: null, error: String(e) }))
      .finally(() => setDetecting(false));
  };

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

  const switchProvider = (provider: string) => {
    setSettings({ ...settings, provider });
    if (provider === "claude-code" && !detection && !detecting) {
      runDetection();
    }
  };

  const switchApiType = (newType: string) => {
    const preset = API_PRESETS[newType];
    setSettings({
      ...settings,
      api_type: newType,
      base_url: preset.base_url,
      model_id: preset.model_id,
    });
  };

  const isCli = settings.provider === "claude-code";

  return (
    <div className="fixed inset-0 bg-fiber/80 backdrop-blur-md flex flex-col items-center justify-center z-[100]" onClick={closeDialog}>
      <div
        ref={dialogRef}
        className="glass-panel w-full max-w-lg rounded-[2rem] overflow-hidden flex flex-col shadow-[0_20px_60px_-15px_rgba(0,0,0,0.8)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-8 py-6 border-b border-slate flex items-center justify-between bg-abyss">
          <div className="flex items-center gap-3">
            <Settings className="w-5 h-5 text-photon" />
            <h2 className="text-lg font-semibold text-textMain">AI 设置</h2>
          </div>
          <button className="text-text-dim hover:text-textMain transition-colors" onClick={closeDialog}>
            <XCircle className="w-6 h-6" />
          </button>
        </div>

        <div className="p-8 flex flex-col gap-6 max-h-[70vh] overflow-y-auto custom-scrollbar">
          {/* 接入方式 */}
          <div className="flex flex-col gap-2">
            <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest">
              <Plug className="w-3 h-3" /> 接入方式
            </span>
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => switchProvider("api")}
                className={`py-2 rounded-xl text-sm font-mono border transition-colors ${
                  !isCli
                    ? "bg-photon text-fiber border-photon"
                    : "bg-fiber text-text-dim border-slate hover:border-photon hover:text-textMain"
                }`}
              >
                API 直连
              </button>
              <button
                type="button"
                onClick={() => switchProvider("claude-code")}
                className={`py-2 rounded-xl text-sm font-mono border transition-colors ${
                  isCli
                    ? "bg-photon text-fiber border-photon"
                    : "bg-fiber text-text-dim border-slate hover:border-photon hover:text-textMain"
                }`}
              >
                Claude Code（本地）
              </button>
            </div>
          </div>

          {isCli ? (
            <>
              {/* CLI 检测状态 */}
              <div className="flex flex-col gap-2">
                <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest">
                  <Terminal className="w-3 h-3" /> 本地 CLI 状态
                </span>
                <div className="bg-fiber border border-slate rounded-xl px-4 py-3 flex items-start gap-3">
                  {detecting ? (
                    <>
                      <Loader2 className="w-4 h-4 mt-0.5 text-photon animate-spin shrink-0" />
                      <span className="text-sm text-text-dim font-mono">检测中...</span>
                    </>
                  ) : detection?.available ? (
                    <>
                      <CheckCircle2 className="w-4 h-4 mt-0.5 text-emerald-400 shrink-0" />
                      <div className="flex flex-col gap-1 min-w-0">
                        <span className="text-sm text-textMain font-mono">
                          Claude Code {detection.version ?? ""}
                        </span>
                        <span className="text-[11px] text-text-dim font-mono truncate" title={detection.path ?? ""}>
                          {detection.path}
                        </span>
                        <span className="text-[11px] text-text-dim/70">
                          使用本地登录的 Claude 订阅，无需 API 密钥；AI 可用只读工具（Read/Glob/Grep）实际查看磁盘内容
                        </span>
                      </div>
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="w-4 h-4 mt-0.5 text-danger shrink-0" />
                      <div className="flex flex-col gap-1.5 min-w-0">
                        <span className="text-sm text-danger font-mono">未检测到 Claude Code</span>
                        <span className="text-[11px] text-text-dim whitespace-pre-wrap">
                          {detection?.error ?? "请先安装：npm install -g @anthropic-ai/claude-code，并运行 claude 完成登录"}
                        </span>
                        <button
                          type="button"
                          onClick={runDetection}
                          className="self-start text-[11px] font-mono text-photon hover:underline"
                        >
                          重新检测
                        </button>
                      </div>
                    </>
                  )}
                </div>
              </div>

              {/* CLI 模型 */}
              <label className="flex flex-col gap-2 group">
                <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
                  <Cpu className="w-3 h-3" /> 模型
                </span>
                <select
                  className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors cursor-pointer appearance-none"
                  value={CLI_MODELS.some((m) => m.id === settings.cli_model) ? settings.cli_model : "default"}
                  onChange={(e) => update("cli_model", e.target.value)}
                >
                  {CLI_MODELS.map((m) => (
                    <option key={m.id} value={m.id}>{m.label}</option>
                  ))}
                </select>
              </label>
            </>
          ) : (
            <>
              {/* API Type */}
              <div className="flex flex-col gap-2">
                <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest">
                  <Layers className="w-3 h-3" /> API 类型
                </span>
                <div className="grid grid-cols-2 gap-2">
                  {(["anthropic", "openai"] as const).map((t) => (
                    <button
                      key={t}
                      type="button"
                      onClick={() => switchApiType(t)}
                      className={`py-2 rounded-xl text-sm font-mono border transition-colors ${
                        settings.api_type === t
                          ? "bg-photon text-fiber border-photon"
                          : "bg-fiber text-text-dim border-slate hover:border-photon hover:text-textMain"
                      }`}
                    >
                      {t === "anthropic" ? "Anthropic (Claude)" : "OpenAI 兼容"}
                    </button>
                  ))}
                </div>
              </div>

              <label className="flex flex-col gap-2 group">
                <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
                  <Key className="w-3 h-3" /> API 密钥
                </span>
                <input
                  type="password"
                  className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors"
                  value={settings.api_key}
                  onChange={(e) => update("api_key", e.target.value)}
                  placeholder="sk-..."
                />
              </label>

              <label className="flex flex-col gap-2 group">
                <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
                  <Globe className="w-3 h-3" /> 基础 URL
                </span>
                <input
                  type="text"
                  className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors"
                  value={settings.base_url}
                  onChange={(e) => update("base_url", e.target.value)}
                  placeholder="https://api.anthropic.com"
                />
              </label>

              <div className="grid grid-cols-2 gap-4">
                <label className="flex flex-col gap-2 group">
                  <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
                    <Cpu className="w-3 h-3" /> 模型
                  </span>
                  <input
                    type="text"
                    className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors"
                    value={settings.model_id}
                    onChange={(e) => update("model_id", e.target.value)}
                    placeholder="claude-sonnet-4-6"
                  />
                </label>

                <label className="flex flex-col gap-2 group">
                  <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
                    <Link className="w-3 h-3" /> 端点模式
                  </span>
                  <select
                    className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors cursor-pointer appearance-none"
                    value={settings.url_mode}
                    onChange={(e) => update("url_mode", e.target.value)}
                  >
                    <option value="append">追加 /v1/messages</option>
                    <option value="raw">直接使用基础 URL</option>
                  </select>
                </label>
              </div>
            </>
          )}

          {/* 网络代理（两种接入方式通用） */}
          <label className="flex flex-col gap-2 group">
            <span className="flex items-center gap-2 text-xs font-mono text-text-dim uppercase tracking-widest group-focus-within:text-photon transition-colors">
              <Globe className="w-3 h-3" /> 网络代理（可选）
            </span>
            <input
              type="text"
              className="w-full bg-fiber text-textMain font-mono text-sm border border-slate focus:border-photon rounded-xl px-4 py-3 outline-none transition-colors"
              value={settings.proxy_url}
              onChange={(e) => update("proxy_url", e.target.value)}
              placeholder="http://127.0.0.1:7890"
            />
            {isCli && (
              <span className="text-[11px] text-text-dim/70">
                Disky 启动的 claude 子进程不会继承终端里的代理环境变量，访问 Anthropic 受限时（403）请在此填写代理
              </span>
            )}
          </label>

          {error && (
            <div className="bg-danger/10 border border-danger/30 text-danger text-xs font-mono p-3 rounded-lg text-center">
              错误: {error}
            </div>
          )}

          <div className="flex justify-end gap-4 mt-4 pt-6 border-t border-slate/50">
            <button className="magnetic-btn px-6 py-2 rounded-full border border-slate text-text-dim hover:text-textMain hover:bg-slate/50 flex-1" onClick={closeDialog}>取消</button>
            <button className="magnetic-btn px-6 py-2 rounded-full bg-photon text-fiber hover:bg-[#D4B55C] font-semibold flex items-center justify-center gap-2 flex-[2] disabled:opacity-50 transition-colors" onClick={handleSave} disabled={saving}>
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
