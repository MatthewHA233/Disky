//! Claude Code CLI 适配器 — 参照 open-design 的 runtimes/defs/claude.ts + claude-stream.ts 机制：
//! 检测本地 `claude` 可执行文件 → spawn `claude -p --output-format stream-json --verbose`
//! → prompt 走 stdin → 逐行解析 JSONL 事件流 → 转发为 Tauri 事件 → session_id 存 SQLite 供 --resume。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::commands::ai::{ensure_ai_db, try_parse_partial_json, AiSettings, AnalyzePathInput, AnalyzePathResult};

/// 工具白名单：磁盘只读（Read/Glob/Grep）+ 技能 + 工作目录内唯一可写区 memory/
/// （备忘录 + 清理计划文件都放这里）。AI 始终无法直接删除磁盘文件。
/// 实测坑两则：①工具输入是绝对路径，相对规则在 Windows 上匹配不可靠，故同时
/// 下发绝对形式；②同一工具的多条 path 规则只有第一条可靠生效（requests/** 排在
/// memory/** 后面时始终被拒），所以可写区只保留 memory/ 一个，不再用 requests/。
fn allowed_tools_arg(workspace: &str) -> String {
    let ws = workspace.trim_end_matches(['\\', '/']).replace('\\', "/");
    format!(
        "Read,Glob,Grep,Skill,Write(memory/**),Write({ws}/memory/**),Edit(memory/**),Edit({ws}/memory/**)"
    )
}

