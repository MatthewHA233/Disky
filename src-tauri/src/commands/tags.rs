use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub is_preset: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileTag {
    pub id: i64,
    pub path: String,
    pub tag_id: i64,
    pub tag_name: String,
    pub tag_color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaggedPath {
    pub path: String,
    pub name: String,
    pub tag_id: i64,
    pub tag_name: String,
    pub tag_color: String,
    pub tagged_at: String,
}

fn ensure_tags_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("disky.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT '#C9A84C',
            is_preset INTEGER NOT NULL DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS file_tags (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL,
            tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            created_at TEXT DEFAULT (datetime('now','localtime')),
            UNIQUE(path, tag_id)
        );
        CREATE INDEX IF NOT EXISTS idx_file_tags_path ON file_tags(path);
        CREATE INDEX IF NOT EXISTS idx_file_tags_tag ON file_tags(tag_id);
        PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| e.to_string())?;

    // Insert preset tags if table is empty
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    if count == 0 {
        conn.execute_batch(
            "INSERT INTO tags (name, color, is_preset) VALUES ('必清', '#E74C3C', 1);
             INSERT INTO tags (name, color, is_preset) VALUES ('可清理', '#E67E22', 1);
             INSERT INTO tags (name, color, is_preset) VALUES ('备份', '#3498DB', 1);",
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(conn)
}

#[tauri::command]
pub fn list_tags(app: tauri::AppHandle) -> Result<Vec<Tag>, String> {
    let conn = ensure_tags_db(&app)?;
    let mut stmt = conn
        .prepare("SELECT id, name, color, is_preset, created_at FROM tags ORDER BY is_preset DESC, id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                is_preset: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(app: tauri::AppHandle, name: String, color: String) -> Result<Tag, String> {
    let conn = ensure_tags_db(&app)?;
    conn.execute(
        "INSERT INTO tags (name, color, is_preset) VALUES (?1, ?2, 0)",
        params![name, color],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    let tag = conn
        .query_row(
            "SELECT id, name, color, is_preset, created_at FROM tags WHERE id = ?1",
            params![id],
            |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    is_preset: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(tag)
}

#[tauri::command]
pub fn rename_tag(app: tauri::AppHandle, id: i64, name: String) -> Result<(), String> {
    let conn = ensure_tags_db(&app)?;
    conn.execute(
        "UPDATE tags SET name = ?1 WHERE id = ?2",
        params![name, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_tag(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let conn = ensure_tags_db(&app)?;
    let is_preset: bool = conn
        .query_row(
            "SELECT is_preset FROM tags WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0).map(|v| v != 0),
        )
        .map_err(|e| e.to_string())?;

    if is_preset {
        return Err("Cannot delete preset tag".into());
    }

    conn.execute("DELETE FROM file_tags WHERE tag_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_tag(app: tauri::AppHandle, path: String, tag_id: i64) -> Result<bool, String> {
    let conn = ensure_tags_db(&app)?;
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM file_tags WHERE path = ?1 AND tag_id = ?2",
            params![path, tag_id],
            |row| row.get::<_, i64>(0).map(|v| v > 0),
        )
        .map_err(|e| e.to_string())?;

    if exists {
        conn.execute(
            "DELETE FROM file_tags WHERE path = ?1 AND tag_id = ?2",
            params![path, tag_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        conn.execute(
            "INSERT INTO file_tags (path, tag_id) VALUES (?1, ?2)",
            params![path, tag_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
pub fn get_tags_for_paths(app: tauri::AppHandle, paths: Vec<String>) -> Result<Vec<FileTag>, String> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let conn = ensure_tags_db(&app)?;
    let placeholders: Vec<String> = (1..=paths.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT ft.id, ft.path, ft.tag_id, t.name, t.color, ft.created_at
         FROM file_tags ft
         JOIN tags t ON t.id = ft.tag_id
         WHERE ft.path IN ({})
         ORDER BY ft.path, t.id",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let params: Vec<&dyn rusqlite::types::ToSql> = paths.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(FileTag {
                id: row.get(0)?,
                path: row.get(1)?,
                tag_id: row.get(2)?,
                tag_name: row.get(3)?,
                tag_color: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tagged_paths(app: tauri::AppHandle, tag_id: Option<i64>) -> Result<Vec<TaggedPath>, String> {
    let conn = ensure_tags_db(&app)?;

    let sql = match tag_id {
        Some(_) =>
            "SELECT ft.path, ft.tag_id, t.name, t.color, ft.created_at
             FROM file_tags ft
             JOIN tags t ON t.id = ft.tag_id
             WHERE ft.tag_id = ?1
             ORDER BY ft.created_at DESC",
        None =>
            "SELECT ft.path, ft.tag_id, t.name, t.color, ft.created_at
             FROM file_tags ft
             JOIN tags t ON t.id = ft.tag_id
             ORDER BY t.id, ft.created_at DESC",
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

    let map_row = |row: &rusqlite::Row| {
        let path: String = row.get(0)?;
        let name = path
            .rsplit_once('\\')
            .or_else(|| path.rsplit_once('/'))
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| path.clone());
        Ok(TaggedPath {
            path,
            name,
            tag_id: row.get(1)?,
            tag_name: row.get(2)?,
            tag_color: row.get(3)?,
            tagged_at: row.get(4)?,
        })
    };

    let results: Vec<TaggedPath> = if let Some(tid) = tag_id {
        let mut rows = stmt.query(params![tid]).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            out.push(map_row(row).map_err(|e: rusqlite::Error| e.to_string())?);
        }
        out
    } else {
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            out.push(map_row(row).map_err(|e: rusqlite::Error| e.to_string())?);
        }
        out
    };

    Ok(results)
}
