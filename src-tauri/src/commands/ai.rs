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
