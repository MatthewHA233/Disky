export interface DirEntry {
  name: string;
  path: string;
  files: number;
  subdirs: number;
  logical_size: number;
  is_dir: boolean;
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
