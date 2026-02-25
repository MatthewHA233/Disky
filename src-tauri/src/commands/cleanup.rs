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

#[tauri::command]
pub fn get_items_info(paths: Vec<String>) -> Vec<ItemInfo> {
    paths.into_iter().filter_map(|p| {
        let meta = fs::metadata(&p).ok()?;
        let size = if meta.is_dir() { dir_size(Path::new(&p)) } else { meta.len() };
        Some(ItemInfo { path: p, size, is_dir: meta.is_dir() })
    }).collect()
}

#[tauri::command]
pub fn delete_items(app: tauri::AppHandle, paths: Vec<String>, to_trash: bool) -> Vec<DeleteResult> {
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

fn is_protected(p: &str) -> bool {
    let norm = p.replace('/', "\\");
    // Check if path is a root-level system directory like C:\Windows
    for prot in PROTECTED {
        if norm.len() > 3 && norm[3..].eq_ignore_ascii_case(prot) {
            return true;
        }
        if norm[3..].to_ascii_lowercase().starts_with(&prot.to_ascii_lowercase())
            && norm.as_bytes().get(3 + prot.len()) == Some(&b'\\')
        {
            return true;
        }
    }
    false
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path).map(|entries| {
        entries.filter_map(|e| e.ok()).map(|e| {
            let meta = e.metadata().ok();
            if meta.as_ref().map_or(false, |m| m.is_dir()) { dir_size(&e.path()) }
            else { meta.map_or(0, |m| m.len()) }
        }).sum()
    }).unwrap_or(0)
}
