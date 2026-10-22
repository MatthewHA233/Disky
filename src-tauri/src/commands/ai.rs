use futures::StreamExt;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::commands::scan::ScanState;
use crate::scanner::ROOT;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub api_key: String,
    pub base_url: String,
    pub model_id: String,
    pub url_mode: String, // "append" | "raw"
    pub api_type: String, // "anthropic" | "openai"
    #[serde(default = "default_provider")]
    pub provider: String, // "api" | "claude-code"
    #[serde(default = "default_cli_model")]
    pub cli_model: String, // "default" | "sonnet" | "opus" | "haiku" | 自定义
    /// 网络代理，如 http://127.0.0.1:7890；空 = 不使用代理。
    /// CLI 模式注入子进程 HTTP(S)_PROXY 环境变量，API 模式配置 reqwest 代理。
    #[serde(default)]
    pub proxy_url: String,
}

fn default_provider() -> String {
    "api".into()
}

fn default_cli_model() -> String {
    "default".into()
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com".into(),
            model_id: "claude-sonnet-4-6".into(),
            url_mode: "append".into(),
            api_type: "anthropic".into(),
            provider: default_provider(),
            cli_model: default_cli_model(),
            // 默认填本机 Clash 常用端口；不需要代理时在设置里清空即可
            proxy_url: "http://127.0.0.1:7890".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
    /// JSON 序列化的思考/工具调用片段（仅 Claude Code provider 产生），见 agent_cli::ChatSegment
    pub meta: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: i64,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamEvent {
    pub delta: String,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeStreamEvent {
    pub delta: String,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysis {
    pub path: String,
    pub description: String,
    pub priority: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzePathInput {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzePathResult {
    pub path: String,
    pub description: String,
    pub priority: f64,
}

pub(crate) fn ensure_ai_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("disky.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS ai_analysis (
            path TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            priority REAL NOT NULL,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS agent_sessions (
            key TEXT PRIMARY KEY,
            session_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chat_sessions (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            created_at TEXT DEFAULT (datetime('now','localtime')),
            updated_at TEXT DEFAULT (datetime('now','localtime'))
        );"
    ).map_err(|e| e.to_string())?;
    // 旧库迁移：chat_messages 加 meta 列 + session_id 列（已存在则忽略报错）
    let _ = conn.execute("ALTER TABLE chat_messages ADD COLUMN meta TEXT", []);
    let _ = conn.execute("ALTER TABLE chat_messages ADD COLUMN session_id INTEGER", []);
    // 把历史的无归属消息迁移到一个默认会话
    let orphans: i64 = conn
        .query_row("SELECT COUNT(*) FROM chat_messages WHERE session_id IS NULL", [], |r| r.get(0))
        .unwrap_or(0);
    if orphans > 0 {
        conn.execute("INSERT INTO chat_sessions (title) VALUES ('历史对话')", [])
            .map_err(|e| e.to_string())?;
        let sid = conn.last_insert_rowid();
        conn.execute("UPDATE chat_messages SET session_id = ?1 WHERE session_id IS NULL", params![sid])
            .map_err(|e| e.to_string())?;
    }
    Ok(conn)
}

#[tauri::command]
pub fn load_ai_settings(app: tauri::AppHandle) -> Result<AiSettings, String> {
    let conn = ensure_ai_db(&app)?;
    let mut settings = AiSettings::default();

    let mut stmt = conn
        .prepare("SELECT key, value FROM ai_settings")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let (key, value) = row.map_err(|e| e.to_string())?;
        match key.as_str() {
            "api_key" => settings.api_key = value,
            "base_url" => settings.base_url = value,
            "model_id" => settings.model_id = value,
            "url_mode" => settings.url_mode = value,
            "api_type" => settings.api_type = value,
            "provider" => settings.provider = value,
            "cli_model" => settings.cli_model = value,
            "proxy_url" => settings.proxy_url = value,
            _ => {}
        }
    }

    Ok(settings)
}

#[tauri::command]
pub fn save_ai_settings(app: tauri::AppHandle, settings: AiSettings) -> Result<(), String> {
    let conn = ensure_ai_db(&app)?;
    let pairs = [
        ("api_key", &settings.api_key),
        ("base_url", &settings.base_url),
        ("model_id", &settings.model_id),
        ("url_mode", &settings.url_mode),
        ("api_type", &settings.api_type),
        ("provider", &settings.provider),
        ("cli_model", &settings.cli_model),
        ("proxy_url", &settings.proxy_url),
    ];
    for (key, value) in &pairs {
        conn.execute(
            "INSERT INTO ai_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_chat_messages(app: tauri::AppHandle, session_id: i64) -> Result<Vec<ChatMessage>, String> {
    let conn = ensure_ai_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT id, role, content, created_at, meta FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                meta: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// 会话（chat_sessions）管理
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_chat_sessions(app: tauri::AppHandle) -> Result<Vec<ChatSession>, String> {
    let conn = ensure_ai_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT id, title, created_at, updated_at FROM chat_sessions ORDER BY updated_at DESC, id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ChatSession {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_chat_session(app: tauri::AppHandle) -> Result<ChatSession, String> {
    let conn = ensure_ai_db(&app)?;
    conn.execute("INSERT INTO chat_sessions (title) VALUES ('')", [])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, title, created_at, updated_at FROM chat_sessions WHERE id = ?1",
        params![id],
        |row| {
            Ok(ChatSession {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_chat_session(app: tauri::AppHandle, session_id: i64, title: String) -> Result<(), String> {
    let conn = ensure_ai_db(&app)?;
    conn.execute(
        "UPDATE chat_sessions SET title = ?1 WHERE id = ?2",
        params![title.trim(), session_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_chat_session(app: tauri::AppHandle, session_id: i64) -> Result<(), String> {
    let conn = ensure_ai_db(&app)?;
    conn.execute("DELETE FROM chat_messages WHERE session_id = ?1", params![session_id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM chat_sessions WHERE id = ?1", params![session_id])
        .map_err(|e| e.to_string())?;
    drop(conn);
    // 同步丢弃该会话的 Claude Code CLI session
    crate::commands::agent_cli::clear_session_id(&app, &format!("chat:{}", session_id));
    Ok(())
}

fn build_api_url(settings: &AiSettings) -> String {
    if settings.url_mode == "raw" {
        return settings.base_url.clone();
    }
    let base = settings.base_url.trim_end_matches('/');
    if settings.api_type == "openai" {
        format!("{}/v1/chat/completions", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

fn build_http_client(settings: &AiSettings) -> Result<Client, String> {
    let mut builder = Client::builder();
    let proxy = settings.proxy_url.trim();
    if !proxy.is_empty() {
        builder = builder
            .proxy(reqwest::Proxy::all(proxy).map_err(|e| format!("代理配置无效: {}", e))?);
    }
    builder.build().map_err(|e| e.to_string())
}

fn apply_auth_headers(
    builder: reqwest::RequestBuilder,
    settings: &AiSettings,
) -> reqwest::RequestBuilder {
    if settings.api_type == "openai" {
        builder.header("Authorization", format!("Bearer {}", settings.api_key))
    } else {
        builder
            .header("x-api-key", &settings.api_key)
            .header("anthropic-version", "2023-06-01")
    }
}

fn build_disk_context(scan_state: &State<'_, ScanState>) -> String {
    let guard = match scan_state.0.lock() {
        Ok(g) => g,
        Err(_) => return "No scan data available.".into(),
    };
    let tree = match guard.as_ref() {
        Some(t) => t,
        None => return "No scan data available. Please run a disk scan first.".into(),
    };

    let root = tree.node(ROOT);
    let mut lines = Vec::new();
    lines.push(format!(
        "Current scan root: {}\nTotal size: {} bytes ({:.2} GB)\nFiles: {}, Directories: {}",
        tree.root_path(),
        root.size(),
        root.size() as f64 / 1_073_741_824.0,
        root.files(),
        root.subdirs(),
    ));

    let children: Vec<_> = tree.children_by_size(ROOT).into_iter().take(20).collect();

    if !children.is_empty() {
        lines.push("\nTop children by size:".into());
        for &child in &children {
            let node = tree.node(child);
            let kind = if node.is_dir() { "DIR" } else { "FILE" };
            lines.push(format!(
                "  [{kind}] {} — {:.2} GB ({} bytes)",
                tree.name(child),
                node.size() as f64 / 1_073_741_824.0,
                node.size(),
            ));
        }
    }

    lines.join("\n")
}

/// 为 CLI provider 生成深层上下文（多层摘要 + 可 Grep 的全量树 dump）。
/// std Mutex 不能跨 await 持有，所以在 await 之前同步生成好字符串。
fn build_cli_scan_context(scan_state: &State<'_, ScanState>) -> (String, String) {
    let guard = match scan_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ("（扫描数据不可用）".into(), String::new()),
    };
    match guard.as_ref() {
        Some(tree) => (
            crate::commands::agent_workspace::build_deep_scan_summary(tree),
            crate::commands::agent_workspace::build_scan_tree_dump(tree),
        ),
        None => ("（尚未扫描，请先在 Disky 中扫描磁盘）".into(), String::new()),
    }
}

#[tauri::command]
pub async fn send_chat_message(
    app: tauri::AppHandle,
    message: String,
    session_id: i64,
    scan_state: State<'_, ScanState>,
) -> Result<(), String> {
    // Load settings
    let settings = load_ai_settings(app.clone())?;

    // Claude Code (本地 CLI) provider 走 agent_cli 适配器
    if settings.provider == "claude-code" {
        let disk_context = build_disk_context(&scan_state);
        let (deep_summary, scan_tree) = build_cli_scan_context(&scan_state);
        crate::commands::agent_cli::run_chat_cli(
            app.clone(), message, settings.clone(), disk_context, deep_summary, scan_tree, session_id,
        )
        .await?;
        maybe_generate_title(&app, &settings, session_id).await;
        return Ok(());
    }

    if settings.api_key.is_empty() {
        return Err("Please configure your API key in AI settings.".into());
    }

    // Build disk context
    let disk_context = build_disk_context(&scan_state);
    let system_prompt = format!(
        "You are Disky AI assistant, a helpful disk analysis assistant built into the Disky disk analyzer tool. \
        You help users understand their disk usage, identify large files/folders, and suggest cleanup strategies.\n\n\
        Current disk scan data:\n{}\n\n\
        Guidelines:\n\
        - Answer in the same language as the user's message.\n\
        - Be concise and actionable.\n\
        - When referencing paths or sizes, use the scan data provided above.\n\
        - If no scan data is available, let the user know they should run a scan first.",
        disk_context
    );

    // Save user message to DB
    {
        let conn = ensure_ai_db(&app)?;
        conn.execute(
            "INSERT INTO chat_messages (role, content, session_id) VALUES ('user', ?1, ?2)",
            params![message, session_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // Load recent history (last 20 messages of THIS session)
    let history = {
        let conn = ensure_ai_db(&app)?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM (
                    SELECT role, content, id FROM chat_messages WHERE session_id = ?1 ORDER BY id DESC LIMIT 20
                ) sub ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    // Build messages array
    let messages: Vec<serde_json::Value> = history
        .iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "content": content,
            })
        })
        .collect();

    // Build request URL
    let url = build_api_url(&settings);

    // Build request body
    let body = if settings.api_type == "openai" {
        let mut all_messages = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt,
        })];
        all_messages.extend(messages);
        serde_json::json!({
            "model": settings.model_id,
            "max_tokens": 4096,
            "messages": all_messages,
            "stream": true,
        })
    } else {
        serde_json::json!({
            "model": settings.model_id,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": messages,
            "stream": true,
        })
    };

    // Send HTTP request
    let client = build_http_client(&settings)?;
    let response = apply_auth_headers(client.post(&url), &settings)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let error_msg = format!("API error ({}): {}", status, text);
        let _ = app.emit(
            "chat-stream",
            ChatStreamEvent {
                delta: String::new(),
                done: true,
                error: Some(error_msg.clone()),
            },
        );
        return Err(error_msg);
    }

    // Stream SSE response
    let mut stream = response.bytes_stream();
    let mut full_reply = String::new();
    let mut buffer = String::new();
    let mut done_emitted = false;
    let is_openai = settings.api_type == "openai";

    'outer: while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                if !done_emitted {
                    let _ = app.emit(
                        "chat-stream",
                        ChatStreamEvent {
                            delta: String::new(),
                            done: true,
                            error: Some(format!("Stream error: {}", e)),
                        },
                    );
                    done_emitted = true;
                }
                break;
            }
        };

        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete SSE lines
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer = buffer[pos + 1..].to_string();

            if !line.starts_with("data: ") {
                continue;
            }
            let json_str = line[6..].trim();

            // [DONE] — OpenAI / many compatible APIs use this to signal stream end
            if json_str == "[DONE]" {
                if !done_emitted {
                    let _ = app.emit(
                        "chat-stream",
                        ChatStreamEvent {
                            delta: String::new(),
                            done: true,
                            error: None,
                        },
                    );
                    done_emitted = true;
                }
                break 'outer;
            }

            let parsed: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Top-level error object (works for both API types)
            if let Some(err) = parsed["error"]["message"].as_str() {
                if !done_emitted {
                    let _ = app.emit(
                        "chat-stream",
                        ChatStreamEvent {
                            delta: String::new(),
                            done: true,
                            error: Some(err.to_string()),
                        },
                    );
                    done_emitted = true;
                }
                break 'outer;
            }

            if is_openai {
                // OpenAI / compatible: choices[0].delta.content
                if let Some(text) = parsed["choices"][0]["delta"]["content"].as_str() {
                    if !text.is_empty() {
                        full_reply.push_str(text);
                        let _ = app.emit(
                            "chat-stream",
                            ChatStreamEvent {
                                delta: text.to_string(),
                                done: false,
                                error: None,
                            },
                        );
                    }
                }
                // finish_reason: JSON null means still streaming; any real value means done
                let finish = &parsed["choices"][0]["finish_reason"];
                if !finish.is_null() {
                    if let Some(r) = finish.as_str() {
                        // some APIs send the string "null" — treat that as still streaming
                        if r != "null" && !done_emitted {
                            let _ = app.emit(
                                "chat-stream",
                                ChatStreamEvent {
                                    delta: String::new(),
                                    done: true,
                                    error: None,
                                },
                            );
                            done_emitted = true;
                        }
                    }
                }
            } else {
                // Anthropic SSE: content_block_delta / message_stop / error
                let event_type = parsed["type"].as_str().unwrap_or("");
                match event_type {
                    "content_block_delta" => {
                        if let Some(text) = parsed["delta"]["text"].as_str() {
                            full_reply.push_str(text);
                            let _ = app.emit(
                                "chat-stream",
                                ChatStreamEvent {
                                    delta: text.to_string(),
                                    done: false,
                                    error: None,
                                },
                            );
                        }
                    }
                    "message_stop" => {
                        if !done_emitted {
                            let _ = app.emit(
                                "chat-stream",
                                ChatStreamEvent {
                                    delta: String::new(),
                                    done: true,
                                    error: None,
                                },
                            );
                            done_emitted = true;
                        }
                    }
                    "error" => {
                        let err_msg = parsed["error"]["message"]
                            .as_str()
                            .unwrap_or("Unknown API error");
                        if !done_emitted {
                            let _ = app.emit(
                                "chat-stream",
                                ChatStreamEvent {
                                    delta: String::new(),
                                    done: true,
                                    error: Some(err_msg.to_string()),
                                },
                            );
                            done_emitted = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Fallback: if stream ended without any done signal, emit one now
    if !done_emitted {
        let _ = app.emit(
            "chat-stream",
            ChatStreamEvent {
                delta: String::new(),
                done: true,
                error: None,
            },
        );
    }

    // Save assistant reply to DB
    if !full_reply.is_empty() {
        let conn = ensure_ai_db(&app)?;
        conn.execute(
            "INSERT INTO chat_messages (role, content, session_id) VALUES ('assistant', ?1, ?2)",
            params![full_reply, session_id],
        )
        .map_err(|e| e.to_string())?;
    }

    maybe_generate_title(&app, &settings, session_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// 会话标题自动生成（首轮对话后用小模型概括）
// ---------------------------------------------------------------------------

fn clean_title(raw: &str) -> String {
    let line = raw.trim().lines().next().unwrap_or("").trim();
    let trimmed = line
        .trim_matches(|c| c == '"' || c == '\'' || c == '「' || c == '」' || c == '《' || c == '》' || c == '。' || c == '.')
        .trim();
    trimmed.chars().take(24).collect()
}

/// 手动触发标题重新总结（不管原标题是否已有）
#[tauri::command]
pub async fn regenerate_chat_title(app: tauri::AppHandle, session_id: i64) -> Result<(), String> {
    let settings = load_ai_settings(app.clone())?;
    generate_title(&app, &settings, session_id, true).await;
    Ok(())
}

/// 首轮对话后自动总结标题（仅当标题为空时）
async fn maybe_generate_title(app: &tauri::AppHandle, settings: &AiSettings, session_id: i64) {
    generate_title(app, settings, session_id, false).await;
}

/// 用小模型把会话前几轮对话概括成短标题。`force=false` 时仅在标题为空才生成。
/// 只看首轮会把"你好啊"这种寒暄当标题——必须喂上后续真正聊的内容。
/// 自动模式失败回退首条用户消息截断；手动模式（force）失败保留原标题。
async fn generate_title(app: &tauri::AppHandle, settings: &AiSettings, session_id: i64, force: bool) {
    let truncate = |s: &str, n: usize| -> String { s.chars().take(n).collect() };

    let convo: Vec<(String, String)> = {
        let Ok(conn) = ensure_ai_db(app) else { return };
        if !force {
            let title: String = conn
                .query_row("SELECT title FROM chat_sessions WHERE id = ?1", params![session_id], |r| r.get(0))
                .unwrap_or_default();
            if !title.trim().is_empty() {
                return;
            }
        }
        let Ok(mut stmt) = conn.prepare(
            "SELECT role, content FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC LIMIT 8",
        ) else {
            return;
        };
        let rows = stmt
            .query_map(params![session_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .ok();
        match rows {
            Some(rows) => rows.flatten().collect(),
            None => return,
        }
    };
    if convo.is_empty() {
        return;
    }

    let transcript: String = convo
        .iter()
        .map(|(role, content)| {
            let who = if role == "user" { "用户" } else { "助手" };
            format!("{}：{}", who, truncate(content, 300))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "以下是磁盘清理助手与用户的对话片段。用不超过12个汉字概括这段对话的核心主题作为会话标题——\
        优先概括具体在做的事（如「D盘清理方案」「node_modules排查」），忽略寒暄和客套。\
        只返回标题本身，不要引号、标点或解释。\n\n{}",
        transcript
    );

    let generated = if settings.provider == "claude-code" {
        crate::commands::agent_cli::generate_title_cli(settings, &prompt).await
    } else {
        generate_title_api(settings, &prompt).await
    };

    let title = match generated.map(|t| clean_title(&t)).filter(|t| !t.is_empty()) {
        Some(t) => t,
        None if force => return, // 手动重试失败：保留原标题，不要覆盖成截断消息
        None => {
            let first_user = convo
                .iter()
                .find(|(role, _)| role == "user")
                .map(|(_, c)| c.as_str())
                .unwrap_or("");
            if first_user.trim().is_empty() {
                return;
            }
            truncate(first_user, 16)
        }
    };

    if let Ok(conn) = ensure_ai_db(app) {
        let _ = conn.execute(
            "UPDATE chat_sessions SET title = ?1, updated_at = datetime('now','localtime') WHERE id = ?2",
            params![title, session_id],
        );
    }
    let _ = app.emit("chat-session-updated", session_id);
}

/// API provider 的标题生成：非流式短请求
async fn generate_title_api(settings: &AiSettings, prompt: &str) -> Option<String> {
    if settings.api_key.is_empty() {
        return None;
    }
    let url = build_api_url(settings);
    let client = build_http_client(settings).ok()?;
    let body = if settings.api_type == "openai" {
        serde_json::json!({
            "model": settings.model_id,
            "max_tokens": 40,
            "messages": [{ "role": "user", "content": prompt }],
        })
    } else {
        serde_json::json!({
            "model": settings.model_id,
            "max_tokens": 40,
            "messages": [{ "role": "user", "content": prompt }],
        })
    };
    let resp = apply_auth_headers(client.post(&url), settings)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let text = if settings.api_type == "openai" {
        v["choices"][0]["message"]["content"].as_str()
    } else {
        v["content"][0]["text"].as_str()
    };
    text.map(|s| s.to_string())
}

/// 一次性提问（非流式），按设置走 API 或 Claude Code，返回模型的文字回复
pub(crate) async fn complete_once(settings: &AiSettings, prompt: &str, max_tokens: u32) -> Result<String, String> {
    if settings.provider == "claude-code" {
        return crate::commands::agent_cli::prompt_once_cli(settings, prompt, None, std::time::Duration::from_secs(180))
            .await;
    }
    if settings.api_key.is_empty() {
        return Err("还没有配置 AI：请在 AI 设置里填写 API Key".into());
    }
    let client = build_http_client(settings)?;
    let body = serde_json::json!({
        "model": settings.model_id,
        "max_tokens": max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
    });
    let resp = apply_auth_headers(client.post(build_api_url(settings)), settings)
        .header("content-type", "application/json")
        // 接口挂住时别让「测试连接」「AI 核对」一直转圈
        .timeout(std::time::Duration::from_secs(120))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 AI 失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("AI 返回错误（{status}）：{}", text.chars().take(300).collect::<String>()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("AI 回复无法解析: {e}"))?;
    let text = if settings.api_type == "openai" {
        v["choices"][0]["message"]["content"].as_str()
    } else {
        v["content"][0]["text"].as_str()
    };
    text.map(str::to_string).ok_or_else(|| "AI 没有返回文字内容".into())
}

#[tauri::command]
pub async fn analyze_paths(
    app: tauri::AppHandle,
    items: Vec<AnalyzePathInput>,
    scan_state: State<'_, ScanState>,
) -> Result<Vec<AnalyzePathResult>, String> {
    let settings = load_ai_settings(app.clone())?;

    if items.is_empty() {
        return Ok(vec![]);
    }

    if settings.provider == "claude-code" {
        let (deep_summary, scan_tree) = build_cli_scan_context(&scan_state);
        return crate::commands::agent_cli::run_analyze_cli(app, items, settings, deep_summary, scan_tree)
            .await;
    }

    if settings.api_key.is_empty() {
        return Err("Please configure your API key in AI settings.".into());
    }

    // Build item list for prompt
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
        待分析项：\n{}\n\n\
        只返回 JSON 数组，无 markdown：\n\
        [{{\"path\":\"...\",\"description\":\"...\",\"priority\":3.5}}]",
        item_lines.join("\n")
    );

    // Build request URL
    let url = build_api_url(&settings);

    let body = serde_json::json!({
        "model": settings.model_id,
        "max_tokens": 4096,
        "messages": [{
            "role": "user",
            "content": prompt,
        }],
        "stream": true,
    });

    let client = build_http_client(&settings)?;
    let response = apply_auth_headers(client.post(&url), &settings)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let err = format!("API error ({}): {}", status, text);
        let _ = app.emit("analyze-stream", AnalyzeStreamEvent { delta: String::new(), done: true, error: Some(err.clone()) });
        return Err(err);
    }

    // Stream response and emit deltas for live preview
    let mut stream = response.bytes_stream();
    let mut full_text = String::new();
    let mut buffer = String::new();
    let is_openai = settings.api_type == "openai";

    'outer: while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("analyze-stream", AnalyzeStreamEvent {
                    delta: String::new(), done: true, error: Some(format!("Stream error: {}", e)),
                });
                break;
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer = buffer[pos + 1..].to_string();

            if !line.starts_with("data: ") { continue; }
            let json_str = line[6..].trim();
            if json_str == "[DONE]" {
                let _ = app.emit("analyze-stream", AnalyzeStreamEvent { delta: String::new(), done: true, error: None });
                break 'outer;
            }

            let parsed: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(err) = parsed["error"]["message"].as_str() {
                let _ = app.emit("analyze-stream", AnalyzeStreamEvent {
                    delta: String::new(), done: true, error: Some(err.to_string()),
                });
                return Err(err.to_string());
            }

            let delta_text = if is_openai {
                parsed["choices"][0]["delta"]["content"].as_str().unwrap_or("").to_string()
            } else if parsed["type"] == "content_block_delta" {
                parsed["delta"]["text"].as_str().unwrap_or("").to_string()
            } else if parsed["type"] == "message_stop" {
                let _ = app.emit("analyze-stream", AnalyzeStreamEvent { delta: String::new(), done: true, error: None });
                break 'outer;
            } else {
                String::new()
            };

            if !delta_text.is_empty() {
                full_text.push_str(&delta_text);
                let _ = app.emit("analyze-stream", AnalyzeStreamEvent {
                    delta: delta_text, done: false, error: None,
                });
            }

            // OpenAI finish_reason
            if is_openai {
                let finish = &parsed["choices"][0]["finish_reason"];
                if !finish.is_null() {
                    if finish.as_str().map_or(false, |r| r != "null") {
                        let _ = app.emit("analyze-stream", AnalyzeStreamEvent { delta: String::new(), done: true, error: None });
                        break 'outer;
                    }
                }
            }
        }
    }

    // Parse accumulated JSON
    let results: Vec<AnalyzePathResult> = match serde_json::from_str(&full_text) {
        Ok(r) => r,
        Err(_) => try_parse_partial_json(&full_text)?,
    };

    Ok(results)
}

/// 从被截断的 JSON 数组中尽可能多地提取完整的对象。
/// 例如 `[{...},{...},{...` → 解析前两个完整对象。
pub(crate) fn try_parse_partial_json(text: &str) -> Result<Vec<AnalyzePathResult>, String> {
    let trimmed = text.trim();
    if !trimmed.starts_with('[') {
        return Err("Response is not a JSON array".into());
    }

    // 追踪大括号深度，找到最后一个完整的顶层 `}` 位置
    let mut last_obj_end: Option<usize> = None;
    let mut brace_depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in trimmed.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => brace_depth += 1,
            '}' if !in_string => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    last_obj_end = Some(i);
                }
            }
            _ => {}
        }
    }

    match last_obj_end {
        Some(pos) => {
            let partial = format!("{}]", &trimmed[..=pos]);
            serde_json::from_str(&partial)
                .map_err(|e| format!("Failed to parse partial results: {}", e))
        }
        None => Err("No complete JSON objects found in truncated response".into()),
    }
}

#[tauri::command]
pub fn save_ai_analysis(
    app: tauri::AppHandle,
    path: String,
    description: String,
    priority: f64,
) -> Result<(), String> {
    let conn = ensure_ai_db(&app)?;
    conn.execute(
        "INSERT INTO ai_analysis (path, description, priority) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET description = excluded.description, priority = excluded.priority, created_at = datetime('now','localtime')",
        params![path, description, priority],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn load_ai_analyses(app: tauri::AppHandle, paths: Vec<String>) -> Result<Vec<AiAnalysis>, String> {
    let conn = ensure_ai_db(&app)?;
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = paths.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT path, description, priority, created_at FROM ai_analysis WHERE path IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let params: Vec<&dyn rusqlite::types::ToSql> = paths.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(AiAnalysis {
                path: row.get(0)?,
                description: row.get(1)?,
                priority: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_all_ai_analyses(app: tauri::AppHandle) -> Result<Vec<AiAnalysis>, String> {
    let conn = ensure_ai_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT path, description, priority, created_at FROM ai_analysis ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AiAnalysis {
                path: row.get(0)?,
                description: row.get(1)?,
                priority: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
