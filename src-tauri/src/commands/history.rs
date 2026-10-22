use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use tauri::{Manager, State};

use crate::archive::digest::{self, DriveDigest};
use crate::commands::scan::ScanState;
use crate::scanner::{join_path, DirEntry, NewChild, NodeId, ScanTree, Visit, ROOT};

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
    cleaned_bytes: i64,
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
// 旧版存档格式（v1）：嵌套结构的 bincode。新存档改用 ScanTree::encode，
// 这里只保留读取，以便加载/对比升级前保存的历史记录。
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
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
    /// 按 BFS 转成紧凑扫描树（每个目录的子项连续追加）。
    fn into_scan_tree(self) -> ScanTree {
        let mut tree = ScanTree::new(&self.path);
        tree.set_totals(ROOT, self.logical_size, self.files, self.subdirs);
        let mut queue = VecDeque::from([(ROOT, self)]);
        while let Some((id, node)) = queue.pop_front() {
            let first = tree.push_children(
                id,
                node.children.iter().map(|c| NewChild {
                    name: &c.name,
                    size: c.logical_size,
                    is_dir: c.is_dir,
                }),
            );
            for (i, child) in node.children.into_iter().enumerate() {
                if child.is_dir {
                    let cid = first + i as NodeId;
                    tree.set_totals(cid, child.logical_size, child.files, child.subdirs);
                    queue.push_back((cid, child));
                }
            }
        }
        tree.shrink_to_fit();
        tree
    }
}

