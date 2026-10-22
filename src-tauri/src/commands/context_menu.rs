use serde::Serialize;
use std::fs;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use tauri::State;

use super::cleanup::{is_protected, dir_size};
use super::scan::{normalize, ScanState};
use crate::scanner::walk::{scan_tree, ScanProgress};
use crate::scanner::DirEntry;

/// AppleScript 字符串字面量转义（双引号与反斜杠）
#[cfg(target_os = "macos")]
fn osa_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

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

#[cfg(windows)]
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

#[cfg(not(windows))]
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Show in Explorer / Finder (select the item)
// ---------------------------------------------------------------------------

/// explorer 的 /select 参数。只能给路径加引号：交给 .arg() 时带空格的路径会被整段加上引号
/// （"/select,C:\Program Files"），explorer 认不出就退回去打开「文档」。盘根要带上 \
#[cfg(windows)]
fn explorer_select_arg(path: &str) -> Result<String, String> {
    // Windows 路径里不会有引号；有的话拼进去会多出参数
    if path.contains('"') {
        return Err("路径不对".into());
    }
    let mut norm = path.replace('/', "\\");
    if norm.len() == 2 && norm.ends_with(':') {
        norm.push('\\');
    }
    Ok(format!("/select,\"{norm}\""))
}

#[cfg(windows)]
#[tauri::command]
pub fn show_in_explorer(path: String) -> Result<(), String> {
    Command::new("explorer.exe")
        .raw_arg(explorer_select_arg(&path)?)
        .spawn()
        .map_err(|e| format!("Failed to open Explorer: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn show_in_explorer(path: String) -> Result<(), String> {
    // open -R：在 Finder 中显示并选中该项
    Command::new("open")
        .args(["-R", &path])
        .spawn()
        .map_err(|e| format!("Failed to reveal in Finder: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Open in terminal (Windows: CMD/PowerShell; macOS: Terminal.app)
// ---------------------------------------------------------------------------

/// 把路径归一化后取所在目录（文件 → 其父目录）
fn terminal_dir(norm: String) -> String {
    if Path::new(&norm).is_dir() {
        norm
    } else {
        Path::new(&norm)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(norm)
    }
}

#[cfg(not(windows))]
#[tauri::command]
pub fn open_in_terminal(path: String, shell: String) -> Result<(), String> {
    let _ = shell; // mac 上忽略 shell 参数，统一打开 Terminal.app
    let dir = terminal_dir(path);
    Command::new("open")
        .args(["-a", "Terminal", &dir])
        .spawn()
        .map_err(|e| format!("Failed to open Terminal: {e}"))?;
    Ok(())
}

#[cfg(windows)]
#[tauri::command]
pub fn open_in_terminal(path: String, shell: String) -> Result<(), String> {
    let dir = terminal_dir(path.replace('/', "\\"));

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
// 4. Show properties (Windows: 属性对话框; macOS: Finder 显示简介)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn show_properties(path: String) -> Result<(), String> {
    let script = format!(
        r#"tell application "Finder"
    open information window of (POSIX file "{}" as alias)
    activate
end tell"#,
        osa_escape(&path)
    );
    Command::new("osascript")
        .args(["-e", &script])
        .spawn()
        .map_err(|e| format!("Failed to show info: {e}"))?;
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
#[tauri::command]
pub fn show_properties(_path: String) -> Result<(), String> {
    Err("Not supported on this platform".into())
}

#[cfg(windows)]
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
    let norm = normalize(path);
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
            entry
                .metadata()
                .map(|m| crate::scanner::file_disk_size(&m))
                .unwrap_or(0)
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

/// 打开系统文件夹选择框，返回所选目录；取消时返回空串。
#[cfg(windows)]
fn pick_folder() -> Result<String, String> {
    let picker_script = r#"Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = '选择目标文件夹'; if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath } else { '' }"#;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", picker_script])
        .output()
        .map_err(|e| format!("Failed to open folder picker: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(not(windows))]
fn pick_folder() -> Result<String, String> {
    // osascript choose folder：用户取消时退出码非 0（"User canceled"），按取消处理
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"POSIX path of (choose folder with prompt "选择目标文件夹")"#,
        ])
        .output()
        .map_err(|e| format!("Failed to open folder picker: {e}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn pick_folder_and_move_blocking(path: &str) -> Result<MoveResult, String> {
    let norm = normalize(path);
    if is_protected(&norm) {
        return Err("Protected system path".into());
    }

    let dest_dir = pick_folder()?;
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
            // Cross-volume move fallback
            let move_output = cross_volume_move(&norm, &dest)?;
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

#[cfg(windows)]
fn cross_volume_move(src: &str, dest: &Path) -> Result<std::process::Output, String> {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Move-Item -LiteralPath '{}' -Destination '{}' -Force",
                src.replace('\'', "''"),
                dest.to_string_lossy().replace('\'', "''")
            ),
        ])
        .output()
        .map_err(|e| format!("Move failed: {e}"))
}

#[cfg(not(windows))]
fn cross_volume_move(src: &str, dest: &Path) -> Result<std::process::Output, String> {
    // /bin/mv 自带跨卷 copy+unlink 回退
    Command::new("mv")
        .arg(src)
        .arg(dest)
        .output()
        .map_err(|e| format!("Move failed: {e}"))
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
        let id = tree
            .find(&norm)
            .ok_or_else(|| format!("Path not found: {norm}"))?;
        if !tree.node(id).is_dir() {
            return Err("Cannot refresh a file".into());
        }
    }

    // Scan outside the lock (potentially long-running)
    let norm_clone = norm.clone();
    let scanned = tokio::task::spawn_blocking(move || {
        scan_tree(&norm_clone, &ScanProgress::default())
    })
    .await
    .map_err(|e| format!("Scan thread error: {e}"))?;

    // 嫁接新子树，并把差量同步传播给所有祖先（否则上层大小停留在旧值）
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_mut().ok_or("No scan data")?;
    let id = tree.find(&norm).ok_or("Path not found after rescan")?;
    // 扫描期间该路径可能已被同名文件替换（例如祖先刚刷新过）
    if !tree.replace_children(id, scanned) {
        return Err("Cannot refresh a file".into());
    }

    // 替换可能触发整理、下标失效，按路径重新定位后返回最新子项
    let id = tree.find(&norm).ok_or("Path not found")?;
    Ok(tree.child_entries(id, 0, usize::MAX).1)
}

// ---------------------------------------------------------------------------
// 8. Surgically remove a node from the scan tree (after delete/move)
// ---------------------------------------------------------------------------

/// 删除/移走某项后，从内存树中摘除该节点并回减祖先大小 —— 不重扫。
/// 此前删除根级条目会触发整盘重扫（分钟级），界面看起来就是"删了没反应"。
#[tauri::command]
pub fn remove_scan_node(path: String, state: State<'_, ScanState>) -> Result<(), String> {
    let norm = normalize(&path);
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_mut().ok_or("No scan data")?;
    let id = tree
        .find(&norm)
        .ok_or_else(|| format!("Node not found: {norm}"))?;
    if !tree.remove(id) {
        return Err("Cannot remove root".into());
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::explorer_select_arg;

    #[test]
    fn explorer_select_quotes_only_the_path() {
        assert_eq!(explorer_select_arg(r"C:\Program Files (x86)\Steam").as_deref(), Ok(r#"/select,"C:\Program Files (x86)\Steam""#));
        assert_eq!(explorer_select_arg("D:/my files/game").as_deref(), Ok(r#"/select,"D:\my files\game""#));
        assert_eq!(explorer_select_arg("C:").as_deref(), Ok(r#"/select,"C:\""#));
        assert!(explorer_select_arg(r#"C:\a" /x"#).is_err(), "带引号会多出参数");
    }
}
