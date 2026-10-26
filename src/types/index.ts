export interface DirEntry {
  name: string;
  path: string;
  files: number;
  subdirs: number;
  logical_size: number;
  is_dir: boolean;
}

/** 档案馆「性质」透镜的分类 */
export type FootprintKind = "prog" | "data" | "cache" | "regen" | "residue" | "system";

/** classify_footprints 的结果：一块被判定了性质、归到某个软件名下的空间 */
export interface Footprint {
  path: string;
  name: string;
  size: number;
  kind: FootprintKind;
  owner: string;
  is_dir: boolean;
  /** true：所在位置里没被单独认出的其余文件，path 指向那个位置 */
  remainder: boolean;
}

/** get_children_page 的分页结果（按大小降序） */
export interface ChildrenPage {
  total: number;
  parent_size: number;
  entries: DirEntry[];
}

export interface DriveInfo {
  mount_point: string;
  total_space: number;
  available_space: number;
}

export interface ScanProgress {
  status: "idle" | "scanning" | "done" | "error";
  message: string;
}

export interface ScanProgressEvent {
  files_scanned: number;
  dirs_scanned: number;
  total_size: number;
}

export interface ItemInfo {
  path: string;
  size: number;
  is_dir: boolean;
}

export interface DeleteResult {
  path: string;
  success: boolean;
  error: string | null;
}

export interface ScanRecord {
  id: number;
  drive: string;
  entry_count: number;
  created_at: string;
  has_tree: boolean;
  cleaned_bytes: number;
}

export interface LoadScanResult {
  drive: string;
  root_path: string;
  children: DirEntry[];
}

/** 某块盘最近一次整盘扫描存下来的完整结果 */
export interface SnapshotInfo {
  root: string;
  /** Unix 秒 */
  scanned_at: number;
  /** 压缩后的文件大小 */
  bytes: number;
}

export interface LoadSnapshotResult {
  root_path: string;
  children: DirEntry[];
  scanned_at: number;
}

export interface DiffEntry {
  path: string;
  name: string;
  old_size: number;
  new_size: number;
  diff: number;
}

export interface ScanTreeEvent {
  children: DirEntry[];
  scanning: boolean;
  current_dir: string;
}

export interface AiSettings {
  api_key: string;
  base_url: string;
  model_id: string;
  url_mode: string; // "append" | "raw"
  api_type: string; // "anthropic" | "openai"
  provider: string; // "api" | "claude-code"
  cli_model: string; // "default" | "sonnet" | "opus" | "haiku" | 自定义
  proxy_url: string; // 网络代理，如 http://127.0.0.1:7890；空 = 不使用
}

export interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
  created_at: string;
  meta?: string | null; // JSON 序列化的 ChatSegment[]（Claude Code provider）
}

