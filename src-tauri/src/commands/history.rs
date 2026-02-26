use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::{Manager, State};

use crate::commands::scan::ScanState;
use crate::scanner::{DirEntry, TreeNode};

// ---------------------------------------------------------------------------
// Public response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ScanRecord {
    id: i64,
    drive: String,
    entry_count: i64,
    created_at: String,
    has_tree: bool,
}

#[derive(Serialize)]
pub struct DiffEntry {
    path: String,
    name: String,
    old_size: i64,
    new_size: i64,
    diff: i64,
}

#[derive(Serialize)]
pub struct LoadScanResult {
    drive: String,
    root_path: String,
    children: Vec<DirEntry>,
}

// ---------------------------------------------------------------------------
// Binary-serializable mirror of TreeNode (TreeNode itself uses #[serde(skip)]
// on `children`, so we need a separate struct for full serialization).
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct StorableNode {
    name: String,
    path: String,
    logical_size: u64,
    files: u64,
    subdirs: u64,
    is_dir: bool,
    children: Vec<StorableNode>,
}

impl StorableNode {
    fn from_tree(node: &TreeNode) -> Self {
        StorableNode {
            name: node.name.clone(),
            path: node.path.clone(),
            logical_size: node.logical_size,
            files: node.files,
            subdirs: node.subdirs,
            is_dir: node.is_dir,
            children: node.children.iter().map(StorableNode::from_tree).collect(),
        }
    }

    fn into_tree(self) -> TreeNode {
        TreeNode {
            name: self.name,
            path: self.path,
            logical_size: self.logical_size,
            files: self.files,
            subdirs: self.subdirs,
            is_dir: self.is_dir,
            children: self.children.into_iter().map(StorableNode::into_tree).collect(),
        }
    }
}

