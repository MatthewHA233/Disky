use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: i64,
    pub path: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

fn ensure_notes_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("disky.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now','localtime')),
            updated_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_notes_path ON notes(path);"
    ).map_err(|e| e.to_string())?;
    Ok(conn)
}

#[tauri::command]
pub fn save_note(app: tauri::AppHandle, path: String, content: String) -> Result<i64, String> {
    let conn = ensure_notes_db(&app)?;
    conn.execute(
        "INSERT INTO notes (path, content) VALUES (?1, ?2)",
        params![path, content],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn get_notes_for_path(app: tauri::AppHandle, path: String) -> Result<Vec<Note>, String> {
    let conn = ensure_notes_db(&app)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, path, content, created_at, updated_at FROM notes WHERE path = ?1 ORDER BY id DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![path], |row| {
            Ok(Note {
                id: row.get(0)?,
                path: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let conn = ensure_notes_db(&app)?;
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_all_notes(app: tauri::AppHandle) -> Result<Vec<Note>, String> {
    let conn = ensure_notes_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT id, path, content, created_at, updated_at FROM notes ORDER BY id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Note {
                id: row.get(0)?,
                path: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
