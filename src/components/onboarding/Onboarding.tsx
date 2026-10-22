import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, CheckCircle2, HardDrive, Loader2, Play, RotateCw, XCircle } from "lucide-react";
import { AiIcon } from "../ui/AiIcon";
import type { ArchiveDrive, ArchiveMode, DirEntry, DriveInfo, ScanProgressEvent } from "../../types";
import { formatNumber, formatSize } from "../../lib/format";
import { driveLabel, driveOf, normalizePath } from "../../lib/path";
import { testAiSettings } from "../../lib/invoke";
import { cn } from "../../lib/utils";
import { Button } from "../ui/Button";
import { AiSettingsFields, useAiSettingsForm } from "../AiSettingsForm";
import { AiArt, ScanArt, StartArt, WelcomeArt } from "./OnboardingArt";
import "./onboarding.css";

interface Props {
  /** 本机所有盘 */
  drives: DriveInfo[];
  /** 已经有扫描摘要的盘 */
  scanned: ArchiveDrive[];
  scanning: boolean;
  /** 正在扫描的盘和进度（0–100） */
  scanningDrive: string | null;
  scanPct: number;
  /** 扫描中实时更新的顶层文件夹、扫过多少文件和目录 */
  liveChildren: DirEntry[];
  progress: ScanProgressEvent | null;
  onScanDrive: (mount: string) => void;
  /** 内置规则认出了多少个软件 */
  recognized: number;
  onFinish: (mode: ArchiveMode) => Promise<void>;
}

type Step = "welcome" | "ai" | "scan" | "start";
const STEPS: { id: Step; name: string }[] = [
  { id: "welcome", name: "认识 Disky" },
  { id: "ai", name: "接入 AI" },
  { id: "scan", name: "扫描硬盘" },
  { id: "start", name: "开始" },
];

/** 结束时淡出的时长，和 onboarding.css 里的 .oobe-leave 一致 */
const LEAVE_MS = 250;

const TEMPLATE_WARNING = "模板模式只看名字来认软件和文件夹，大概率有认错、放错的地方，需要你自己检查。之后可以随时在 AI 设置里接入 AI，再查一遍。";

/** 底部的步骤圆点：当前这步拉长 */
function Pips({ at }: { at: number }) {
  return (
    <span className="flex items-center gap-1.5" role="img" aria-label={`第 ${at + 1} 步，共 ${STEPS.length} 步：${STEPS[at].name}`}>
      {STEPS.map((s, i) => (
        <i
          key={s.id}
          className={cn("oobe-pip block h-1.5 rounded-full", i === at ? "w-5 bg-photon" : i < at ? "w-1.5 bg-photon" : "w-1.5 bg-[color:var(--fl-stroke-strong)]")}
        />
      ))}
    </span>
  );
}

function Title({ children }: { children: ReactNode }) {
  return <h1 className="oobe-title text-[28px] leading-9 font-semibold">{children}</h1>;
}

function Caution({ children }: { children: ReactNode }) {
  return (
    <p className="flex gap-2 p-3.5 rounded-lg border border-caution/40 bg-caution/10 text-[13px] leading-6">
      <AlertTriangle className="w-4 h-4 shrink-0 mt-1 text-caution" />{children}
    </p>
  );
}

interface ChoiceProps {
  active: boolean;
  title: string;
  desc: string;
  icon: ReactNode;
  onPick: () => void;
}

function Choice({ active, title, desc, icon, onPick }: ChoiceProps) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onPick}
      className={cn(
        "flex-1 flex items-start gap-3 p-3.5 rounded-lg border text-left transition-colors",
        active ? "border-photon bg-photon/5" : "border-slate bg-abyss hover:bg-hover"
      )}
    >
      <span className="mt-0.5 shrink-0">{icon}</span>
      <span>
        <span className="block text-[13px] font-semibold">{title}</span>
        <span className="block mt-1 text-xs leading-5 text-text-dim">{desc}</span>
      </span>
    </button>
  );
}