fn count_nodes(node: &StorableNode) -> i64 {
    1 + node.children.iter().map(count_nodes).sum::<i64>()
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

fn data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn ensure_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = data_dir(app)?;
    let db_path = dir.join("disky.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scans (
            id INTEGER PRIMARY KEY, drive TEXT, csv_path TEXT,
            entry_count INTEGER,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS scan_entries (
            scan_id INTEGER, path TEXT, name TEXT, logical_size INTEGER,
            FOREIGN KEY(scan_id) REFERENCES scans(id)
        );
        CREATE INDEX IF NOT EXISTS idx_entries_scan ON scan_entries(scan_id);"
    ).map_err(|e| e.to_string())?;
    // New column for bincode file name (ignore error if column already exists)
    let _ = conn.execute_batch("ALTER TABLE scans ADD COLUMN tree_file TEXT DEFAULT ''");
    Ok(conn)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_scan(
    app: tauri::AppHandle,
    drive: String,
    scan_state: State<'_, ScanState>,
) -> Result<i64, String> {
    // Convert tree while holding the lock (pure CPU, fast)
    let storable = {
        let guard = scan_state.0.lock().map_err(|e| e.to_string())?;
        let tree = guard.as_ref().ok_or("没有扫描数据，请先执行扫描。")?;
        StorableNode::from_tree(tree)
    };

    // Heavy I/O in a background thread so the UI stays responsive
    tokio::task::spawn_blocking(move || {
        let count = count_nodes(&storable);
        let blob = bincode::serialize(&storable).map_err(|e| e.to_string())?;

        let dir = data_dir(&app)?;
        let conn = ensure_db(&app)?;

        conn.execute(
            "INSERT INTO scans (drive, entry_count, tree_file) VALUES (?1, ?2, '')",
            params![drive, count],
        ).map_err(|e| e.to_string())?;
        let scan_id = conn.last_insert_rowid();

        let file_name = format!("scan_{}.bin", scan_id);
        let file_path = dir.join(&file_name);
        std::fs::write(&file_path, &blob).map_err(|e| format!("写入文件失败: {e}"))?;

        conn.execute(
            "UPDATE scans SET tree_file = ?1 WHERE id = ?2",
            params![file_name, scan_id],
        ).map_err(|e| e.to_string())?;

        Ok::<i64, String>(scan_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn load_scan(
    app: tauri::AppHandle,
    id: i64,
    scan_state: State<'_, ScanState>,
) -> Result<LoadScanResult, String> {
    let app_clone = app.clone();
    let (drive, tree) = tokio::task::spawn_blocking(move || -> Result<(String, TreeNode), String> {
        let conn = ensure_db(&app_clone)?;
        let (drive, tree_file): (String, String) = conn
            .query_row(
                "SELECT drive, COALESCE(tree_file, '') FROM scans WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;

        if tree_file.is_empty() {
            return Err("此记录为旧格式，无法加载。".to_string());
        }

        let dir = data_dir(&app_clone)?;
        let file_path = dir.join(&tree_file);
        let blob = std::fs::read(&file_path)
            .map_err(|e| format!("读取文件失败: {e}"))?;
        let storable: StorableNode = bincode::deserialize(&blob)
            .map_err(|e| format!("反序列化失败: {e}"))?;
        Ok((drive, storable.into_tree()))
    })
    .await
    .map_err(|e| e.to_string())??;

    let mut children: Vec<DirEntry> = tree.children.iter().map(|c| c.to_dir_entry()).collect();
    children.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));

    let root_path = tree.path.clone();
    *scan_state.0.lock().map_err(|e| e.to_string())? = Some(tree);

    Ok(LoadScanResult { drive, root_path, children })
}

#[tauri::command]
pub fn delete_scan(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let conn = ensure_db(&app)?;

    let tree_file: String = conn
        .query_row(
            "SELECT COALESCE(tree_file, '') FROM scans WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap_or_default();

    if !tree_file.is_empty() {
        let dir = data_dir(&app)?;
        let _ = std::fs::remove_file(dir.join(&tree_file));
    }

    conn.execute("DELETE FROM scan_entries WHERE scan_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM scans WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_scans(app: tauri::AppHandle) -> Result<Vec<ScanRecord>, String> {
    let conn = ensure_db(&app)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, drive, entry_count, created_at, COALESCE(tree_file, '')
             FROM scans ORDER BY id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let tf: String = row.get(4)?;
            Ok(ScanRecord {
                id: row.get(0)?,
                drive: row.get(1)?,
                entry_count: row.get(2)?,
                created_at: row.get(3)?,
                has_tree: !tf.is_empty(),
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Compare
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn compare_scans(
    app: tauri::AppHandle,
    id_a: i64,
    id_b: i64,
    root: String,
) -> Result<Vec<DiffEntry>, String> {
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        let conn = ensure_db(&app_clone)?;
        let norm = root.replace('/', "\\").trim_end_matches('\\').to_string();

        let file_a = tree_file_for(&conn, id_a)?;
        let file_b = tree_file_for(&conn, id_b)?;

        if !file_a.is_empty() && !file_b.is_empty() {
            let dir = data_dir(&app_clone)?;
            let blob_a = std::fs::read(dir.join(&file_a))
                .map_err(|e| format!("读取文件失败: {e}"))?;
            let blob_b = std::fs::read(dir.join(&file_b))
                .map_err(|e| format!("读取文件失败: {e}"))?;
            let tree_a: StorableNode = bincode::deserialize(&blob_a)
                .map_err(|e| format!("反序列化失败: {e}"))?;
            let tree_b: StorableNode = bincode::deserialize(&blob_b)
                .map_err(|e| format!("反序列化失败: {e}"))?;
            return Ok(compare_trees(&tree_a, &tree_b, &norm));
        }

        // Fallback: SQL comparison for old records
        compare_sql(&conn, id_a, id_b, &norm)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn tree_file_for(conn: &Connection, id: i64) -> Result<String, String> {
    conn.query_row(
        "SELECT COALESCE(tree_file, '') FROM scans WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn compare_trees(tree_a: &StorableNode, tree_b: &StorableNode, root: &str) -> Vec<DiffEntry> {
    fn flatten_dirs(node: &StorableNode, prefix: &str, map: &mut HashMap<String, (String, i64)>) {
        if !node.is_dir { return; }
        let np = node.path.to_ascii_lowercase();
        let pp = prefix.to_ascii_lowercase();
        if np.starts_with(&pp) && np != pp {
            map.insert(node.path.clone(), (node.name.clone(), node.logical_size as i64));
        }
        for child in &node.children {
            flatten_dirs(child, prefix, map);
        }
    }

    let mut map_a: HashMap<String, (String, i64)> = HashMap::new();
    let mut map_b: HashMap<String, (String, i64)> = HashMap::new();
    flatten_dirs(tree_a, root, &mut map_a);
    flatten_dirs(tree_b, root, &mut map_b);

    let all_paths: HashSet<&String> = map_a.keys().chain(map_b.keys()).collect();
    let mut diffs = Vec::new();
    for path in all_paths {
        let size_a = map_a.get(path).map(|(_, s)| *s).unwrap_or(0);
        let size_b = map_b.get(path).map(|(_, s)| *s).unwrap_or(0);
        if size_a != size_b {
            let name = map_a.get(path)
                .or_else(|| map_b.get(path))
                .map(|(n, _)| n.as_str())
                .unwrap_or("");
            diffs.push(DiffEntry {
                path: path.clone(),
                name: name.to_string(),
                old_size: size_a,
                new_size: size_b,
                diff: size_b - size_a,
            });
        }
    }

    diffs.sort_by(|a, b| b.diff.unsigned_abs().cmp(&a.diff.unsigned_abs()));
    diffs.truncate(200);
    diffs
}

fn compare_sql(conn: &Connection, id_a: i64, id_b: i64, norm: &str) -> Result<Vec<DiffEntry>, String> {
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
