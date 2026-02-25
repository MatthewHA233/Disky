import { useEffect, useState } from "react";
import type { AiSettings } from "../types";
import { loadAiSettings, saveAiSettings } from "../lib/invoke";

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

  useEffect(() => {
    loadAiSettings()
      .then((s) => setSettings(s))
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await saveAiSettings(settings);
      onClose();
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
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>AI 设置</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-body">
          <label className="ai-field">
            <span className="ai-label">API Key</span>
            <input
              type="password"
              className="ai-input"
              value={settings.api_key}
              onChange={(e) => update("api_key", e.target.value)}
              placeholder="sk-..."
            />
          </label>

          <label className="ai-field">
            <span className="ai-label">Base URL</span>
            <input
              type="text"
              className="ai-input"
              value={settings.base_url}
              onChange={(e) => update("base_url", e.target.value)}
              placeholder="https://api.anthropic.com"
            />
          </label>

          <label className="ai-field">
            <span className="ai-label">Model ID</span>
            <input
              type="text"
              className="ai-input"
              value={settings.model_id}
              onChange={(e) => update("model_id", e.target.value)}
              placeholder="claude-sonnet-4-6"
            />
          </label>

          <label className="ai-field">
            <span className="ai-label">URL 模式</span>
            <select
              className="ai-input"
              value={settings.url_mode}
              onChange={(e) => update("url_mode", e.target.value)}
            >
              <option value="append">自动追加 /v1/messages</option>
              <option value="raw">直接使用 Base URL</option>
            </select>
          </label>

          {error && <div className="chat-error">{error}</div>}

          <div className="modal-actions">
            <button className="btn" onClick={onClose}>
              取消
            </button>
            <button
              className="btn btn-primary"
              onClick={handleSave}
              disabled={saving}
            >
              {saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
