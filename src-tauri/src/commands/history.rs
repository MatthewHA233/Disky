use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{Manager, State};

use crate::commands::scan::ScanState;
use crate::scanner::TreeNode;

#[derive(Serialize)]
pub struct ScanRecord {
    id: i64,
    drive: String,
    entry_count: i64,
    created_at: String,
}

#[derive(Serialize)]
pub struct DiffEntry {
    path: String,
    name: String,
    old_size: i64,
    new_size: i64,
    diff: i64,
}

fn ensure_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("disky.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scans (
            id INTEGER PRIMARY KEY, drive TEXT, csv_path TEXT, entry_count INTEGER, created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS scan_entries (
            scan_id INTEGER, path TEXT, name TEXT, logical_size INTEGER,
            FOREIGN KEY(scan_id) REFERENCES scans(id)
        );
        CREATE INDEX IF NOT EXISTS idx_entries_scan ON scan_entries(scan_id);"
    ).map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Recursively collect all nodes from the tree into flat (path, name, size) tuples.
fn flatten_tree(node: &TreeNode, out: &mut Vec<(String, String, i64)>) {
    out.push((node.path.clone(), node.name.clone(), node.logical_size as i64));
    for child in &node.children {
        flatten_tree(child, out);
    }
}

#[tauri::command]
pub fn save_scan(app: tauri::AppHandle, drive: String, scan_state: State<'_, ScanState>) -> Result<i64, String> {
    let guard = scan_state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_ref().ok_or("No scan data. Run a scan first.")?;

    let mut entries = Vec::new();
    flatten_tree(tree, &mut entries);
    let count = entries.len() as i64;

    let conn = ensure_db(&app)?;
    conn.execute(
        "INSERT INTO scans (drive, csv_path, entry_count) VALUES (?1, ?2, ?3)",
        params![drive, "", count],
    ).map_err(|e| e.to_string())?;
    let scan_id = conn.last_insert_rowid();

    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    for (path, name, size) in &entries {
        conn.execute(
            "INSERT INTO scan_entries (scan_id, path, name, logical_size) VALUES (?1, ?2, ?3, ?4)",
            params![scan_id, path, name, size],
        ).map_err(|e| e.to_string())?;
    }
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

    Ok(scan_id)
}

#[tauri::command]
pub fn list_scans(app: tauri::AppHandle) -> Result<Vec<ScanRecord>, String> {
    let conn = ensure_db(&app)?;
    let mut stmt = conn.prepare(
        "SELECT id, drive, entry_count, created_at FROM scans ORDER BY id DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(ScanRecord {
            id: row.get(0)?,
            drive: row.get(1)?,
            entry_count: row.get(2)?,
            created_at: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn compare_scans(app: tauri::AppHandle, id_a: i64, id_b: i64, root: String) -> Result<Vec<DiffEntry>, String> {
    let conn = ensure_db(&app)?;
    let norm = root.replace('/', "\\").trim_end_matches('\\').to_string();
    let prefix = format!("{}\\%", norm);

    let mut stmt = conn.prepare(
        "SELECT COALESCE(a.path, b.path) as path, COALESCE(a.name, b.name) as name,
                COALESCE(a.logical_size, 0) as old_size, COALESCE(b.logical_size, 0) as new_size
         FROM (SELECT path, name, logical_size FROM scan_entries WHERE scan_id = ?1 AND path LIKE ?3) a
         FULL OUTER JOIN (SELECT path, name, logical_size FROM scan_entries WHERE scan_id = ?2 AND path LIKE ?3) b
         ON a.path = b.path
         WHERE COALESCE(a.logical_size, 0) != COALESCE(b.logical_size, 0)
         ORDER BY ABS(COALESCE(b.logical_size, 0) - COALESCE(a.logical_size, 0)) DESC
         LIMIT 200"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map(params![id_a, id_b, prefix], |row| {
        let old_size: i64 = row.get(2)?;
        let new_size: i64 = row.get(3)?;
        Ok(DiffEntry {
            path: row.get(0)?, name: row.get(1)?,
            old_size, new_size, diff: new_size - old_size,
        })
    }).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
