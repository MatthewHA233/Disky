import { useCallback, useEffect, useState } from "react";
import type { AiSettings, CliDetection } from "../types";
import { loadAiSettings, saveAiSettings, detectClaudeCli } from "../lib/invoke";
import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import { Button } from "./ui/Button";
import { Field, RadioGroup, Select, TextInput } from "./ui/Field";

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

/** 代理默认值：本机 Clash 常用端口。打开设置时空值会自动填上，不用时清空保存即可 */
const DEFAULT_PROXY_URL = "http://127.0.0.1:7890";

/** 本地 Claude Code 的检测结果 */
function CliStatus({ detecting, detection, onRetry }: { detecting: boolean; detection: CliDetection | null; onRetry: () => void }) {
  if (detecting) {
    return (
      <div className="flex items-center gap-2 p-3 rounded bg-abyss border border-slate text-text-dim">
        <Loader2 className="w-4 h-4 animate-spin" />正在查找本机的 Claude Code
      </div>
    );
  }
  if (detection?.available) {
    return (
      <div className="flex items-start gap-2.5 p-3 rounded bg-abyss border border-slate">
        <CheckCircle2 className="w-4 h-4 mt-0.5 shrink-0 text-success" />
        <div className="min-w-0">
          <div>已找到 Claude Code {detection.version ?? ""}</div>
          <div className="truncate text-xs text-text-faint" title={detection.path ?? ""}>{detection.path}</div>
          <div className="mt-1 text-xs text-text-dim">用本机已登录的 Claude 订阅，不需要 API 密钥；AI 只能读取磁盘内容，不会改动文件。</div>
        </div>
      </div>
    );
  }
  return (
    <div className="flex items-start gap-2.5 p-3 rounded bg-danger/10">
      <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0 text-danger" />
      <div className="min-w-0">
        <div className="text-danger">没有找到 Claude Code</div>
        <div className="mt-0.5 text-xs text-text-dim whitespace-pre-wrap">
          {detection?.error ?? "先安装：npm install -g @anthropic-ai/claude-code，再运行一次 claude 完成登录。"}
        </div>
        <Button variant="subtle" className="mt-1.5 -ml-2 h-7 px-2 text-photon" onClick={onRetry}>重新查找</Button>
      </div>
    </div>
  );
}

export interface AiSettingsFormState {
  settings: AiSettings;
  update: <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => void;
  switchProvider: (provider: string) => void;
  switchApiType: (type: string) => void;
  /** 读到已保存的设置之前不允许保存，否则会用默认值（空密钥）覆盖掉原来的 */
  loaded: boolean;
  loadError: string | null;
  detection: CliDetection | null;
  detecting: boolean;
  runDetection: () => void;
  save: () => Promise<void>;
  /** 按现在填的能不能用：API 方式要有密钥，Claude Code 方式要找得到 */
  ready: boolean;
}

