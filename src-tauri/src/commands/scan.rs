use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::Disks;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::walk::{scan_tree, shallow_scan, ScanProgress};
use crate::scanner::{DirEntry, TreeNode};

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
pub struct ScanState(pub Mutex<Option<TreeNode>>);

#[tauri::command]
pub fn list_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|d| DriveInfo {
            mount_point: d.mount_point().to_string_lossy().to_string(),
            total_space: d.total_space(),
            available_space: d.available_space(),
        })
        .collect()
}

#[tauri::command]
pub async fn scan_disk(
    app: AppHandle,
    path: String,
    state: State<'_, ScanState>,
) -> Result<u64, String> {
    let progress = Arc::new(ScanProgress {
        files_scanned: AtomicU64::new(0),
        dirs_scanned: AtomicU64::new(0),
        total_size: AtomicU64::new(0),
    });

    let norm = path.replace('/', "\\").trim_end_matches('\\').to_string();

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
            // Phase 1: Shallow scan — immediate children only (dirs at size=0)
            let children = shallow_scan(&norm);
            let mut snapshot: Vec<DirEntry> = children.iter().map(|c| c.to_dir_entry()).collect();
            snapshot.sort_unstable_by(|a, b| b.logical_size.cmp(&a.logical_size));
            let _ = app_clone.emit(
                "scan-tree",
                ScanTreeEvent {
                    children: snapshot,
                    scanning: true,
                    current_dir: String::new(),
                },
            );

            // Collect top-level directory indices for deep scanning
            let dir_indices: Vec<(usize, String)> = children
                .iter()
                .enumerate()
                .filter(|(_, c)| c.is_dir)
                .map(|(i, c)| (i, c.path.clone()))
                .collect();

            // Phase 2: Parallel deep scan with progressive UI updates.
            // Each top-level dir completes → lock, update, emit snapshot → unlock.
            let children_shared = Arc::new(Mutex::new(children));

            {
                use rayon::prelude::*;
                dir_indices.par_iter().for_each(|(idx, dir_path)| {
                    let scanned = scan_tree(dir_path, &progress);

                    // Short lock: update slot + clone sorted snapshot, then release
                    let snapshot = {
                        let mut guard = children_shared.lock().unwrap();
                        guard[*idx] = scanned;
                        let mut s: Vec<DirEntry> =
                            guard.iter().map(|c| c.to_dir_entry()).collect();
                        s.sort_unstable_by(|a, b| b.logical_size.cmp(&a.logical_size));
                        s
                    };

                    let _ = app_clone.emit(
                        "scan-tree",
                        ScanTreeEvent {
                            children: snapshot,
                            scanning: true,
                            current_dir: String::new(),
                        },
                    );
                });
            }

            // All parallel work done — sole owner, unwrap safely
            let mut children = Arc::try_unwrap(children_shared)
                .expect("sole owner after par_iter")
                .into_inner()
                .unwrap();

            children.sort_unstable_by(|a, b| b.logical_size.cmp(&a.logical_size));

            let root_name = norm
                .rsplit_once('\\')
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| norm.clone());
            let total_size: u64 = children.iter().map(|c| c.logical_size).sum();
            let total_files: u64 = children
                .iter()
                .map(|c| if c.is_dir { c.files } else { 1 })
                .sum();
            let total_subdirs: u64 = children.iter().filter(|c| c.is_dir).count() as u64
                + children.iter().map(|c| c.subdirs).sum::<u64>();

            TreeNode {
                name: root_name,
                path: norm,
                logical_size: total_size,
                files: total_files,
                subdirs: total_subdirs,
                is_dir: true,
                children,
            }
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
    let snapshot: Vec<DirEntry> = tree.children.iter().map(|c| c.to_dir_entry()).collect();
    let _ = app.emit(
        "scan-tree",
        ScanTreeEvent {
            children: snapshot,
            scanning: false,
            current_dir: String::new(),
        },
    );

    let count = count_nodes(&tree);
    *state.0.lock().map_err(|e| e.to_string())? = Some(tree);
    Ok(count)
}

#[tauri::command]
pub fn get_children(
    parent_path: String,
    top_n: usize,
    state: State<'_, ScanState>,
) -> Result<Vec<DirEntry>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_ref().ok_or("No scan data. Run a scan first.")?;

    let norm = normalize(&parent_path);
    let node = find_node(tree, &norm).ok_or_else(|| format!("Path not found: {norm}"))?;

    let mut entries: Vec<DirEntry> = node.children.iter().map(|c| c.to_dir_entry()).collect();
    entries.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));
    entries.truncate(top_n);
    Ok(entries)
}

/// Find a node by path (case-insensitive for Windows).
/// Avoids `to_ascii_lowercase()` allocations — uses `eq_ignore_ascii_case` directly.
pub(crate) fn find_node<'a>(node: &'a TreeNode, target: &str) -> Option<&'a TreeNode> {
    if node.path.eq_ignore_ascii_case(target) {
        return Some(node);
    }
    // Only recurse into directories whose path is a true prefix of the target
    if node.is_dir
        && target.len() > node.path.len()
        && target.as_bytes().get(node.path.len()) == Some(&b'\\')
        && target[..node.path.len()].eq_ignore_ascii_case(&node.path)
    {
        for child in &node.children {
            if let Some(found) = find_node(child, target) {
                return Some(found);
            }
        }
    }
    None
}

/// Mutable variant of `find_node` for in-place updates.
pub(crate) fn find_node_mut<'a>(node: &'a mut TreeNode, target: &str) -> Option<&'a mut TreeNode> {
    if node.path.eq_ignore_ascii_case(target) {
        return Some(node);
    }
    if node.is_dir
        && target.len() > node.path.len()
        && target.as_bytes().get(node.path.len()) == Some(&b'\\')
        && target[..node.path.len()].eq_ignore_ascii_case(&node.path)
    {
        for child in &mut node.children {
            if let Some(found) = find_node_mut(child, target) {
                return Some(found);
            }
        }
    }
    None
}

fn count_nodes(node: &TreeNode) -> u64 {
    1 + node.children.iter().map(count_nodes).sum::<u64>()
}

pub(crate) fn normalize(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}
