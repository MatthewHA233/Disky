use futures::StreamExt;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::commands::scan::ScanState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub api_key: String,
    pub base_url: String,
    pub model_id: String,
    pub url_mode: String, // "append" | "raw"
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com".into(),
            model_id: "claude-sonnet-4-6".into(),
            url_mode: "append".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamEvent {
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

fn ensure_ai_db(app: &tauri::AppHandle) -> Result<Connection, String> {
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
        );"
    ).map_err(|e| e.to_string())?;
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
pub fn list_chat_messages(app: tauri::AppHandle) -> Result<Vec<ChatMessage>, String> {
    let conn = ensure_ai_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT id, role, content, created_at FROM chat_messages ORDER BY id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_chat_history(app: tauri::AppHandle) -> Result<(), String> {
    let conn = ensure_ai_db(&app)?;
    conn.execute("DELETE FROM chat_messages", [])
        .map_err(|e| e.to_string())?;
    Ok(())
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

    let mut lines = Vec::new();
    lines.push(format!(
        "Current scan root: {}\nTotal size: {} bytes ({:.2} GB)\nFiles: {}, Directories: {}",
        tree.path,
        tree.logical_size,
        tree.logical_size as f64 / 1_073_741_824.0,
        tree.files,
        tree.subdirs,
    ));

    let mut children: Vec<_> = tree.children.iter().collect();
    children.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));
    children.truncate(20);

    if !children.is_empty() {
        lines.push("\nTop children by size:".into());
        for child in &children {
            let kind = if child.is_dir { "DIR" } else { "FILE" };
            lines.push(format!(
                "  [{kind}] {} — {:.2} GB ({} bytes)",
                child.name,
                child.logical_size as f64 / 1_073_741_824.0,
                child.logical_size,
            ));
        }
    }

    lines.join("\n")
}

#[tauri::command]
pub async fn send_chat_message(
    app: tauri::AppHandle,
    message: String,
    scan_state: State<'_, ScanState>,
) -> Result<(), String> {
    // Load settings
    let settings = load_ai_settings(app.clone())?;
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
            "INSERT INTO chat_messages (role, content) VALUES ('user', ?1)",
            params![message],
        )
        .map_err(|e| e.to_string())?;
    }

    // Load recent history (last 20 messages)
    let history = {
        let conn = ensure_ai_db(&app)?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM (
                    SELECT role, content, id FROM chat_messages ORDER BY id DESC LIMIT 20
                ) sub ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
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
    let url = if settings.url_mode == "raw" {
        settings.base_url.clone()
    } else {
        let base = settings.base_url.trim_end_matches('/');
        format!("{}/v1/messages", base)
    };

    // Build request body
    let body = serde_json::json!({
        "model": settings.model_id,
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": messages,
        "stream": true,
    });

    // Send HTTP request
    let client = Client::new();
    let response = client
        .post(&url)
        .header("x-api-key", &settings.api_key)
        .header("anthropic-version", "2023-06-01")
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

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit(
                    "chat-stream",
                    ChatStreamEvent {
                        delta: String::new(),
                        done: true,
                        error: Some(format!("Stream error: {}", e)),
                    },
                );
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
            let json_str = &line[6..];
            if json_str == "[DONE]" {
                continue;
            }

            let parsed: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

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
                    let _ = app.emit(
                        "chat-stream",
                        ChatStreamEvent {
                            delta: String::new(),
                            done: true,
                            error: None,
                        },
                    );
                }
                "error" => {
                    let err_msg = parsed["error"]["message"]
                        .as_str()
                        .unwrap_or("Unknown API error");
                    let _ = app.emit(
                        "chat-stream",
                        ChatStreamEvent {
                            delta: String::new(),
                            done: true,
                            error: Some(err_msg.to_string()),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    // Save assistant reply to DB
    if !full_reply.is_empty() {
        let conn = ensure_ai_db(&app)?;
        conn.execute(
            "INSERT INTO chat_messages (role, content) VALUES ('assistant', ?1)",
            params![full_reply],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn analyze_paths(
    app: tauri::AppHandle,
    items: Vec<AnalyzePathInput>,
) -> Result<Vec<AnalyzePathResult>, String> {
    let settings = load_ai_settings(app.clone())?;
    if settings.api_key.is_empty() {
        return Err("Please configure your API key in AI settings.".into());
    }

    if items.is_empty() {
        return Ok(vec![]);
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
    let url = if settings.url_mode == "raw" {
        settings.base_url.clone()
    } else {
        let base = settings.base_url.trim_end_matches('/');
        format!("{}/v1/messages", base)
    };

    let body = serde_json::json!({
        "model": settings.model_id,
        "max_tokens": 4096,
        "messages": [{
            "role": "user",
            "content": prompt,
        }],
        "stream": false,
    });

    let client = Client::new();
    let response = client
        .post(&url)
        .header("x-api-key", &settings.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API error ({}): {}", status, text));
    }

    let resp_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Extract text content from Claude API response
    let text_content = resp_json["content"]
        .as_array()
        .and_then(|arr| arr.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| "No text content in API response".to_string())?;

    // Parse JSON array from response text (with truncation resilience)
    let results: Vec<AnalyzePathResult> = match serde_json::from_str(text_content) {
        Ok(r) => r,
        Err(_) => try_parse_partial_json(text_content)?,
    };

    Ok(results)
}

/// 从被截断的 JSON 数组中尽可能多地提取完整的对象。
/// 例如 `[{...},{...},{...` → 解析前两个完整对象。
fn try_parse_partial_json(text: &str) -> Result<Vec<AnalyzePathResult>, String> {
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