/** AI 设置表单的状态：AI 设置对话框和新手引导共用 */
export function useAiSettingsForm(): AiSettingsFormState {
  const [settings, setSettings] = useState<AiSettings>({
    api_key: "",
    base_url: "https://api.anthropic.com",
    model_id: "claude-sonnet-4-6",
    url_mode: "append",
    api_type: "anthropic",
    provider: "api",
    cli_model: "default",
    proxy_url: DEFAULT_PROXY_URL,
  });
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [detection, setDetection] = useState<CliDetection | null>(null);
  const [detecting, setDetecting] = useState(false);

  const runDetection = useCallback(() => {
    setDetecting(true);
    setDetection(null);
    detectClaudeCli()
      .then(setDetection)
      .catch((e) => setDetection({ available: false, path: null, version: null, error: String(e) }))
      .finally(() => setDetecting(false));
  }, []);

  useEffect(() => {
    loadAiSettings()
      .then((s) => {
        // 已保存过空代理的旧设置也预填默认值，方便一键保存启用
        setSettings({ ...s, proxy_url: s.proxy_url || DEFAULT_PROXY_URL });
        setLoaded(true);
        if (s.provider === "claude-code") runDetection();
      })
      .catch((e) => setLoadError(`读取已保存的设置失败，为避免覆盖暂时不能保存：${String(e)}`));
  }, [runDetection]);

  const update = <K extends keyof AiSettings>(key: K, value: AiSettings[K]) => setSettings((s) => ({ ...s, [key]: value }));
  const switchProvider = (provider: string) => {
    setSettings((s) => ({ ...s, provider }));
    if (provider === "claude-code" && !detection && !detecting) runDetection();
  };
  const switchApiType = (type: string) => {
    const preset = API_PRESETS[type];
    setSettings((s) => ({ ...s, api_type: type, base_url: preset.base_url, model_id: preset.model_id }));
  };
  const ready = settings.provider === "claude-code" ? !!detection?.available : settings.api_key.trim() !== "";

  return {
    settings, update, switchProvider, switchApiType, loaded, loadError, detection, detecting, runDetection, ready,
    save: () => saveAiSettings(settings),
  };
}

/** AI 设置的各个填写项 */
export function AiSettingsFields({ form }: { form: AiSettingsFormState }) {
  const { settings, update, switchProvider, switchApiType, detection, detecting, runDetection } = form;
  const isCli = settings.provider === "claude-code";
  return (
    <div className="flex flex-col gap-5">
      <RadioGroup
        label="接入方式"
        value={isCli ? "claude-code" : "api"}
        onChange={switchProvider}
        options={[
          { value: "api", label: "API 直连", description: "用你自己的 API 密钥，支持 Anthropic 与 OpenAI 兼容接口" },
          { value: "claude-code", label: "本机的 Claude Code", description: "复用已登录的 Claude 订阅，不需要密钥" },
        ]}
      />

      {isCli ? (
        <>
          <CliStatus detecting={detecting} detection={detection} onRetry={runDetection} />
          <Field label="模型">
            <Select
              value={CLI_MODELS.some((m) => m.id === settings.cli_model) ? settings.cli_model : "default"}
              onChange={(e) => update("cli_model", e.target.value)}
            >
              {CLI_MODELS.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
            </Select>
          </Field>
        </>
      ) : (
        <>
          <RadioGroup
            label="接口类型"
            inline
            value={settings.api_type === "openai" ? "openai" : "anthropic"}
            onChange={switchApiType}
            options={[
              { value: "anthropic", label: "Anthropic（Claude）" },
              { value: "openai", label: "OpenAI 兼容" },
            ]}
          />
          <Field label="API 密钥">
            <TextInput type="password" value={settings.api_key} onChange={(e) => update("api_key", e.target.value)} placeholder="sk-..." autoComplete="off" />
          </Field>
          <Field label="接口地址">
            <TextInput value={settings.base_url} onChange={(e) => update("base_url", e.target.value)} placeholder="https://api.anthropic.com" />
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field label="模型">
              <TextInput value={settings.model_id} onChange={(e) => update("model_id", e.target.value)} placeholder="claude-sonnet-4-6" />
            </Field>
            <Field label="地址拼接">
              <Select value={settings.url_mode} onChange={(e) => update("url_mode", e.target.value)}>
                <option value="append">自动追加 /v1/messages</option>
                <option value="raw">原样使用接口地址</option>
              </Select>
            </Field>
          </div>
        </>
      )}

      <Field
        label="网络代理（可选）"
        hint={isCli
          ? "Disky 启动的 claude 不会继承终端里的代理设置，访问受限（403）时在这里填写。"
          : "默认填了本机常用的代理端口；不需要代理就清空后保存。"}
      >
        <TextInput value={settings.proxy_url} onChange={(e) => update("proxy_url", e.target.value)} placeholder="http://127.0.0.1:7890" />
      </Field>
    </div>
  );
}
