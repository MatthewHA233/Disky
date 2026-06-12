//! macOS 完全磁盘访问权限（Full Disk Access）检测与设置页跳转。
//! Windows 上两个命令都是直通的 no-op。

/// 探测是否已授予完全磁盘访问权限。
/// TCC.db 受 TCC 自身保护，只有拿到 FDA 的进程才能打开 —— 是最可靠的探针
/// （文件始终存在，不依赖用户装没装 Mail/Safari）。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    let mut probes = vec!["/Library/Application Support/com.apple.TCC/TCC.db".to_string()];
    if let Ok(home) = std::env::var("HOME") {
        probes.push(format!("{home}/Library/Application Support/com.apple.TCC/TCC.db"));
    }
    probes.iter().any(|p| std::fs::File::open(p).is_ok())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    true
}

/// 打开「系统设置 → 隐私与安全性 → 完全磁盘访问权限」面板。
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn open_full_disk_access_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .spawn()
        .map_err(|e| format!("Failed to open settings: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn open_full_disk_access_settings() -> Result<(), String> {
    Ok(())
}