// ---------------------------------------------------------------------------
// CLI 检测（对应 open-design executables.ts 的 resolveOnPath + 版本探测）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CliDetection {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(path_var) = std::env::var("PATH") {
        for d in std::env::split_paths(&path_var) {
            dirs.push(d);
        }
    }
    // GUI 启动时 PATH 可能不完整，补充常见安装位置（npm 全局、原生安装器、bun）
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".bun").join("bin"));
        dirs.push(home.join("AppData").join("Roaming").join("npm"));
        // macOS 原生安装器位置（GUI 应用的 PATH 只有 /usr/bin:/bin:/usr/sbin:/sbin）
        dirs.push(home.join(".claude").join("local"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    #[cfg(not(windows))]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// 在 PATH + 常见目录中解析 claude 可执行文件，Windows 下按 PATHEXT 尝试扩展名
pub fn resolve_claude_bin() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("CLAUDE_BIN") {
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    let mut seen = HashSet::new();
    for dir in candidate_dirs() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        for ext in &exts {
            let candidate = dir.join(format!("claude{}", ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 构建子进程命令：.cmd/.bat 必须经 cmd.exe 启动（CreateProcess 不直接执行脚本），
/// 全程加 CREATE_NO_WINDOW 避免弹黑窗。参数只含受控的短 token，prompt 一律走 stdin
/// （open-design 同样如此，规避 cmd shim ~8KB 命令行上限）。
fn build_command(exe: &PathBuf, args: &[String]) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ext = exe
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext == "cmd" || ext == "bat" {
            let mut joined = format!("\"{}\"", exe.display());
            for a in args {
                // cmd.exe 对 ()&|<>^ 等字符有特殊解析，含这些字符的参数也要加引号
                if a.contains(|c: char| " ()&|<>^%".contains(c)) {
                    joined.push_str(&format!(" \"{}\"", a));
                } else {
                    joined.push(' ');
                    joined.push_str(a);
                }
            }
            let mut cmd = std::process::Command::new("cmd.exe");
            // /d 跳过 AutoRun，/s 保留外层引号语义，/c 执行后退出
            cmd.raw_arg(format!("/d /s /c \"{}\"", joined));
            cmd.creation_flags(CREATE_NO_WINDOW);
            return cmd;
        }
        let mut cmd = std::process::Command::new(exe);
        cmd.args(args);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new(exe);
        cmd.args(args);
        cmd
    }
}

#[tauri::command]
pub async fn detect_claude_cli() -> Result<CliDetection, String> {
    let Some(bin) = resolve_claude_bin() else {
        return Ok(CliDetection {
            available: false,
            path: None,
            version: None,
            error: Some("未在 PATH 或常见安装目录中找到 claude，可先安装 Claude Code（npm install -g @anthropic-ai/claude-code）".into()),
        });
    };

    let mut cmd = build_command(&bin, &["--version".into()]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let mut child = tokio::process::Command::from(cmd)
        .spawn()
        .map_err(|e| format!("无法启动 claude: {}", e))?;

    let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut out).await;
        }
        let _ = child.wait().await;
        out
    })
    .await;

    match output {
        Ok(out) => {
            let version = out.trim().to_string();
            Ok(CliDetection {
                available: true,
                path: Some(bin.display().to_string()),
                version: if version.is_empty() { None } else { Some(version) },
                error: None,
            })
        }
        Err(_) => {
            let _ = child.kill().await;
            Ok(CliDetection {
                available: false,
                path: Some(bin.display().to_string()),
                version: None,
                error: Some("claude --version 探测超时".into()),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// session 持久化（对应 open-design agent-session-resume.ts）
// ---------------------------------------------------------------------------

fn load_session_id(app: &tauri::AppHandle, key: &str) -> Option<String> {
    let conn = ensure_ai_db(app).ok()?;
    conn.query_row(
        "SELECT session_id FROM agent_sessions WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn store_session_id(app: &tauri::AppHandle, key: &str, session_id: &str) {
    if let Ok(conn) = ensure_ai_db(app) {
        let _ = conn.execute(
            "INSERT INTO agent_sessions (key, session_id) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET session_id = excluded.session_id",
            params![key, session_id],
        );
    }
}

pub fn clear_session_id(app: &tauri::AppHandle, key: &str) {
    if let Ok(conn) = ensure_ai_db(app) {
        let _ = conn.execute("DELETE FROM agent_sessions WHERE key = ?1", params![key]);
    }
}

/// `--resume` 目标 session 已被清理时 Claude Code 的报错特征（同 open-design 的容错模式）
fn is_resume_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("no conversation found with session id")
        || lower.contains("no session found")
        || (lower.contains("session") && lower.contains("not found"))
}

// ---------------------------------------------------------------------------
// 标题生成（小模型，纯文本输出，无工具）
// ---------------------------------------------------------------------------

/// 用 claude -p --model haiku 一次性概括出短标题（纯文本输出，不启用工具）。
/// 失败返回 None，由调用方回退。
pub async fn generate_title_cli(settings: &AiSettings, prompt: &str) -> Option<String> {
    // 优先用最快的 haiku；用户自定义了 cli_model 也仍用 haiku 省成本（标题不需要强模型）
    // 标题生成限时 30s，超时放弃走回退
    prompt_once_cli(settings, prompt, Some("claude-haiku-4-5"), std::time::Duration::from_secs(30))
        .await
        .ok()
}

/// 一次性提问：`claude -p`，纯文本输出。`model` 为 None 时用设置里的模型。
pub async fn prompt_once_cli(
    settings: &AiSettings,
    prompt: &str,
    model: Option<&str>,
    timeout: std::time::Duration,
) -> Result<String, String> {
    prompt_once_cli_with(settings, prompt, model, &[], timeout).await
}

/// 同 `prompt_once_cli`，另外带上命令行参数（限定能用的工具、加可读目录等），放在最后
pub async fn prompt_once_cli_with(
    settings: &AiSettings,
    prompt: &str,
    model: Option<&str>,
    extra: &[String],
    timeout: std::time::Duration,
) -> Result<String, String> {
    let bin = resolve_claude_bin().ok_or("没有找到 Claude Code，请先安装或在 AI 设置里改用 API")?;
    let mut args: Vec<String> = vec!["-p".into()];
    let model = model.map(str::to_string).or_else(|| {
        let m = settings.cli_model.trim();
        (!m.is_empty() && m != "default").then(|| m.to_string())
    });
    if let Some(m) = model {
        args.push("--model".into());
        args.push(m);
    }
    args.extend(extra.iter().cloned());
    let mut std_cmd = build_command(&bin, &args);
    std_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let proxy = settings.proxy_url.trim();
    if !proxy.is_empty() {
        std_cmd
            .env("HTTP_PROXY", proxy)
            .env("HTTPS_PROXY", proxy)
            .env("ALL_PROXY", proxy)
            .env("NO_PROXY", "localhost,127.0.0.1");
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("启动 Claude Code 失败: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        drop(stdin);
    }
    let out = tokio::time::timeout(timeout, async {
        let mut buf = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut buf).await;
        }
        let _ = child.wait().await;
        buf
    })
    .await
    .map_err(|_| "Claude Code 没有在限定时间内回复".to_string())?;
    let trimmed = out.trim();
    if trimmed.is_empty() {
        Err("Claude Code 没有返回内容".into())
    } else {
        Ok(trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// 流式事件（chat-stream 的超集：旧字段不变，新增 kind/tool_* 可选字段）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct CliStreamEvent {
    pub delta: String,
    pub done: bool,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>, // "text" | "thinking" | "tool_use" | "tool_result" | "status"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_error: Option<bool>,
}

/// 持久化到 chat_messages.meta 的片段（思考/工具调用），文本正文仍存 content
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatSegment {
    #[serde(rename = "thinking")]
    Thinking { text: String },
    #[serde(rename = "tool")]
    Tool {
        id: String,
        name: String,
        input: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        is_error: bool,
    },
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{}…", cut)
}

// ---------------------------------------------------------------------------
// JSONL 流解析器（移植 claude-stream.ts 的核心状态机）
// ---------------------------------------------------------------------------

struct BlockState {
    is_tool_use: bool,
    tool_id: Option<String>,
    tool_name: Option<String>,
    input_json: String,
}

/// 解析一次 spawn 的全部 stdout 行，把事件转发给 `emit`，并累积最终文本/片段
struct StreamParser {
    session_id: Option<String>,
    full_text: String,
    segments: Vec<ChatSegment>,
    result_text: Option<String>,
    result_is_error: bool,
    // 流式状态
    current_message_id: Option<String>,
    blocks: HashMap<String, BlockState>,
    streamed_tool_use_ids: HashSet<String>,
    // 本次运行是否已通过 stream_event 增量收到过文本/思考。
    // assistant 包会按内容块重复完整文本，若按消息 id 去重，流事件与包内 id
    // 对不上时整块文本会被二次注入到流式中间（界面表现为段落交错重复），
    // 所以这里用运行级布尔做硬抑制：增量通了就永不采用包内文本兜底。
    streamed_any_text: bool,
    streamed_any_thinking: bool,
}

impl StreamParser {
    fn new() -> Self {
        Self {
            session_id: None,
            full_text: String::new(),
            segments: Vec::new(),
            result_text: None,
            result_is_error: false,
            current_message_id: None,
            blocks: HashMap::new(),
            streamed_tool_use_ids: HashSet::new(),
            streamed_any_text: false,
            streamed_any_thinking: false,
        }
    }

    fn block_key(&self, index: &Value) -> String {
        format!(
            "{}:{}",
            self.current_message_id.as_deref().unwrap_or("anon"),
            index
        )
    }

    fn push_thinking(&mut self, text: &str) {
        if let Some(ChatSegment::Thinking { text: t }) = self.segments.last_mut() {
            t.push_str(text);
        } else {
            self.segments.push(ChatSegment::Thinking { text: text.to_string() });
        }
    }

    fn push_tool_use(&mut self, id: &str, name: &str, input: &Value, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        // 提取关键字段做摘要再发给前端——整个 input JSON 截断后无法解析，
        // 界面只能显示一坨原始 JSON（如 Write 长内容时 file_path 完全看不到）
        let input_str = ["file_path", "path", "pattern", "command", "query"]
            .iter()
            .find_map(|k| input[*k].as_str())
            .map(|s| truncate_chars(s, 200))
            .unwrap_or_else(|| truncate_chars(&input.to_string(), 200));
        self.segments.push(ChatSegment::Tool {
            id: id.to_string(),
            name: name.to_string(),
            input: input_str.clone(),
            result: None,
            is_error: false,
        });
        emit(CliStreamEvent {
            kind: Some("tool_use".into()),
            tool_id: Some(id.to_string()),
            tool_name: Some(name.to_string()),
            tool_input: Some(input_str),
            ..Default::default()
        });
    }

    fn push_tool_result(&mut self, tool_use_id: &str, content: String, is_error: bool, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        let content = truncate_chars(&content, 2000);
        for seg in self.segments.iter_mut().rev() {
            if let ChatSegment::Tool { id, result, is_error: err, .. } = seg {
                if id == tool_use_id {
                    *result = Some(content.clone());
                    *err = is_error;
                    break;
                }
            }
        }
        emit(CliStreamEvent {
            kind: Some("tool_result".into()),
            tool_id: Some(tool_use_id.to_string()),
            tool_result: Some(content),
            tool_error: Some(is_error),
            ..Default::default()
        });
    }

    fn emit_text(&mut self, text: &str, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        if text.is_empty() {
            return;
        }
        self.full_text.push_str(text);
        emit(CliStreamEvent {
            delta: text.to_string(),
            kind: Some("text".into()),
            ..Default::default()
        });
    }

    fn emit_thinking(&mut self, text: &str, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        if text.is_empty() {
            return;
        }
        self.push_thinking(text);
        emit(CliStreamEvent {
            delta: text.to_string(),
            kind: Some("thinking".into()),
            ..Default::default()
        });
    }

    fn handle_line(&mut self, line: &str, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let Ok(obj) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let obj_type = obj["type"].as_str().unwrap_or("");

        match obj_type {
            "system" => {
                if obj["subtype"] == "init" {
                    if let Some(sid) = obj["session_id"].as_str() {
                        self.session_id = Some(sid.to_string());
                    }
                    emit(CliStreamEvent {
                        kind: Some("status".into()),
                        status: Some(format!(
                            "已连接 Claude Code（{}）",
                            obj["model"].as_str().unwrap_or("default")
                        )),
                        ..Default::default()
                    });
                }
            }
            "stream_event" => {
                if obj["event"].is_object() {
                    let ev = obj["event"].clone();
                    self.handle_stream_event(&ev, emit);
                }
            }
            // 最终 assistant 包：工具调用的兜底来源；老版本 CLI 不发增量时文本也只在这里
            "assistant" => {
                let msg = &obj["message"];
                if let Some(id) = msg["id"].as_str() {
                    self.current_message_id = Some(id.to_string());
                }
                let text_already = self.streamed_any_text;
                let thinking_already = self.streamed_any_thinking;
                if let Some(content) = msg["content"].as_array() {
                    for block in content.clone() {
                        match block["type"].as_str().unwrap_or("") {
                            "tool_use" => {
                                let id = block["id"].as_str().unwrap_or("").to_string();
                                if self.streamed_tool_use_ids.remove(&id) {
                                    continue;
                                }
                                let name = block["name"].as_str().unwrap_or("?").to_string();
                                self.push_tool_use(&id, &name, &block["input"], emit);
                            }
                            "text" if !text_already => {
                                if let Some(t) = block["text"].as_str() {
                                    let t = t.to_string();
                                    self.emit_text(&t, emit);
                                }
                            }
                            "thinking" if !thinking_already => {
                                if let Some(t) = block["thinking"].as_str() {
                                    let t = t.to_string();
                                    self.emit_thinking(&t, emit);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            // user 包通常是上一轮工具的 tool_result 回写
            "user" => {
                if let Some(content) = obj["message"]["content"].as_array() {
                    for block in content.clone() {
                        if block["type"] == "tool_result" {
                            let tool_use_id = block["tool_use_id"].as_str().unwrap_or("").to_string();
                            let text = stringify_tool_result(&block["content"]);
                            let is_error = block["is_error"].as_bool().unwrap_or(false);
                            self.push_tool_result(&tool_use_id, text, is_error, emit);
                        }
                    }
                }
            }
            "result" => {
                if let Some(sid) = obj["session_id"].as_str() {
                    self.session_id = Some(sid.to_string());
                }
                self.result_is_error = obj["is_error"].as_bool().unwrap_or(false)
                    || obj["subtype"].as_str().map_or(false, |s| s != "success");
                if let Some(r) = obj["result"].as_str() {
                    self.result_text = Some(r.to_string());
                }
            }
            _ => {}
        }
    }

    fn handle_stream_event(&mut self, ev: &Value, emit: &mut (dyn FnMut(CliStreamEvent) + Send)) {
        match ev["type"].as_str().unwrap_or("") {
            "message_start" => {
                self.current_message_id =
                    ev["message"]["id"].as_str().map(|s| s.to_string());
            }
            "content_block_start" => {
                let key = self.block_key(&ev["index"]);
                let block = &ev["content_block"];
                self.blocks.insert(
                    key,
                    BlockState {
                        is_tool_use: block["type"] == "tool_use",
                        tool_id: block["id"].as_str().map(|s| s.to_string()),
                        tool_name: block["name"].as_str().map(|s| s.to_string()),
                        input_json: String::new(),
                    },
                );
            }
            "content_block_delta" => {
                let delta = &ev["delta"];
                match delta["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        if let Some(t) = delta["text"].as_str() {
                            self.streamed_any_text = true;
                            let t = t.to_string();
                            self.emit_text(&t, emit);
                        }
                    }
                    "thinking_delta" => {
                        if let Some(t) = delta["thinking"].as_str() {
                            self.streamed_any_thinking = true;
                            let t = t.to_string();
                            self.emit_thinking(&t, emit);
                        }
                    }
                    "input_json_delta" => {
                        let key = self.block_key(&ev["index"]);
                        if let Some(partial) = delta["partial_json"].as_str() {
                            if let Some(state) = self.blocks.get_mut(&key) {
                                if state.is_tool_use {
                                    state.input_json.push_str(partial);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let key = self.block_key(&ev["index"]);
                if let Some(state) = self.blocks.remove(&key) {
                    if state.is_tool_use {
                        if let (Some(id), Some(name)) = (state.tool_id, state.tool_name) {
                            if let Ok(input) = serde_json::from_str::<Value>(&state.input_json) {
                                self.push_tool_use(&id, &name, &input, emit);
                                self.streamed_tool_use_ids.insert(id);
                            }
                            // 解析失败则留给最终 assistant 包兜底
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn stringify_tool_result(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .map(|c| {
                if c["type"] == "text" {
                    c["text"].as_str().unwrap_or("").to_string()
                } else {
                    c.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    content.to_string()
}

// ---------------------------------------------------------------------------
// spawn + 流驱动
// ---------------------------------------------------------------------------

struct RunOutcome {
    parser: StreamParser,
    exit_ok: bool,
    stderr_text: String,
}

fn sanitize_model_id(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() || trimmed == "default" {
        return None;
    }
    if trimmed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-')) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn build_cli_args(
    settings: &AiSettings,
    resume_session: Option<&str>,
    new_session: &str,
    max_turns: u32,
    include_partial: bool,
    workspace: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--allowedTools".into(),
        allowed_tools_arg(workspace),
        "--max-turns".into(),
        max_turns.to_string(),
    ];
    // 老版本 CLI 不认识该 flag 会直接退出，失败后由调用方降级重试
    if include_partial {
        args.push("--include-partial-messages".into());
    }
    if let Some(model) = sanitize_model_id(&settings.cli_model) {
        args.push("--model".into());
        args.push(model);
    }
    match resume_session {
        Some(id) => {
            args.push("--resume".into());
            args.push(id.to_string());
        }
        None => {
            args.push("--session-id".into());
            args.push(new_session.to_string());
        }
    }
    args
}

/// spawn 一次 claude 进程：写 prompt 到 stdin，逐行解析 stdout，stderr 收集供错误诊断。
/// GUI 启动的进程往往没有 shell 里的代理环境变量，CLI 直连 Anthropic 会 403，
/// 所以把设置里的代理注入子进程 env（HTTP_PROXY / HTTPS_PROXY / ALL_PROXY）。
async fn run_claude_once(
    bin: &PathBuf,
    args: &[String],
    cwd: Option<&str>,
    prompt: &str,
    proxy_url: &str,
    emit: &mut (dyn FnMut(CliStreamEvent) + Send),
) -> Result<RunOutcome, String> {
    let mut std_cmd = build_command(bin, args);
    std_cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        if std::path::Path::new(dir).is_dir() {
            std_cmd.current_dir(dir);
        }
    }
    let proxy = proxy_url.trim();
    if !proxy.is_empty() {
        std_cmd
            .env("HTTP_PROXY", proxy)
            .env("HTTPS_PROXY", proxy)
            .env("ALL_PROXY", proxy)
            .env("NO_PROXY", "localhost,127.0.0.1");
    }
    let mut child = tokio::process::Command::from(std_cmd)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("无法启动 claude: {}", e))?;

    // 写 prompt 后立即关闭 stdin，claude 完成本轮后自行退出
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| format!("写入 prompt 失败: {}", e))?;
        drop(stdin);
    }

    let stderr_task = {
        let mut stderr = child.stderr.take();
        tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(s) = stderr.as_mut() {
                let _ = s.read_to_string(&mut buf).await;
            }
            buf
        })
    };

    let mut parser = StreamParser::new();
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            parser.handle_line(&line, emit);
        }
    }

    let status = child.wait().await.map_err(|e| format!("等待进程失败: {}", e))?;
    let stderr_text = stderr_task.await.unwrap_or_default();

    Ok(RunOutcome {
        parser,
        exit_ok: status.success(),
        stderr_text,
    })
}

// ---------------------------------------------------------------------------
// AI 清理计划（requests/cleanup-plan.json → 前端确认窗 → Disky 回收站流程）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlanItem {
    pub path: String,
    #[serde(default)]
    pub reason: String,
}

/// agent 可能写入清理计划的候选位置（首选 memory/，requests/ 为旧协议兼容，
/// draft 是 agent 在权限受挫时自发使用过的名字）
const CLEANUP_PLAN_CANDIDATES: &[&str] = &[
    "memory/cleanup-plan.json",
    "requests/cleanup-plan.json",
    "memory/cleanup-plan-draft.json",
];

/// 消费 agent 写下的清理计划文件（读后即删），过滤掉受保护/不存在的路径
fn take_cleanup_plan(workspace: &std::path::Path) -> Option<Vec<CleanupPlanItem>> {
    let file = CLEANUP_PLAN_CANDIDATES
        .iter()
        .map(|rel| {
            // PathBuf::join 接受 "/" 相对路径，但 Windows 上统一成 "\" 以保持路径字符串一致
            #[cfg(windows)]
            let rel = rel.replace('/', "\\");
            workspace.join(&*rel)
        })
        .find(|p| p.is_file())?;
    let text = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let v: Value = serde_json::from_str(&text).ok()?;
    let arr = if v.is_array() { v } else { v.get("items")?.clone() };
    let items: Vec<CleanupPlanItem> = serde_json::from_value(arr).ok()?;
    let valid: Vec<CleanupPlanItem> = items
        .into_iter()
        .filter(|i| {
            let p = crate::scanner::normalize_path(&i.path);
            std::path::Path::new(&p).is_absolute()
                && !crate::commands::cleanup::is_protected(&p)
                && std::path::Path::new(&p).exists()
        })
        .collect();
    if valid.is_empty() {
        None
    } else {
        Some(valid)
    }
}

// ---------------------------------------------------------------------------
// 对话（由 ai.rs 的 send_chat_message 按 provider 分流到这里）
// ---------------------------------------------------------------------------

pub async fn run_chat_cli(
    app: tauri::AppHandle,
    message: String,
    settings: AiSettings,
    disk_context: String,
    deep_summary: String,
    scan_tree: String,
    chat_session_id: i64,
    // 档案馆现状和执行整理动作的工具（读不出来时为 None）
    archive: Option<(String, crate::commands::archive_chat::ChatTools)>,
) -> Result<(), String> {
    // 每个聊天会话保有独立的 CLI resume session，互不串记忆
    let session_key = format!("chat:{}", chat_session_id);
    let emit_err = |app: &tauri::AppHandle, msg: &str| {
        let _ = app.emit(
            "chat-stream",
            CliStreamEvent {
                done: true,
                error: Some(msg.to_string()),
                ..Default::default()
            },
        );
    };

    let Some(bin) = resolve_claude_bin() else {
        let msg = "未检测到 Claude Code，请先安装并登录（npm install -g @anthropic-ai/claude-code）";
        emit_err(&app, msg);
        return Err(msg.into());
    };

    // 保存用户消息
    {
        let conn = ensure_ai_db(&app)?;
        conn.execute(
            "INSERT INTO chat_messages (role, content, session_id) VALUES ('user', ?1, ?2)",
            params![message, chat_session_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // 受管工作目录：CLAUDE.md（角色指令）+ skills + context/*（每轮刷新）+ memory/（AI 备忘）。
    // cwd 指到这里后 Claude Code 自动加载 CLAUDE.md 与技能 —— 同 open-design 的注入方式。
    let user_data = crate::commands::agent_workspace::build_user_data_summary(&app);
    let workspace = crate::commands::agent_workspace::prepare_workspace(
        &app,
        &deep_summary,
        &scan_tree,
        &user_data,
    )?;
    let workspace_str = workspace.display().to_string();
    let (archive_brief, mut archive_tools) = match archive {
        Some((brief, tools)) => (Some(brief), Some(tools)),
        None => (None, None),
    };
    crate::commands::agent_workspace::write_archive_context(&workspace, archive_brief.as_deref())?;

    let mut resume = load_session_id(&app, &session_key);
    let new_session = uuid::Uuid::new_v4().to_string();
    let mut include_partial = true;

    let app_emit = app.clone();
    let mut emit = move |ev: CliStreamEvent| {
        let _ = app_emit.emit("chat-stream", ev);
    };

    // 新会话发概览 + 上下文文件指引（详细指令在 CLAUDE.md，由 CLI 自动加载）；
    // 续接会话只发增量（CLI 自己保有记忆），但 context/ 文件每轮都已刷新。
    // 失败降级：老 CLI 不支持 --include-partial-messages → 去掉重试；
    // resume 目标 session 已被清理 → 换新 session 重试（同 open-design 的容错路径）。
    let outcome = loop {
        let prompt = if resume.is_some() {
            format!(
                "[最新磁盘扫描概览（context/ 上下文文件已同步刷新）]\n{}\n\n{}",
                disk_context, message
            )
        } else {
            format!(
                "[当前磁盘扫描概览]\n{}\n\n更深层明细见 context/scan-summary.md 与 context/scan-tree.tsv（按 CLAUDE.md 的渐进式披露规则按需查看）。开始前先 Read memory/notes.md 恢复你的既有认知，再 Read context/archive.md 看档案馆现状。\n\n{}",
                disk_context, message
            )
        };
        let args = build_cli_args(&settings, resume.as_deref(), &new_session, 30, include_partial, &workspace_str);
        let outcome = run_claude_once(&bin, &args, Some(&workspace_str), &prompt, &settings.proxy_url, &mut emit).await?;
        if outcome.exit_ok {
            break outcome;
        }
        if include_partial && outcome.stderr_text.contains("--include-partial-messages") {
            include_partial = false;
            continue;
        }
        if resume.is_some()
            && (is_resume_failure(&outcome.stderr_text)
                || is_resume_failure(outcome.parser.result_text.as_deref().unwrap_or("")))
        {
            clear_session_id(&app, &session_key);
            resume = None;
            continue;
        }
        break outcome;
    };

    let parser = &outcome.parser;

    // 持久化 session id 供下一轮 --resume
    if let Some(sid) = &parser.session_id {
        store_session_id(&app, &session_key, sid);
    }

    // 失败处理：进程非零退出且没有任何文本产出
    if !outcome.exit_ok && parser.full_text.is_empty() && parser.result_text.is_none() {
        let stderr_tail = truncate_chars(outcome.stderr_text.trim(), 500);
        let msg = if stderr_tail.is_empty() {
            "Claude Code 进程异常退出（无输出）。请确认已运行 claude 并完成登录。".to_string()
        } else {
            format!("Claude Code 出错: {}", stderr_tail)
        };
        emit_err(&app, &msg);
        return Err(msg);
    }

    // 文本兜底：增量为空时用 result 事件里的最终文本
    let mut full_reply = parser.full_text.clone();
    if full_reply.is_empty() {
        if let Some(r) = &parser.result_text {
            full_reply = r.clone();
            let _ = app.emit(
                "chat-stream",
                CliStreamEvent {
                    delta: full_reply.clone(),
                    kind: Some("text".into()),
                    ..Default::default()
                },
            );
        }
    }

    if parser.result_is_error && full_reply.is_empty() {
        let msg = format!(
            "Claude Code 返回错误: {}",
            truncate_chars(outcome.stderr_text.trim(), 500)
        );
        emit_err(&app, &msg);
        return Err(msg);
    }

    // 保存 assistant 回复（meta 列存思考/工具片段）
    if !full_reply.is_empty() || !parser.segments.is_empty() {
        let meta = if parser.segments.is_empty() {
            None
        } else {
            serde_json::to_string(&parser.segments).ok()
        };
        let conn = ensure_ai_db(&app)?;
        conn.execute(
            "INSERT INTO chat_messages (role, content, meta, session_id) VALUES ('assistant', ?1, ?2, ?3)",
            params![full_reply, meta, chat_session_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // agent 提交了清理计划 → 通知前端弹确认窗（确认后走回收站 + 记清理历史）
    if let Some(items) = take_cleanup_plan(&workspace) {
        let _ = app.emit("agent-cleanup-request", serde_json::json!({ "items": items }));
    }

    // 整理助手写下的动作：按同一套规矩执行，结果留给下一轮，档案馆重新读
    // 读不出现状时没有执行的工具，动作文件留着（下一轮开始前会作废，不会误执行旧的）
    if let Some(tools) = archive_tools.as_mut() {
        if let Some(text) = crate::commands::agent_workspace::take_archive_actions(&workspace) {
            let results = crate::commands::archive_chat::apply_cli_actions(tools, &text);
            let _ = crate::commands::agent_workspace::write_archive_results(&workspace, &results);
            if tools.changed {
                let _ = app.emit("archive-changed", ());
            }
        }
    }

    let _ = app.emit(
        "chat-stream",
        CliStreamEvent {
            done: true,
            ..Default::default()
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 批量路径分析（由 ai.rs 的 analyze_paths 按 provider 分流到这里）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct AnalyzeStreamEventCli {
    delta: String,
    done: bool,
    error: Option<String>,
}

pub async fn run_analyze_cli(
    app: tauri::AppHandle,
    items: Vec<AnalyzePathInput>,
    settings: AiSettings,
    deep_summary: String,
    scan_tree: String,
) -> Result<Vec<AnalyzePathResult>, String> {
    let Some(bin) = resolve_claude_bin() else {
        let msg = "未检测到 Claude Code，请先安装并登录（npm install -g @anthropic-ai/claude-code）";
        let _ = app.emit(
            "analyze-stream",
            AnalyzeStreamEventCli { delta: String::new(), done: true, error: Some(msg.into()) },
        );
        return Err(msg.into());
    };

    let mut item_lines = Vec::new();
    for item in &items {
        let kind = if item.is_dir { "DIR" } else { "FILE" };
        item_lines.push(format!(
            "- [{}] {} (path: {}, size: {} bytes)",
            kind, item.name, item.path, item.size
        ));
    }

    let prompt = format!(
        "你是磁盘分析助手。分析以下文件/文件夹，为每个提供：\n\
        1. 简短中文描述（1-2句，说明可能的作用）\n\
        2. 清理优先级（0.5-5.0，0.5步进）：\n\
           - 0.5-1.0 = 系统/程序核心，勿删\n\
           - 1.5-2.0 = 可能重要，谨慎\n\
           - 2.5-3.0 = 中等，或可清理\n\
           - 3.5-4.0 = 推荐清理（缓存/日志/临时）\n\
           - 4.5-5.0 = 强烈推荐清理（旧备份/垃圾）\n\n\
        评级标准与 Windows 目录知识见 disk-cleanup 技能；memory/notes.md 里可能有你之前的调查结论，可先查。\n\
        如果不确定某项的作用，可以用 Read / Glob / Grep 工具查看其实际内容再判断，但不要修改磁盘上的任何文件。\n\n\
        待分析项：\n{}\n\n\
        最终回复只返回 JSON 数组，无 markdown 代码块：\n\
        [{{\"path\":\"...\",\"description\":\"...\",\"priority\":3.5}}]",
        item_lines.join("\n")
    );

    // 分析任务每次独立 session，不参与对话 resume；
    // cwd 同样指向受管工作目录，复用 CLAUDE.md 规则、disk-cleanup 技能与 AI 备忘录
    let new_session = uuid::Uuid::new_v4().to_string();
    let user_data = crate::commands::agent_workspace::build_user_data_summary(&app);
    let workspace = crate::commands::agent_workspace::prepare_workspace(
        &app,
        &deep_summary,
        &scan_tree,
        &user_data,
    )?;
    let cwd = Some(workspace.display().to_string());

    let app_emit = app.clone();
    let mut emit = move |ev: CliStreamEvent| {
        // 分析视图只消费文本增量；工具调用以一行提示透出，思考不透出
        match ev.kind.as_deref() {
            Some("text") => {
                let _ = app_emit.emit(
                    "analyze-stream",
                    AnalyzeStreamEventCli { delta: ev.delta, done: false, error: None },
                );
            }
            Some("tool_use") => {
                let name = ev.tool_name.unwrap_or_default();
                let summary = summarize_tool_input(ev.tool_input.as_deref().unwrap_or(""));
                let _ = app_emit.emit(
                    "analyze-stream",
                    AnalyzeStreamEventCli {
                        delta: format!("\n⚙ {} {}\n", name, summary),
                        done: false,
                        error: None,
                    },
                );
            }
            _ => {}
        }
    };

    let workspace_str = workspace.display().to_string();
    let mut outcome = run_claude_once(
        &bin,
        &build_cli_args(&settings, None, &new_session, 15, true, &workspace_str),
        cwd.as_deref(),
        &prompt,
        &settings.proxy_url,
        &mut emit,
    )
    .await?;
    if !outcome.exit_ok && outcome.stderr_text.contains("--include-partial-messages") {
        outcome = run_claude_once(
            &bin,
            &build_cli_args(&settings, None, &new_session, 15, false, &workspace_str),
            cwd.as_deref(),
            &prompt,
            &settings.proxy_url,
            &mut emit,
        )
        .await?;
    }
    let parser = &outcome.parser;

    let full_text = if !parser.full_text.is_empty() {
        parser.full_text.clone()
    } else {
        parser.result_text.clone().unwrap_or_default()
    };

    if full_text.is_empty() {
        let msg = format!(
            "Claude Code 分析失败: {}",
            truncate_chars(outcome.stderr_text.trim(), 500)
        );
        let _ = app.emit(
            "analyze-stream",
            AnalyzeStreamEventCli { delta: String::new(), done: true, error: Some(msg.clone()) },
        );
        return Err(msg);
    }

    let _ = app.emit(
        "analyze-stream",
        AnalyzeStreamEventCli { delta: String::new(), done: true, error: None },
    );

    // 模型可能仍包了 markdown 代码块，剥掉再解析
    let cleaned = strip_code_fence(&full_text);
    let results: Vec<AnalyzePathResult> = match serde_json::from_str(&cleaned) {
        Ok(r) => r,
        Err(_) => try_parse_partial_json(&cleaned)?,
    };
    Ok(results)
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                return trimmed[start..=end].to_string();
            }
        }
        return trimmed[start..].to_string();
    }
    trimmed.to_string()
}

fn summarize_tool_input(input_json: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(input_json) else {
        return String::new();
    };
    for key in ["file_path", "path", "pattern", "command", "query"] {
        if let Some(s) = v[key].as_str() {
            return truncate_chars(s, 120);
        }
    }
    truncate_chars(input_json, 120)
}
