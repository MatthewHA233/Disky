use serde::Serialize;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tauri::State;

use super::cleanup::{is_protected, dir_size};
use super::scan::{adjust_ancestors, find_node, find_node_mut, normalize, ScanState};
use crate::scanner::walk::{scan_tree, ScanProgress};
use crate::scanner::DirEntry;

#[derive(Serialize)]
pub struct EmptyFolderResult {
    deleted: u64,
    bytes_freed: u64,
    errors: Vec<String>,
}

#[derive(Serialize)]
pub struct MoveResult {
    new_path: String,
}

// ---------------------------------------------------------------------------
// 1. Open with default program
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let norm = path.replace('/', "\\");
    Command::new("cmd")
        .raw_arg(format!("/c start \"\" \"{}\"", norm))
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("Failed to open: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Show in Explorer (select the item)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn show_in_explorer(path: String) -> Result<(), String> {
    let norm = path.replace('/', "\\");
    Command::new("explorer.exe")
        .arg(format!("/select,{norm}"))
        .spawn()
        .map_err(|e| format!("Failed to open Explorer: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Open in CMD or PowerShell
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_in_terminal(path: String, shell: String) -> Result<(), String> {
    let norm = path.replace('/', "\\");
    let dir = if Path::new(&norm).is_dir() {
        norm
    } else {
        Path::new(&norm)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| norm)
    };

    match shell.as_str() {
        "cmd" => {
            Command::new("cmd")
                .args(["/c", "start", "cmd", "/k", &format!("cd /d \"{}\"", dir)])
                .spawn()
                .map_err(|e| format!("Failed to open CMD: {e}"))?;
        }
        "powershell" => {
            Command::new("cmd")
                .args([
                    "/c",
                    "start",
                    "powershell",
                    "-NoExit",
                    "-Command",
                    &format!("Set-Location '{}'", dir),
                ])
                .spawn()
                .map_err(|e| format!("Failed to open PowerShell: {e}"))?;
        }
        _ => return Err(format!("Unknown shell: {shell}")),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Show Windows Properties dialog
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn show_properties(path: String) -> Result<(), String> {
    let norm = path.replace('/', "\\");
    let p = Path::new(&norm);
    let parent = p
        .parent()
        .map(|pp| pp.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let script = format!(
        "$f=(New-Object -ComObject Shell.Application).NameSpace('{}').ParseName('{}'); if($f){{$f.InvokeVerb('properties')}}; Start-Sleep -Milliseconds 800",
        parent.replace('\'', "''"),
        name.replace('\'', "''")
    );
    Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .spawn()
        .map_err(|e| format!("Failed to show properties: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Empty folder (delete contents, keep folder)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn empty_folder(path: String, to_trash: bool) -> Result<EmptyFolderResult, String> {
    // 重 IO（递归算大小 + 回收站 COM），必须离开主线程，否则整个 UI 冻结
    tokio::task::spawn_blocking(move || empty_folder_blocking(&path, to_trash))
        .await
        .map_err(|e| e.to_string())?
}

fn empty_folder_blocking(path: &str, to_trash: bool) -> Result<EmptyFolderResult, String> {
    let norm = path.replace('/', "\\");
    let p = Path::new(&norm);

    if !p.is_dir() {
        return Err("Not a directory".into());
    }
    if is_protected(&norm) {
        return Err("Protected system path".into());
    }

    let mut deleted: u64 = 0;
    let mut bytes_freed: u64 = 0;
    let mut errors = Vec::new();

    let entries = fs::read_dir(p).map_err(|e| format!("Cannot read directory: {e}"))?;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let entry_path = entry.path();
        let entry_size = if entry_path.is_dir() {
            dir_size(&entry_path)
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };
        let result = if to_trash {
            trash::delete(&entry_path).map_err(|e| e.to_string())
        } else if entry_path.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|e| e.to_string())
        } else {
            fs::remove_file(&entry_path).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                deleted += 1;
                bytes_freed += entry_size;
            }
            Err(e) => errors.push(format!("{}: {}", entry_path.display(), e)),
        }
    }

    Ok(EmptyFolderResult { deleted, bytes_freed, errors })
}

// ---------------------------------------------------------------------------
// 6. Pick folder via dialog and move item
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn pick_folder_and_move(path: String) -> Result<MoveResult, String> {
    // PowerShell 选择框 + 跨盘移动都是阻塞操作，放主线程会把整个窗口挂住
    tokio::task::spawn_blocking(move || pick_folder_and_move_blocking(&path))
        .await
        .map_err(|e| e.to_string())?
}

fn pick_folder_and_move_blocking(path: &str) -> Result<MoveResult, String> {
    let norm = path.replace('/', "\\");
    if is_protected(&norm) {
        return Err("Protected system path".into());
    }

    // Open folder picker via PowerShell FolderBrowserDialog
    let picker_script = r#"Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = '选择目标文件夹'; if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath } else { '' }"#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", picker_script])
        .output()
        .map_err(|e| format!("Failed to open folder picker: {e}"))?;

    let dest_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if dest_dir.is_empty() {
        return Err("cancelled".into());
    }

    let src = Path::new(&norm);
    let file_name = src
        .file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy();
    let dest = Path::new(&dest_dir).join(file_name.as_ref());

    if dest.exists() {
        return Err(format!("Target already exists: {}", dest.display()));
    }

    // Try fs::rename first (fast, same-drive)
    match fs::rename(&norm, &dest) {
        Ok(()) => Ok(MoveResult {
            new_path: dest.to_string_lossy().to_string(),
        }),
        Err(_) => {
            // Fall back to PowerShell Move-Item for cross-drive
            let move_output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Move-Item -LiteralPath '{}' -Destination '{}' -Force",
                        norm.replace('\'', "''"),
                        dest.to_string_lossy().replace('\'', "''")
                    ),
                ])
                .output()
                .map_err(|e| format!("Move failed: {e}"))?;

            if move_output.status.success() {
                Ok(MoveResult {
                    new_path: dest.to_string_lossy().to_string(),
                })
            } else {
                let stderr = String::from_utf8_lossy(&move_output.stderr);
                Err(format!("Move failed: {}", stderr.trim()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 7. Refresh a single directory node in the scan tree
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn refresh_scan_node(
    path: String,
    state: State<'_, ScanState>,
) -> Result<Vec<DirEntry>, String> {
    let norm = normalize(&path);

    // Verify the path is a directory in the current tree
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        let tree = guard.as_ref().ok_or("No scan data. Run a scan first.")?;
        let node = find_node(tree, &norm).ok_or_else(|| format!("Path not found: {norm}"))?;
        if !node.is_dir {
            return Err("Cannot refresh a file".into());
        }
    }

    // Scan outside the lock (potentially long-running)
    let norm_clone = norm.clone();
    let scanned = tokio::task::spawn_blocking(move || {
        let progress = Arc::new(ScanProgress {
            files_scanned: AtomicU64::new(0),
            dirs_scanned: AtomicU64::new(0),
            total_size: AtomicU64::new(0),
        });
        scan_tree(&norm_clone, &progress)
    })
    .await
    .map_err(|e| format!("Scan thread error: {e}"))?;

    // Update the tree in-place
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_mut().ok_or("No scan data")?;
    let node = find_node_mut(tree, &norm).ok_or("Path not found after rescan")?;

    // 记录差量，刷新后同步传播给所有祖先目录（否则上层大小停留在旧值）
    let delta_size = scanned.logical_size as i64 - node.logical_size as i64;
    let delta_files = scanned.files as i64 - node.files as i64;
    let delta_dirs = scanned.subdirs as i64 - node.subdirs as i64;

    node.children = scanned.children;
    node.logical_size = scanned.logical_size;
    node.files = scanned.files;
    node.subdirs = scanned.subdirs;

    adjust_ancestors(tree, &norm, delta_size, delta_files, delta_dirs);

    // Return updated children
    let tree_ref = guard.as_ref().ok_or("No scan data")?;
    let node = find_node(tree_ref, &norm).ok_or("Path not found")?;
    let mut entries: Vec<DirEntry> = node.children.iter().map(|c| c.to_dir_entry()).collect();
    entries.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));
    Ok(entries)
}

// ---------------------------------------------------------------------------
// 8. Surgically remove a node from the scan tree (after delete/move)
// ---------------------------------------------------------------------------

/// 删除/移走某项后，从内存树中摘除该节点并回减祖先大小 —— 不重扫。
/// 此前删除根级条目会触发整盘重扫（分钟级），界面看起来就是"删了没反应"。
#[tauri::command]
pub fn remove_scan_node(path: String, state: State<'_, ScanState>) -> Result<(), String> {
    let norm = normalize(&path);
    let parent_path = match norm.rfind('\\') {
        Some(i) => normalize(&norm[..i]),
        None => return Err("Cannot remove root".into()),
    };

    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_mut().ok_or("No scan data")?;
    let parent = find_node_mut(tree, &parent_path)
        .ok_or_else(|| format!("Parent not found: {parent_path}"))?;
    let idx = parent
        .children
        .iter()
        .position(|c| normalize(&c.path).eq_ignore_ascii_case(&norm))
        .ok_or_else(|| format!("Node not found: {norm}"))?;
    let removed = parent.children.remove(idx);

    let (delta_files, delta_dirs) = if removed.is_dir {
        (-(removed.files as i64), -(removed.subdirs as i64) - 1)
    } else {
        (-1, 0)
    };
    let delta_size = -(removed.logical_size as i64);

    // 父节点也是 norm 的真祖先，adjust_ancestors 会一并处理
    adjust_ancestors(tree, &norm, delta_size, delta_files, delta_dirs);
    Ok(())
}
