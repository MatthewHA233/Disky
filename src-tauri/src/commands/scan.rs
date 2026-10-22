use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::Disks;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::walk::{scan_tree_progressive, ScanProgress};
use crate::scanner::{normalize_path, DirEntry, ScanTree, ROOT};

#[derive(Serialize)]
pub struct DriveInfo {
    mount_point: String,
    total_space: u64,
    available_space: u64,
}

#[derive(Serialize, Clone)]
struct ScanProgressEvent {
    files_scanned: u64,
    dirs_scanned: u64,
    total_size: u64,
}

#[derive(Serialize, Clone)]
struct ScanTreeEvent {
    children: Vec<DirEntry>,
    scanning: bool,
    current_dir: String,
}

/// Holds the in-memory scan tree.
pub struct ScanState(pub Mutex<Option<ScanTree>>);

/// 单页子项上限：虚拟列表按需翻页，单次 IPC 不必传整个超大目录。
const MAX_PAGE_SIZE: usize = 5000;

#[derive(Serialize)]
pub struct ChildrenPage {
    /// 子项总数（含未返回的）
    total: usize,
    /// 父目录自身大小（前端算占比用，分页时不必凑齐全部子项再求和）
    parent_size: u64,
    entries: Vec<DirEntry>,
}

/// macOS：DMG 安装镜像会挂载成 /Volumes 下的只读卷，不是可清理的磁盘。
/// 用 statfs 的 MNT_RDONLY 标志识别并过滤。
#[cfg(target_os = "macos")]
fn is_read_only_volume(mp: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(mp.as_os_str().as_bytes()) else {
        return false;
    };
    let mut st = std::mem::MaybeUninit::<libc::statfs>::uninit();
    unsafe {
        libc::statfs(c.as_ptr(), st.as_mut_ptr()) == 0
            && (st.assume_init().f_flags & libc::MNT_RDONLY as u32) != 0
    }
}

#[cfg(target_os = "macos")]
fn keep_drive(d: &sysinfo::Disk) -> bool {
    // APFS 会挂出大量系统卷（/System/Volumes/Preboot、VM、Update 等），
    // 只保留启动卷 "/" 和外接/其他可写卷 /Volumes/*
    let mp = d.mount_point().to_string_lossy();
    mp == "/"
        || (mp.starts_with("/Volumes/")
            && !mp.contains("/.timemachine")
            && !is_read_only_volume(d.mount_point()))
}

#[cfg(not(target_os = "macos"))]
fn keep_drive(_d: &sysinfo::Disk) -> bool {
    true
}

#[tauri::command]
pub fn list_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<DriveInfo> = disks
        .iter()
        .filter(|d| keep_drive(d))
        .map(|d| DriveInfo {
            mount_point: d.mount_point().to_string_lossy().to_string(),
            total_space: d.total_space(),
            available_space: d.available_space(),
        })
        .collect();
    // 同一卷可能出现多个挂载条目（APFS 快照等）— 按挂载点去重
    drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    drives.dedup_by(|a, b| a.mount_point == b.mount_point);
    drives
}

#[tauri::command]
pub async fn scan_disk(
    app: AppHandle,
    path: String,
    state: State<'_, ScanState>,
) -> Result<u64, String> {
    let progress = Arc::new(ScanProgress::default());

    let norm = normalize_path(&path);

    // Start periodic progress reporter (200 ms interval)
    let done = Arc::new(AtomicBool::new(false));
    let progress_reporter = {
        let progress = progress.clone();
        let app = app.clone();
        let done = done.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let _ = app.emit(
                    "scan-progress",
                    ScanProgressEvent {
                        files_scanned: progress.files_scanned.load(Ordering::Relaxed),
                        dirs_scanned: progress.dirs_scanned.load(Ordering::Relaxed),
                        total_size: progress.total_size.load(Ordering::Relaxed),
                    },
                );
            }
        })
    };

    let tree = {
        let norm = norm.clone();
        let progress = progress.clone();
        let app_clone = app.clone();
        tokio::task::spawn_blocking(move || {
            scan_tree_progressive(&norm, &progress, |children| {
                let _ = app_clone.emit(
                    "scan-tree",
                    ScanTreeEvent {
                        children,
                        scanning: true,
                        current_dir: String::new(),
                    },
                );
            })
        })
        .await
        .map_err(|e| format!("scan thread error: {e}"))?
    };

    // Stop progress reporter
    done.store(true, Ordering::Relaxed);
    let _ = progress_reporter.await;

    // Emit final progress
    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            files_scanned: progress.files_scanned.load(Ordering::Relaxed),
            dirs_scanned: progress.dirs_scanned.load(Ordering::Relaxed),
            total_size: progress.total_size.load(Ordering::Relaxed),
        },
    );

    // Emit final tree
    let (_, snapshot) = tree.child_entries(ROOT, 0, usize::MAX);
    let _ = app.emit(
        "scan-tree",
        ScanTreeEvent {
            children: snapshot,
            scanning: false,
            current_dir: String::new(),
        },
    );

    let count = tree.node_count();
    *state.0.lock().map_err(|e| e.to_string())? = Some(tree);
    Ok(count)
}

#[tauri::command]
pub fn get_children(
    parent_path: String,
    top_n: usize,
    state: State<'_, ScanState>,
) -> Result<Vec<DirEntry>, String> {
    children_page(&state, &parent_path, 0, top_n).map(|p| p.entries)
}

/// 分页获取子项（按大小降序），供虚拟列表滚动到末尾时继续加载。
#[tauri::command]
pub fn get_children_page(
    parent_path: String,
    offset: usize,
    limit: usize,
    state: State<'_, ScanState>,
) -> Result<ChildrenPage, String> {
    children_page(&state, &parent_path, offset, limit)
}

fn children_page(
    state: &ScanState,
    parent_path: &str,
    offset: usize,
    limit: usize,
) -> Result<ChildrenPage, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_ref().ok_or("No scan data. Run a scan first.")?;

    let norm = normalize(parent_path);
    let id = tree
        .find(&norm)
        .ok_or_else(|| format!("Path not found: {norm}"))?;
    let (total, entries) = tree.child_entries(id, offset, limit.min(MAX_PAGE_SIZE));
    Ok(ChildrenPage {
        total,
        parent_size: tree.node(id).size(),
        entries,
    })
}

pub(crate) fn normalize(p: &str) -> String {
    normalize_path(p)
}
