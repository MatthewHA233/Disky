use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use tauri::Manager;

#[cfg(windows)]
const PROTECTED: &[&str] = &["Windows", "Program Files", "Program Files (x86)", "System Volume Information", "$Recycle.Bin"];

/// macOS：受 SIP/系统签名保护或删了会废系统的根级目录（整个子树拒绝删除）。
/// /Library、/Applications、/Users、/opt 留给用户自行清理（对应 Windows 的 AppData 不保护）。
#[cfg(not(windows))]
const PROTECTED: &[&str] = &["System", "usr", "bin", "sbin", "etc", "var", "private", "dev", "cores"];

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
            let size = if meta.is_dir() {
                dir_size(Path::new(&p))
            } else {
                crate::scanner::file_disk_size(&meta)
            };
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

#[cfg(windows)]
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

#[cfg(not(windows))]
pub(crate) fn is_protected(p: &str) -> bool {
    let norm = p.trim_end_matches('/');
    // 相对路径或根目录 "/" 一律保护
    let Some(rest) = norm.strip_prefix('/') else {
        return true;
    };
    let comps: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if comps.is_empty() {
        return true; // "/"
    }
    // 根级单组件（/Users、/Applications 本身）等价于盘符根，保护
    if comps.len() == 1 {
        return true;
    }
    // /Volumes/X 是其他卷的根，等价于盘符根
    if comps[0].eq_ignore_ascii_case("Volumes") && comps.len() == 2 {
        return true;
    }
    PROTECTED.iter().any(|prot| comps[0].eq_ignore_ascii_case(prot))
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::is_protected;

    #[test]
    fn protects_system_roots_and_volume_roots() {
        assert!(is_protected("/"));
        assert!(is_protected("/Users")); // 根级单组件等价盘符根
        assert!(is_protected("/Volumes/SSD")); // 卷根
        assert!(is_protected("/System/Library/CoreServices"));
        assert!(is_protected("/usr/local/bin"));
        assert!(is_protected("relative/path"));
    }

    #[test]
    fn allows_user_data() {
        assert!(!is_protected("/Users/me/Downloads/old.dmg"));
        assert!(!is_protected("/Users/me/Library/Caches/Yarn"));
        assert!(!is_protected("/Volumes/SSD/backup"));
        assert!(!is_protected("/Applications/OldApp.app"));
    }
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path).map(|entries| {
        entries.filter_map(|e| e.ok()).map(|e| {
            let meta = e.metadata().ok();
            if meta.as_ref().map_or(false, |m| m.is_dir()) { dir_size(&e.path()) }
            else { meta.map_or(0, |m| crate::scanner::file_disk_size(&m)) }
        }).sum()
    }).unwrap_or(0)
}
