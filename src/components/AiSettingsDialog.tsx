import { useState } from "react";
import { Loader2 } from "lucide-react";
import { Dialog } from "./ui/Dialog";
import { Button } from "./ui/Button";
import { AiSettingsFields, useAiSettingsForm } from "./AiSettingsForm";

interface Props {
  onClose: () => void;
}

export function AiSettingsDialog({ onClose }: Props) {
  const form = useAiSettingsForm();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await form.save();
      onClose();
    } catch (e) {
      setError(`保存失败：${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const shown = error ?? form.loadError;
  return (
    <Dialog
      title="AI 设置"
      subtitle="档案馆的整理助手、AI 分析都用这里的配置"
      onClose={onClose}
      closable={!saving}
      width="max-w-lg"
      footer={<>
        <Button onClick={onClose} disabled={saving}>取消</Button>
        <Button variant="primary" onClick={handleSave} disabled={saving || !form.loaded}>
          {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
          {saving ? "正在保存" : "保存"}
        </Button>
      </>}
    >
      <AiSettingsFields form={form} />
      {shown && <div className="mt-5 p-3 rounded bg-danger/10 text-xs text-danger">{shown}</div>}
    </Dialog>
  );
}