/// 读取存档文件：新格式直接解码，旧格式 bincode 反序列化后转换。
fn read_tree_file(path: &Path) -> Result<ScanTree, String> {
    let blob = std::fs::read(path).map_err(|e| format!("读取文件失败: {e}"))?;
    if ScanTree::is_encoded(&blob) {
        return ScanTree::decode(&blob);
    }
    let legacy: StorableNode =
        bincode::deserialize(&blob).map_err(|e| format!("反序列化失败: {e}"))?;
    drop(blob);
    Ok(legacy.into_scan_tree())
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
    // New columns (ignore error if column already exists)
    let _ = conn.execute_batch("ALTER TABLE scans ADD COLUMN tree_file TEXT DEFAULT ''");
    let _ = conn.execute_batch("ALTER TABLE scans ADD COLUMN cleaned_bytes INTEGER DEFAULT 0");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cleanup_logs (
            id INTEGER PRIMARY KEY,
            drive TEXT NOT NULL,
            path TEXT NOT NULL DEFAULT '',
            action_type TEXT NOT NULL DEFAULT 'delete',
            bytes_freed INTEGER NOT NULL,
            session_ts TEXT NOT NULL DEFAULT '',
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );"
    ).map_err(|e| e.to_string())?;
    // Migration for existing cleanup_logs table
    let _ = conn.execute_batch("ALTER TABLE cleanup_logs ADD COLUMN path TEXT NOT NULL DEFAULT ''");
    let _ = conn.execute_batch("ALTER TABLE cleanup_logs ADD COLUMN action_type TEXT NOT NULL DEFAULT 'delete'");
    let _ = conn.execute_batch("ALTER TABLE cleanup_logs ADD COLUMN session_ts TEXT NOT NULL DEFAULT ''");
    let _ = conn.execute_batch("ALTER TABLE cleanup_logs ADD COLUMN description TEXT NOT NULL DEFAULT ''");
    Ok(conn)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_scan(
    app: tauri::AppHandle,
    drive: String,
    cleaned_bytes: i64,
    scan_state: State<'_, ScanState>,
) -> Result<i64, String> {
    // 持锁编码（只是顺序写出几个数组，很快），不再整树克隆。
    // 有刷新/删除留下的垃圾节点时先原地整理，避免「树 + 整理副本 + blob」三份同时在内存
    let (blob, count) = {
        let mut guard = scan_state.0.lock().map_err(|e| e.to_string())?;
        let tree = guard.as_mut().ok_or("没有扫描数据，请先执行扫描。")?;
        tree.compact();
        (tree.encode(), tree.node_count() as i64)
    };

    // Heavy I/O in a background thread so the UI stays responsive
    tokio::task::spawn_blocking(move || {
        let dir = data_dir(&app)?;
        let conn = ensure_db(&app)?;

        conn.execute(
            "INSERT INTO scans (drive, entry_count, tree_file, cleaned_bytes) VALUES (?1, ?2, '', ?3)",
            params![drive, count, cleaned_bytes],
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
    let (drive, tree) = tokio::task::spawn_blocking(move || -> Result<(String, ScanTree), String> {
        let conn = ensure_db(&app_clone)?;
        let (drive, tree_file, saved_at): (String, String, i64) = conn
            .query_row(
                "SELECT drive, COALESCE(tree_file, ''), COALESCE(CAST(strftime('%s', created_at, 'utc') AS INTEGER), 0)
                 FROM scans WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| e.to_string())?;

        if tree_file.is_empty() {
            return Err("此记录为旧格式，无法加载。".to_string());
        }

        let dir = data_dir(&app_clone)?;
        let tree = read_tree_file(&dir.join(&tree_file))?;
        // 打开的记录比档案馆已有的摘要新（或那块盘还没有摘要）时，用它更新档案馆
        if digest::is_volume_root(tree.root_path()) {
            if let Ok(dir) = crate::commands::archive::digest_dir(&app_clone) {
                let d = DriveDigest::build(&tree, saved_at);
                if let Err(e) = digest::save_if_newer(&dir, &d) {
                    eprintln!("保存档案馆摘要失败: {e}");
                }
            }
        }
        Ok((drive, tree))
    })
    .await
    .map_err(|e| e.to_string())??;

    let (_, children) = tree.child_entries(ROOT, 0, usize::MAX);
    let root_path = tree.root_path().to_string();
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
            "SELECT id, drive, entry_count, created_at, COALESCE(tree_file, ''), COALESCE(cleaned_bytes, 0)
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
                cleaned_bytes: row.get(5)?,
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
        let norm = crate::scanner::normalize_path(&root);

        let file_a = tree_file_for(&conn, id_a)?;
        let file_b = tree_file_for(&conn, id_b)?;

        if !file_a.is_empty() && !file_b.is_empty() {
            let dir = data_dir(&app_clone)?;
            let tree_a = read_tree_file(&dir.join(&file_a))?;
            let tree_b = read_tree_file(&dir.join(&file_b))?;
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

/// root 之下（不含 root 自身）的全部目录：路径 → (名字, 大小)。
fn dirs_under(tree: &ScanTree, root: &str) -> HashMap<String, (String, i64)> {
    let mut map = HashMap::new();
    let Some(start) = tree.find(root) else {
        return map;
    };
    tree.walk(start, |id, parent| {
        let node = tree.node(id);
        if !node.is_dir() {
            return Visit::Skip;
        }
        let name = tree.name(id);
        map.insert(join_path(parent, name), (name.to_string(), node.size() as i64));
        Visit::Descend
    });
    map
}

fn compare_trees(tree_a: &ScanTree, tree_b: &ScanTree, root: &str) -> Vec<DiffEntry> {
    let map_a = dirs_under(tree_a, root);
    let map_b = dirs_under(tree_b, root);

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
    let prefix = format!("{}{}%", norm, crate::scanner::SEP);
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

// ---------------------------------------------------------------------------
// Cleanup Logs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CleanupAction {
    id: i64,
    path: String,
    action_type: String,
    bytes_freed: i64,
    description: String,
    created_at: String,
}

#[derive(Serialize)]
pub struct CleanupSession {
    session_ts: String,
    drive: String,
    total_freed: i64,
    actions: Vec<CleanupAction>,
}

#[derive(Serialize)]
pub struct CleanupLogPage {
    sessions: Vec<CleanupSession>,
    total_sessions: i64,
}

#[tauri::command]
pub fn save_cleanup_log(
    app: tauri::AppHandle,
    drive: String,
    path: String,
    action_type: String,
    bytes_freed: i64,
    session_ts: String,
    description: String,
) -> Result<i64, String> {
    let conn = ensure_db(&app)?;
    conn.execute(
        "INSERT INTO cleanup_logs (drive, path, action_type, bytes_freed, session_ts, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![drive, path, action_type, bytes_freed, session_ts, description],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn list_cleanup_logs(
    app: tauri::AppHandle,
    page: i64,
    page_size: i64,
) -> Result<CleanupLogPage, String> {
    let conn = ensure_db(&app)?;

    // Count distinct sessions
    let total_sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT DISTINCT session_ts, drive FROM cleanup_logs WHERE session_ts != '')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // Get paginated session keys
    let offset = (page - 1) * page_size;
    let mut sess_stmt = conn
        .prepare(
            "SELECT session_ts, drive, SUM(bytes_freed) as total_freed
             FROM cleanup_logs
             WHERE session_ts != ''
             GROUP BY session_ts, drive
             ORDER BY MAX(created_at) DESC
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| e.to_string())?;

    let session_keys: Vec<(String, String, i64)> = sess_stmt
        .query_map(params![page_size, offset], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // For each session, fetch its actions
    let mut sessions = Vec::new();
    let mut action_stmt = conn
        .prepare(
            "SELECT id, path, action_type, bytes_freed, COALESCE(description, ''), created_at
             FROM cleanup_logs
             WHERE session_ts = ?1 AND drive = ?2
             ORDER BY id ASC",
        )
        .map_err(|e| e.to_string())?;

    for (session_ts, drive, total_freed) in &session_keys {
        let actions: Vec<CleanupAction> = action_stmt
            .query_map(params![session_ts, drive], |row| {
                Ok(CleanupAction {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    action_type: row.get(2)?,
                    bytes_freed: row.get(3)?,
                    description: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        sessions.push(CleanupSession {
            session_ts: session_ts.clone(),
            drive: drive.clone(),
            total_freed: *total_freed,
            actions,
        });
    }

    Ok(CleanupLogPage { sessions, total_sessions })
}