function ScanRow({ drive, scanned, scanning, active, pct, onScan }: {
  drive: DriveInfo; scanned?: ArchiveDrive; scanning: boolean; active: boolean; pct: number; onScan: () => void;
}) {
  const used = drive.total_space - drive.available_space;
  return (
    <div className="flex items-center gap-3 px-3.5 py-3 rounded-lg bg-abyss border border-slate">
      <HardDrive className="w-5 h-5 shrink-0 text-text-dim" />
      <div className="flex-1 min-w-0">
        <div className="text-[13px] font-semibold">{driveLabel(driveOf(drive.mount_point))}</div>
        <div className="text-xs text-text-dim">用了 {formatSize(used)} / {formatSize(drive.total_space)}</div>
        {active && (
          <div className="mt-1.5 h-1 rounded-full bg-slate overflow-hidden">
            <i className="block h-full bg-photon transition-[width] duration-300" style={{ width: `${Math.max(3, pct)}%` }} />
          </div>
        )}
      </div>
      {active ? (
        <span className="inline-flex items-center gap-1.5 text-xs text-photon"><Loader2 className="w-3.5 h-3.5 animate-spin" />正在扫描 {Math.round(pct)}%</span>
      ) : scanned ? (
        <>
          <span className="inline-flex items-center gap-1 text-xs text-success"><CheckCircle2 className="w-3.5 h-3.5" />扫描过了</span>
          <Button variant="subtle" className="h-7 px-2 text-xs" disabled={scanning} onClick={onScan}><RotateCw className="w-3 h-3" />重新扫描</Button>
        </>
      ) : (
        <Button className="h-8" disabled={scanning} onClick={onScan}><Play className="w-3.5 h-3.5" />扫描</Button>
      )}
    </div>
  );
}

const LIVE_ROWS = 8;

