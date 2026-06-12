pub mod walk;

use serde::Serialize;

// ---------------------------------------------------------------------------
// 跨平台路径工具：Windows 用 "\"，macOS/Linux 用 "/"。
// 整个扫描树/前端通信都使用平台原生分隔符的字符串路径。
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub const SEP: char = '\\';
#[cfg(not(windows))]
pub const SEP: char = '/';

pub const SEP_BYTE: u8 = SEP as u8;

/// 统一分隔符并去掉尾随分隔符；Unix 根目录 "/" 保留原样（去掉会变成空串）。
#[cfg(windows)]
pub fn normalize_path(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}

#[cfg(not(windows))]
pub fn normalize_path(p: &str) -> String {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        "/".to_string()
    } else {
        t.to_string()
    }
}

/// 拼接目录与子项名；处理 Unix 根目录避免产生 "//name"。
pub fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with(SEP) {
        format!("{dir}{name}")
    } else {
        format!("{dir}{SEP}{name}")
    }
}

/// `ancestor` 是否为 `target` 的真祖先（路径前缀 + 分隔符边界）。
/// 大小写不敏感（NTFS 与默认 APFS 都不区分大小写）。
/// 处理 Unix 根 "/"（自身以分隔符结尾，无需再查边界字节）。
pub fn is_strict_ancestor(ancestor: &str, target: &str) -> bool {
    if target.len() <= ancestor.len() {
        return false;
    }
    let Some(prefix) = target.get(..ancestor.len()) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case(ancestor) {
        return false;
    }
    if ancestor.ends_with(SEP) {
        return true;
    }
    target.as_bytes().get(ancestor.len()) == Some(&SEP_BYTE)
}

/// 取路径最后一段作为显示名；根路径（"/"、"C:"）返回其本身。
pub fn path_file_name(p: &str) -> String {
    match p.rsplit_once(SEP) {
        Some((_, n)) if !n.is_empty() => n.to_string(),
        _ => p.to_string(),
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn normalize_keeps_unix_root() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("//"), "/");
        assert_eq!(normalize_path("/Users/"), "/Users");
        assert_eq!(normalize_path("/Users/me"), "/Users/me");
    }

    #[test]
    fn join_handles_root() {
        assert_eq!(join_path("/", "Users"), "/Users");
        assert_eq!(join_path("/Users", "me"), "/Users/me");
    }

    #[test]
    fn ancestor_handles_root_and_boundaries() {
        assert!(is_strict_ancestor("/", "/Users"));
        assert!(is_strict_ancestor("/Users", "/Users/me"));
        assert!(is_strict_ancestor("/users", "/Users/me")); // 大小写不敏感
        assert!(!is_strict_ancestor("/Users", "/Users")); // 自身不算
        assert!(!is_strict_ancestor("/Users", "/UsersBackup/x")); // 分隔符边界
    }

    #[test]
    fn file_name_of_root_is_root() {
        assert_eq!(path_file_name("/"), "/");
        assert_eq!(path_file_name("/Users/me"), "me");
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub logical_size: u64,
    pub files: u64,
    pub subdirs: u64,
    pub is_dir: bool,
    #[serde(skip)]
    pub children: Vec<TreeNode>,
}

/// Serializable child entry returned to the frontend (matches old DirEntry shape).
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub files: u64,
    pub subdirs: u64,
    pub logical_size: u64,
    pub is_dir: bool,
}

impl TreeNode {
    pub fn to_dir_entry(&self) -> DirEntry {
        DirEntry {
            name: self.name.clone(),
            path: self.path.clone(),
            files: self.files,
            subdirs: self.subdirs,
            logical_size: self.logical_size,
            is_dir: self.is_dir,
        }
    }
}
