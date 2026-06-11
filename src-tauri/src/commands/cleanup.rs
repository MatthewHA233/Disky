use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use tauri::Manager;

const PROTECTED: &[&str] = &["Windows", "Program Files", "Program Files (x86)", "System Volume Information", "$Recycle.Bin"];

#[derive(Serialize)]
pub struct ItemInfo {
    path: String,
    size: u64,
    is_dir: bool,
}

#[derive(Serialize)]
pub struct DeleteResult {
    path: String,
    success: bool,
    error: Option<String>,
}

// 注意：磁盘遍历/删除/回收站（COM）都是重操作，同步命令会在主线程执行并冻结 UI，
// 因此这两个命令必须 async + spawn_blocking。

#[tauri::command]
pub async fn get_items_info(paths: Vec<String>) -> Result<Vec<ItemInfo>, String> {
    tokio::task::spawn_blocking(move || {
        paths.into_iter().filter_map(|p| {
            let meta = fs::metadata(&p).ok()?;
            let size = if meta.is_dir() { dir_size(Path::new(&p)) } else { meta.len() };
            Some(ItemInfo { path: p, size, is_dir: meta.is_dir() })
        }).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_items(app: tauri::AppHandle, paths: Vec<String>, to_trash: bool) -> Result<Vec<DeleteResult>, String> {
    tokio::task::spawn_blocking(move || delete_items_blocking(&app, paths, to_trash))
        .await
        .map_err(|e| e.to_string())
}

fn delete_items_blocking(app: &tauri::AppHandle, paths: Vec<String>, to_trash: bool) -> Vec<DeleteResult> {
    let log_dir = app.path().app_data_dir().unwrap_or_default();
    let _ = fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("cleanup-log.jsonl");

    paths.into_iter().map(|p| {
        if is_protected(&p) {
            return DeleteResult { path: p, success: false, error: Some("Protected system path".into()) };
        }
        let result = if to_trash {
            trash::delete(&p).map_err(|e| e.to_string())
        } else {
            let path = Path::new(&p);
            if path.is_dir() { fs::remove_dir_all(path).map_err(|e| e.to_string()) }
            else { fs::remove_file(path).map_err(|e| e.to_string()) }
        };
        let (success, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        let dr = DeleteResult { path: p, success, error };
        if let Ok(json) = serde_json::to_string(&dr) {
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(f, "{json}");
            }
        }
        dr
    }).collect()
}

pub(crate) fn is_protected(p: &str) -> bool {
    let norm = p.replace('/', "\\");
    // 盘符根（"D:" / "D:\"）或异常短路径一律保护；
    // get() 而非直接切片，避免短路径/非字符边界 panic
    let rest = match norm.get(3..) {
        Some(r) if !r.is_empty() => r,
        _ => return true,
    };
    // Check if path is a root-level system directory like C:\Windows
    for prot in PROTECTED {
        if rest.eq_ignore_ascii_case(prot) {
            return true;
        }
        if let Some(prefix) = rest.get(..prot.len()) {
            if prefix.eq_ignore_ascii_case(prot) && rest.as_bytes().get(prot.len()) == Some(&b'\\') {
                return true;
            }
        }
    }
    false
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path).map(|entries| {
        entries.filter_map(|e| e.ok()).map(|e| {
            let meta = e.metadata().ok();
            if meta.as_ref().map_or(false, |m| m.is_dir()) { dir_size(&e.path()) }
            else { meta.map_or(0, |m| m.len()) }
        }).sum()
    }).unwrap_or(0)
}
