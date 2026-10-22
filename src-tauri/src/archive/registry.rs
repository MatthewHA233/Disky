//! 系统登记的已安装软件。
//!
//! Windows 读注册表卸载信息（与「设置 → 应用」同源）：显示名、发布者、安装位置、图标。
//! 用来给软件起正确的名字、找到装在非常规位置的软件，以及判断一个程序目录是不是卸载后的残留。
//! 其他平台暂时没有对应来源，返回空列表，软件只靠目录识别。

#[derive(Debug, Clone, Default)]
pub struct InstalledApp {
    pub name: String,
    pub publisher: String,
    /// 安装目录（已规范化，不带尾随分隔符）
    pub location: Option<String>,
    /// 图标所在文件（exe / ico），不带图标序号
    pub icon: Option<String>,
}

#[cfg(not(windows))]
pub fn installed_apps() -> Vec<InstalledApp> {
    Vec::new()
}

#[cfg(windows)]
pub fn installed_apps() -> Vec<InstalledApp> {
    win::installed_apps()
}

/// 注册表里的路径常带引号、图标序号或启动参数：`"C:\x\a.exe",0`、`C:\x\a.exe /uninstall`
pub fn clean_file_path(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = if let Some(rest) = s.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        // 去掉 ",序号"
        let s = match s.rsplit_once(',') {
            Some((head, tail)) if tail.trim().trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => head,
            _ => s,
        };
        // 去掉 exe 之后的参数
        match s.to_ascii_lowercase().find(".exe") {
            Some(i) => &s[..i + 4],
            None => s,
        }
    };
    let s = s.trim();
    if s.is_empty() { None } else { Some(crate::scanner::normalize_path(s)) }
}

/// 展开 `%ProgramFiles%` 这类环境变量
pub fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
mod win {
    use super::{clean_file_path, expand_env, InstalledApp};
    use crate::scanner::normalize_path;
    use std::collections::HashSet;
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_SZ,
    };

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(parent: HKEY, sub: &str) -> Option<Key> {
        let mut h: HKEY = std::ptr::null_mut();
        let name = wide(sub);
        let rc = unsafe { RegOpenKeyExW(parent, name.as_ptr(), 0, KEY_READ, &mut h) };
        (rc == ERROR_SUCCESS).then_some(Key(h))
    }

    fn subkeys(key: &Key) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u16; 512];
        for i in 0.. {
            let mut len = buf.len() as u32;
            let rc = unsafe {
                RegEnumKeyExW(
                    key.0,
                    i,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            match rc {
                ERROR_SUCCESS => out.push(String::from_utf16_lossy(&buf[..len as usize])),
                ERROR_MORE_DATA => continue, // 名字超长的键跳过
                _ => break,
            }
        }
        out
    }

    fn raw_value(key: &Key, name: &str) -> Option<(u32, Vec<u8>)> {
        let wname = wide(name);
        let mut ty = 0u32;
        let mut size = 0u32;
        let rc = unsafe {
            RegQueryValueExW(key.0, wname.as_ptr(), std::ptr::null(), &mut ty, std::ptr::null_mut(), &mut size)
        };
        if rc != ERROR_SUCCESS || size == 0 || size > 64 * 1024 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let rc = unsafe {
            RegQueryValueExW(key.0, wname.as_ptr(), std::ptr::null(), &mut ty, data.as_mut_ptr(), &mut size)
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        data.truncate(size as usize);
        Some((ty, data))
    }

    fn string(key: &Key, name: &str) -> Option<String> {
        let (ty, data) = raw_value(key, name)?;
        if ty != REG_SZ && ty != REG_EXPAND_SZ {
            return None;
        }
        let units: Vec<u16> = data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let s = String::from_utf16_lossy(&units[..end]);
        let s = if ty == REG_EXPAND_SZ { expand_env(&s) } else { s };
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn dword(key: &Key, name: &str) -> Option<u32> {
        let (ty, data) = raw_value(key, name)?;
        (ty == REG_DWORD && data.len() >= 4).then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    fn read_entry(key: &Key) -> Option<InstalledApp> {
        let name = string(key, "DisplayName")?;
        if dword(key, "SystemComponent") == Some(1) || string(key, "ParentKeyName").is_some() {
            return None;
        }
        if let Some(t) = string(key, "ReleaseType") {
            if t.contains("Update") || t.contains("Hotfix") {
                return None;
            }
        }
        let location = string(key, "InstallLocation")
            .map(|s| normalize_path(s.trim_matches('"')))
            .filter(|s| s.len() > 3);
        let icon = string(key, "DisplayIcon")
            .and_then(|s| clean_file_path(&s))
            .filter(|p| {
                let l = p.to_ascii_lowercase();
                (l.ends_with(".exe") || l.ends_with(".ico")) && !l.contains("\\windows\\installer\\")
            });
        Some(InstalledApp {
            name,
            publisher: string(key, "Publisher").unwrap_or_default(),
            location,
            icon,
        })
    }

    pub fn installed_apps() -> Vec<InstalledApp> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (hive, path) in [
            (HKEY_LOCAL_MACHINE, UNINSTALL),
            (HKEY_LOCAL_MACHINE, UNINSTALL_32),
            (HKEY_CURRENT_USER, UNINSTALL),
        ] {
            let Some(root) = open(hive, path) else { continue };
            for sub in subkeys(&root) {
                let Some(key) = open(root.0, &sub) else { continue };
                let Some(app) = read_entry(&key) else { continue };
                let id = format!(
                    "{}|{}",
                    app.name.to_lowercase(),
                    app.location.as_deref().unwrap_or("").to_lowercase()
                );
                if seen.insert(id) {
                    out.push(app);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_registry_paths() {
        assert_eq!(clean_file_path("\"C:\\A B\\a.exe\",0").as_deref(), Some("C:\\A B\\a.exe"));
        assert_eq!(clean_file_path("C:\\x\\a.exe,-101").as_deref(), Some("C:\\x\\a.exe"));
        assert_eq!(clean_file_path("C:\\x\\uninst.exe /S").as_deref(), Some("C:\\x\\uninst.exe"));
        assert_eq!(clean_file_path("  ").as_deref(), None);
    }

    #[test]
    fn expands_known_env_and_keeps_unknown() {
        std::env::set_var("DISKY_TEST_DIR", "X");
        assert_eq!(expand_env("%DISKY_TEST_DIR%\\a"), "X\\a");
        assert_eq!(expand_env("%NOPE_DISKY%\\a"), "%NOPE_DISKY%\\a");
        assert_eq!(expand_env("100%"), "100%");
    }
}