export interface ChatSession {
  id: number;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface ChatStreamEvent {
  delta: string;
  done: boolean;
  error: string | null;
  // Claude Code provider 扩展字段
  kind?: "text" | "thinking" | "tool_use" | "tool_result" | "status";
  status?: string;
  tool_id?: string;
  tool_name?: string;
  tool_input?: string;
  tool_result?: string;
  tool_error?: boolean;
}

export type ChatSegment =
  | { type: "thinking"; text: string }
  | { type: "tool"; id: string; name: string; input: string; result?: string | null; is_error: boolean };

export interface CliDetection {
  available: boolean;
  path: string | null;
  version: string | null;
  error: string | null;
}

export interface Note {
  id: number;
  path: string;
  content: string;
  created_at: string;
  updated_at: string;
}

export interface AiAnalysis {
  path: string;
  description: string;
  priority: number;
  created_at: string;
}

export interface AnalyzePathInput {
  path: string;
  name: string;
  size: number;
  is_dir: boolean;
}

export interface EmptyFolderResult {
  deleted: number;
  bytes_freed: number;
  errors: string[];
}

export interface MoveResult {
  new_path: string;
}

export interface CleanupAction {
  id: number;
  path: string;
  action_type: "delete" | "empty";
  bytes_freed: number;
  description: string;
  created_at: string;
}

export interface CleanupSession {
  session_ts: string;
  drive: string;
  total_freed: number;
  actions: CleanupAction[];
}

export interface CleanupLogPage {
  sessions: CleanupSession[];
  total_sessions: number;
}

export interface ContextMenuTarget {
  entry: DirEntry;
  x: number;
  y: number;
}

export interface Tag {
  id: number;
  name: string;
  color: string;
  is_preset: boolean;
  created_at: string;
}

export interface FileTag {
  id: number;
  path: string;
  tag_id: number;
  tag_name: string;
  tag_color: string;
  created_at: string;
}

export interface TaggedPath {
  path: string;
  name: string;
  is_dir: boolean;
  tag_id: number;
  tag_name: string;
  tag_color: string;
  tagged_at: string;
}

// ---------------------------------------------------------------------------
// 档案馆：跨盘的软件清单与箱子（get_archive）
// ---------------------------------------------------------------------------

/** 档案馆的三个门类：软件 / 个人文件 / 插件与开发环境 */
export type ArchiveCategory = "software" | "personal" | "devenv";

/** 软件名下一块空间的性质 */
export type PartKind = "prog" | "data" | "regen" | "cache" | "residue" | "system";

export interface ArchivePart {
  path: string;
  kind: PartKind;
  size: number;
  /** 删除命令不会拒绝（受保护的系统目录、里面还装着别的软件的位置为 false） */
  deletable: boolean;
  /** 临时文件夹：只清空里面的东西，目录本身留着 */
  contents_only: boolean;
  /** 不是内置规则认的：user（你指定的）/ ai（AI 查过之后指定的） */
  origin?: "user" | "ai";
  /** AI 指定时给的依据 */
  reason?: string;
}

/** 能指定给一块位置的成分 */
export type ClaimKind = "prog" | "data" | "regen" | "cache" | "residue";

/** AI 查过之后指定的一条归属 */
export interface ArchiveClaim {
  path: string;
  app_key: string | null;
  kind: ClaimKind;
  source: "user" | "ai";
  confidence: number;
  reason: string;
}

/** 整理模式：ai（接入了 AI）/ template（只按内置规则） */
export type ArchiveMode = "ai" | "template";

/** 整理助手记下的用户回答和偏好 */
export interface ArchiveNote {
  id: number;
  note: string;
  created_at: string;
}

export interface ArchiveAttributeResult {
  records: ArchiveClaim[];
  /** AI 最后的总结 */
  summary: string;
}

export interface ArchiveApp {
  /** 稳定标识，装箱结果按它记 */
  key: string;
  name: string;
  publisher: string;
  /** 提取图标用的文件 */
  icon: string | null;
  /** 程序目录里没有可执行文件、系统里也没有安装记录 */
  uninstalled: boolean;
  /** 内置规则判断的箱子 id */
  hint: string | null;
  /** 软件或插件与开发环境（没认出归属的文件夹是个人文件） */
  category: "software" | "devenv";
  parts: ArchivePart[];
}

/** 没认出归属的位置（待整理目录树） */
export interface ArchiveFolder {
  key: string;
  path: string;
  name: string;
  is_dir: boolean;
  /** 盘根为 0 */
  depth: number;
  /** 扣掉已归到软件名下的部分后剩下的大小 */
  rest: number;
  /** 按名字猜的个人文件箱子（只作建议） */
  hint: string | null;
  kids: ArchiveFolder[];
}

export interface ArchiveDrive {
  root: string;
  /** Unix 秒 */
  scanned_at: number;
  total: number;
  /** 扫描之后的变化在实时跟进（外接盘不盯） */
  live: boolean;
}

export interface ArchiveBoxRow {
  id: string;
  name: string;
  color: string;
  builtin: boolean;
  category: ArchiveCategory;
}

export interface ArchiveUserPlacement {
  key: string;
  /** null：用户把它放回了待整理 */
  box_id: string | null;
}

export interface ArchiveAiPlacement {
  key: string;
  box_id: string;
  confidence: number;
  reason: string;
}

export interface ArchiveModel {
  drives: ArchiveDrive[];
  apps: ArchiveApp[];
  tree: ArchiveFolder[];
  boxes: ArchiveBoxRow[];
  user: ArchiveUserPlacement[];
  ai: ArchiveAiPlacement[];
  checks: ArchiveCacheCheck[];
  /** 和上次扫描比的大小变化 */
  growth: ArchiveGrowth;
}

/** 和上一次扫描比，变大为正、变小为负（字节）；只列变化不小于 10 MB 的 */
export interface ArchiveGrowth {
  /** 软件 key → 变化 */
  apps: Record<string, number>;
  /** 文件夹 key（dir:路径）→ 变化 */
  folders: Record<string, number>;
  /** 盘根 → 上次扫描的时间（Unix 秒） */
  since: Record<string, number>;
}

/** AI 对「像是缓存 / 残留」的位置的核对结论 */
export interface ArchiveCacheCheck {
  path: string;
  /** safe：删了不丢东西；keep：别删；unsure：看不出来 */
  verdict: "safe" | "keep" | "unsure";
  reason: string;
}

export interface ArchiveCheckItem {
  path: string;
  owner: string;
  kind: PartKind;
  size: number;
}

export interface ArchiveClassifyItem {
  key: string;
  name: string;
  /** 发布者、位置、大小等线索 */
  detail: string;
}
