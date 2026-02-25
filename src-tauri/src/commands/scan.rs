use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use sysinfo::Disks;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::{DirEntry, TreeNode};
use crate::scanner::walk::{scan_tree, shallow_scan, ScanProgress};

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
    let tree = tokio::task::spawn_blocking(move || {
        let progress = Arc::new(ScanProgress {
            files_scanned: AtomicU64::new(0),
            dirs_scanned: AtomicU64::new(0),
            total_size: AtomicU64::new(0),
        });

        let norm = path.replace('/', "\\").trim_end_matches('\\').to_string();

        // Phase 1: Shallow scan — enumerate first-level children only
        let mut children = shallow_scan(&norm);

        // Emit initial snapshot (dirs have size=0)
        let snapshot: Vec<DirEntry> = children.iter().map(|c| c.to_dir_entry()).collect();
        let _ = app.emit("scan-tree", ScanTreeEvent {
            children: snapshot,
            scanning: true,
            current_dir: String::new(),
        });

        // Collect directory paths and their indices for deep scanning
        let dir_indices: Vec<(usize, String)> = children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_dir)
            .map(|(i, c)| (i, c.path.clone()))
            .collect();

        // Phase 2: Deep-scan each first-level directory one by one
        for (idx, dir_path) in &dir_indices {
            let dir_name = dir_path
                .rsplit_once('\\')
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| dir_path.clone());

            // Emit "currently scanning" event
            let snapshot: Vec<DirEntry> = children.iter().map(|c| c.to_dir_entry()).collect();
            let _ = app.emit("scan-tree", ScanTreeEvent {
                children: snapshot,
                scanning: true,
                current_dir: dir_name,
            });

            // Deep-scan this directory using existing scan_tree
            let scanned = scan_tree(
                dir_path,
                &progress,
                &|p| {
                    let _ = app.emit(
                        "scan-progress",
                        ScanProgressEvent {
                            files_scanned: p.files_scanned.load(Ordering::Relaxed),
                            dirs_scanned: p.dirs_scanned.load(Ordering::Relaxed),
                            total_size: p.total_size.load(Ordering::Relaxed),
                        },
                    );
                },
                500,
            );

            // Replace the shallow placeholder with the fully-scanned subtree
            children[*idx] = scanned;

            // Emit updated snapshot
            let snapshot: Vec<DirEntry> = children.iter().map(|c| c.to_dir_entry()).collect();
            let _ = app.emit("scan-tree", ScanTreeEvent {
                children: snapshot,
                scanning: true,
                current_dir: String::new(),
            });
        }

        // Final snapshot: scanning done
        let snapshot: Vec<DirEntry> = children.iter().map(|c| c.to_dir_entry()).collect();
        let _ = app.emit("scan-tree", ScanTreeEvent {
            children: snapshot,
            scanning: false,
            current_dir: String::new(),
        });

        // Build the root TreeNode
        let root_name = norm
            .rsplit_once('\\')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| norm.clone());

        let total_size: u64 = children.iter().map(|c| c.logical_size).sum();
        let total_files: u64 = children.iter().map(|c| {
            if c.is_dir { c.files } else { 1 }
        }).sum();
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
    .map_err(|e| format!("scan thread error: {e}"))?;

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
fn find_node<'a>(node: &'a TreeNode, target: &str) -> Option<&'a TreeNode> {
    if node.path.eq_ignore_ascii_case(target) {
        return Some(node);
    }
    // Only recurse into directories whose path is a prefix of the target
    if node.is_dir && target.to_ascii_lowercase().starts_with(&node.path.to_ascii_lowercase()) {
        for child in &node.children {
            if let Some(found) = find_node(child, target) {
                return Some(found);
            }
        }
    }
    None
}

fn count_nodes(node: &TreeNode) -> u64 {
    1 + node.children.iter().map(count_nodes).sum::<u64>()
}

fn normalize(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}