/** 扫描中的实时目录：顶层文件夹按大小排，条子跟着长，和目录视图扫描时一样 */
function LiveScan({ folders, progress }: { folders: DirEntry[]; progress: ScanProgressEvent | null }) {
  const top = [...folders].sort((a, b) => b.logical_size - a.logical_size).slice(0, LIVE_ROWS);
  const max = Math.max(1, top[0]?.logical_size ?? 1);
  return (
    <div className="p-3.5 rounded-lg bg-abyss border border-slate">
      <div className="flex items-center gap-2 text-xs text-text-dim">
        <Loader2 className="w-3.5 h-3.5 animate-spin text-photon" />
        {progress
          ? <>已经看过 <b className="font-semibold text-textMain tabular-nums">{formatNumber(progress.dirs_scanned)}</b> 个文件夹、<b className="font-semibold text-textMain tabular-nums">{formatNumber(progress.files_scanned)}</b> 个文件，共 <b className="font-semibold text-textMain tabular-nums">{formatSize(progress.total_size)}</b></>
          : "正在开始"}
      </div>
      <ul className="mt-2.5 flex flex-col gap-1.5">
        {top.map((c) => (
          <li key={c.path} className="oobe-pop grid grid-cols-[minmax(0,9rem)_1fr_4.5rem] items-center gap-2.5 text-xs">
            <span className="truncate" title={c.path}>{c.name}</span>
            <span className="h-1.5 rounded-full bg-[color:var(--fl-stroke)] overflow-hidden">
              <i className="oobe-live-bar block h-full rounded-full bg-photon" style={{ width: `${Math.max(2, (c.logical_size / max) * 100)}%` }} />
            </span>
            <span className="text-right text-text-dim tabular-nums">{formatSize(c.logical_size)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** 新手引导（仿 Windows 首次开机）：左边插画、右边内容，换页时内容从右边滑进来 */
export function Onboarding({ drives, scanned, scanning, scanningDrive, scanPct, liveChildren, progress, onScanDrive, recognized, onFinish }: Props) {
  const [step, setStep] = useState<Step>("welcome");
  const [dir, setDir] = useState<"next" | "back">("next");
  const [choice, setChoice] = useState<ArchiveMode>("ai");
  const form = useAiSettingsForm();
  const [test, setTest] = useState<{ state: "idle" | "testing" | "ok" | "fail"; text: string }>({ state: "idle", text: "" });
  // 改了设置，之前的测试结果就不作数了
  useEffect(() => setTest({ state: "idle", text: "" }), [form.settings]);
  const [busy, setBusy] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scannedOf = (mount: string) => scanned.find((d) => normalizePath(d.root).toLowerCase() === normalizePath(mount).toLowerCase());
  const at = STEPS.findIndex((s) => s.id === step);
  const go = (to: Step, d: "next" | "back") => {
    setDir(d);
    setStep(to);
  };

  const runTest = async () => {
    setTest({ state: "testing", text: "" });
    try {
      const reply = await testAiSettings(form.settings);
      setTest({ state: "ok", text: reply ? `连上了，AI 回复：「${reply}」` : "连上了" });
    } catch (e) {
      setTest({ state: "fail", text: String(e) });
    }
  };

  // 结束：先淡出，再交给外面（失败了就回来显示错误）
  const finish = async (mode: ArchiveMode) => {
    setBusy(true);
    setLeaving(true);
    await new Promise((r) => setTimeout(r, LEAVE_MS));
    try {
      await onFinish(mode);
    } catch (e) {
      setLeaving(false);
      setError(String(e));
      setBusy(false);
    }
  };

  const next = async () => {
    setError(null);
    if (step === "welcome") return go("ai", "next");
    if (step === "ai") {
      if (choice === "ai") {
        try {
          await form.save();
        } catch (e) {
          return setError(`AI 设置没能保存：${String(e)}`);
        }
      }
      return go("scan", "next");
    }
    if (step === "scan") return go("start", "next");
    await finish(choice);
  };

  const canNext =
    step === "ai" ? (choice === "template" || (form.loaded && form.ready)) :
    step === "scan" ? scanned.length > 0 && !scanning :
    !busy;
  const nextLabel = step === "welcome" ? "开始设置" : step === "start" ? (choice === "ai" ? "开始整理" : "进入档案馆") : "下一步";

  const art =
    step === "welcome" ? <WelcomeArt /> :
    step === "ai" ? <AiArt /> :
    step === "scan" ? (
      <ScanArt
        done={scanned.length}
        total={Math.max(drives.length, scanned.length)}
        scanning={scanning}
        pct={scanPct}
        top={[...liveChildren].sort((a, b) => b.logical_size - a.logical_size).slice(0, 4).map((c) => c.logical_size)}
      />
    ) :
    <StartArt mode={choice} />;

  return (
    // 层级在对话框（z-100）下面：引导期间弹出的权限提示、错误提示要能看见
    <div className={cn("oobe fixed inset-0 z-[90] flex bg-fiber text-textMain", leaving && "oobe-leave")} role="dialog" aria-modal="true" aria-label="新手引导">
      <aside className="oobe-hero hidden md:flex relative w-[42%] max-w-[600px] items-center justify-center overflow-hidden border-r border-slate">
        <div className="absolute left-7 top-6 flex items-center gap-2">
          <img src="/icon.png" alt="" className="w-5 h-5" draggable={false} />
          <span className="text-sm font-semibold">Disky</span>
        </div>
        <div key={step} className="oobe-art w-full flex justify-center">{art}</div>
      </aside>

      <section className="flex-1 min-w-0 flex flex-col">
        <main className="flex-1 min-h-0 overflow-auto">
          <div key={step} data-dir={dir} className="oobe-page max-w-[600px] px-12 pt-20 pb-10 flex flex-col gap-4 text-[13px] leading-6">
            {step === "welcome" && (
              <>
                <Title>欢迎使用 Disky</Title>
                <p className="text-sm">Disky 用来弄清楚硬盘空间被谁占了。</p>
                <p className="text-text-dim">
                  它会扫一遍你的每块盘，把找到的东西分成软件、你自己的文件、开发环境和工具三类。同一个软件装在 C 盘、数据放在 D 盘，也会算在一起。
                </p>
                <p className="text-text-dim">之后你可以按自己的习惯把它们分进箱子，看还有多少没整理。哪些缓存能删，会先让 AI 核对。</p>
                <p className="text-text-dim">Disky 不会移动你的文件，删除东西一定要你自己确认。</p>
              </>
            )}

            {step === "ai" && (
              <>
                <Title>接入 AI</Title>
                <p className="text-text-dim">
                  Disky 自己只能按名字认软件，认不出的很多。接入 AI 后，它会去看那些认不出的文件夹，拿不准的在右边的助手里问你，你同意了才改。
                </p>
                <div className="flex gap-3" role="radiogroup" aria-label="整理方式">
                  <Choice
                    active={choice === "ai"}
                    title="接入 AI（推荐）"
                    desc="填 API 密钥，或者用本机已经登录的 Claude Code。"
                    icon={<AiIcon className="w-4 h-4 text-ai" />}
                    onPick={() => setChoice("ai")}
                  />
                  <Choice
                    active={choice === "template"}
                    title="模板模式"
                    desc="先不用 AI，只按内置规则整理，以后再接。"
                    icon={<AlertTriangle className="w-4 h-4 text-caution" />}
                    onPick={() => setChoice("template")}
                  />
                </div>
                {choice === "ai" ? (
                  <div className="p-4 rounded-lg bg-abyss border border-slate">
                    <AiSettingsFields form={form} />
                    <div className="mt-4 flex items-center gap-3">
                      <Button disabled={!form.ready || test.state === "testing"} onClick={runTest}>
                        {test.state === "testing" && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                        测试连接
                      </Button>
                      {test.state === "ok" && <span className="inline-flex items-center gap-1 text-xs text-success"><CheckCircle2 className="w-3.5 h-3.5" />{test.text}</span>}
                      {test.state === "fail" && <span className="inline-flex items-start gap-1 text-xs text-danger break-all"><XCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />{test.text}</span>}
                      {test.state === "idle" && !form.ready && <span className="text-xs text-text-faint">填好密钥（或找到 Claude Code）后才能继续</span>}
                    </div>
                    {form.loadError && <p className="mt-3 p-2.5 rounded bg-danger/10 text-xs text-danger">{form.loadError}</p>}
                  </div>
                ) : (
                  <Caution>{TEMPLATE_WARNING}</Caution>
                )}
              </>
            )}

            {step === "scan" && (
              <>
                <Title>扫描硬盘</Title>
                <p className="text-text-dim">
                  每块盘扫一次大概要半分钟到几分钟，扫过的会记住。至少扫一块才能继续；软件常常分在几块盘上，都扫了才算得准。
                </p>
                <div className="flex flex-col gap-2">
                  {drives.map((d) => (
                    <ScanRow
                      key={d.mount_point}
                      drive={d}
                      scanned={scannedOf(d.mount_point)}
                      scanning={scanning}
                      active={scanning && scanningDrive != null && normalizePath(scanningDrive) === normalizePath(d.mount_point)}
                      pct={scanPct}
                      onScan={() => onScanDrive(d.mount_point)}
                    />
                  ))}
                </div>
                {scanning && <LiveScan folders={liveChildren} progress={progress} />}
              </>
            )}

            {step === "start" && (
              <>
                <Title>{choice === "ai" ? "准备好了" : "先整理了一版"}</Title>
                <p className="text-text-dim">
                  扫了 {scanned.length} 块盘，按内置规则先认出了 {recognized} 个软件和工具。
                  {choice === "ai"
                    ? "点「开始整理」后右边会打开助手，它会一件一件问你，你回答了它才改。你说过的要求它会记下来。"
                    : "认出的软件已经按用途放进箱子，没认出的文件夹在左边，拖进箱子就行。"}
                </p>
                {choice === "template" && <Caution>{TEMPLATE_WARNING}</Caution>}
              </>
            )}

            {error && <p className="p-3 rounded bg-danger/10 text-xs text-danger">{error}</p>}
          </div>
        </main>

        <footer className="shrink-0 h-20 px-12 flex items-center gap-2">
          <Pips at={at} />
          {/* 扫描出问题、或者不想现在设置：先按模板进去，之后能在档案馆里「重新开始整理」 */}
          {step !== "start" && (
            <button
              type="button"
              className="ml-4 text-xs text-text-dim hover:text-textMain hover:underline disabled:opacity-50"
              disabled={busy || scanning}
              title="先按内置规则整理进入档案馆，以后可以在档案馆里重新开始整理"
              onClick={() => finish("template")}
            >
              跳过
            </button>
          )}
          <span className="ml-auto flex gap-2">
            {at > 0 && <Button disabled={busy || scanning} onClick={() => go(STEPS[at - 1].id, "back")}>上一步</Button>}
            <Button variant="primary" className="min-w-[96px]" disabled={!canNext} onClick={next}>
              {busy && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              {nextLabel}
            </Button>
          </span>
        </footer>
      </section>
    </div>
  );
}
