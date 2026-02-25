use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::TreeNode;

#[cfg(windows)]
mod win32 {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    // Win32 constants
    pub const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut _;
    pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    #[repr(C)]
    pub struct WIN32_FIND_DATAW {
        pub dw_file_attributes: u32,
        pub ft_creation_time: [u32; 2],
        pub ft_last_access_time: [u32; 2],
        pub ft_last_write_time: [u32; 2],
        pub n_file_size_high: u32,
        pub n_file_size_low: u32,
        pub dw_reserved0: u32,
        pub dw_reserved1: u32,
        pub c_file_name: [u16; 260],
        pub c_alternate_file_name: [u16; 14],
    }

    extern "system" {
        pub fn FindFirstFileW(
            lp_file_name: *const u16,
            lp_find_file_data: *mut WIN32_FIND_DATAW,
        ) -> *mut std::ffi::c_void;
        pub fn FindNextFileW(
            h_find_file: *mut std::ffi::c_void,
            lp_find_file_data: *mut WIN32_FIND_DATAW,
        ) -> i32;
        pub fn FindClose(h_find_file: *mut std::ffi::c_void) -> i32;
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    pub fn from_wide(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub dirs_scanned: AtomicU64,
    pub total_size: AtomicU64,
}

/// Recursively scan using Win32 FindFirstFileW/FindNextFileW.
/// Calls `on_progress` every `interval` entries.
pub fn scan_tree(
    root: &str,
    progress: &Arc<ScanProgress>,
    on_progress: &dyn Fn(&ScanProgress),
    interval: u64,
) -> TreeNode {
    let name = root
        .rsplit_once('\\')
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| root.to_string());

    let norm = normalize(root);
    scan_dir(&norm, &name, progress, on_progress, interval)
}

#[cfg(windows)]
fn scan_dir(
    dir_path: &str,
    dir_name: &str,
    progress: &Arc<ScanProgress>,
    on_progress: &dyn Fn(&ScanProgress),
    interval: u64,
) -> TreeNode {
    use win32::*;

    let pattern = format!("{}\\*", dir_path);
    let wide = to_wide(&pattern);

    let mut find_data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileW(wide.as_ptr(), &mut find_data) };

    if handle == INVALID_HANDLE_VALUE {
        return TreeNode {
            name: dir_name.to_string(),
            path: dir_path.to_string(),
            logical_size: 0,
            files: 0,
            subdirs: 0,
            is_dir: true,
            children: Vec::new(),
        };
    }

    let mut children = Vec::new();
    let mut total_size: u64 = 0;
    let mut total_files: u64 = 0;
    let mut total_subdirs: u64 = 0;

    loop {
        let name = from_wide(&find_data.c_file_name);

        // Skip . and ..
        if name != "." && name != ".." {
            let attrs = find_data.dw_file_attributes;
            let child_path = format!("{}\\{}", dir_path, name);

            // Skip reparse points (symlinks, junctions) to avoid loops
            if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                // skip
            } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                let child = scan_dir(&child_path, &name, progress, on_progress, interval);
                total_size += child.logical_size;
                total_files += child.files;
                total_subdirs += 1 + child.subdirs;
                children.push(child);

                let count = progress.dirs_scanned.fetch_add(1, Ordering::Relaxed) + 1;
                if count % interval == 0 {
                    on_progress(progress);
                }
            } else {
                let size = ((find_data.n_file_size_high as u64) << 32)
                    | (find_data.n_file_size_low as u64);
                total_size += size;
                total_files += 1;

                children.push(TreeNode {
                    name,
                    path: child_path,
                    logical_size: size,
                    files: 0,
                    subdirs: 0,
                    is_dir: false,
                    children: Vec::new(),
                });

                // Only count file sizes in atomic progress (avoids double-counting at dir level)
                progress.total_size.fetch_add(size, Ordering::Relaxed);

                let count = progress.files_scanned.fetch_add(1, Ordering::Relaxed) + 1;
                if count % interval == 0 {
                    on_progress(progress);
                }
            }
        }

        if unsafe { FindNextFileW(handle, &mut find_data) } == 0 {
            break;
        }
    }

    unsafe { FindClose(handle) };

    // Sort children by size descending
    children.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));

    TreeNode {
        name: dir_name.to_string(),
        path: dir_path.to_string(),
        logical_size: total_size,
        files: total_files,
        subdirs: total_subdirs,
        is_dir: true,
        children,
    }
}

/// Non-recursive scan: only enumerate the immediate children of `dir_path`.
/// Files get their real size; directories get size=0 and empty children.
#[cfg(windows)]
pub fn shallow_scan(dir_path: &str) -> Vec<TreeNode> {
    use win32::*;

    let norm = normalize(dir_path);
    let pattern = format!("{}\\*", norm);
    let wide = to_wide(&pattern);

    let mut find_data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe { FindFirstFileW(wide.as_ptr(), &mut find_data) };

    if handle == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut entries = Vec::new();

    loop {
        let name = from_wide(&find_data.c_file_name);

        if name != "." && name != ".." {
            let attrs = find_data.dw_file_attributes;
            let child_path = format!("{}\\{}", norm, name);

            if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                // skip reparse points
            } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                entries.push(TreeNode {
                    name,
                    path: child_path,
                    logical_size: 0,
                    files: 0,
                    subdirs: 0,
                    is_dir: true,
                    children: Vec::new(),
                });
            } else {
                let size = ((find_data.n_file_size_high as u64) << 32)
                    | (find_data.n_file_size_low as u64);
                entries.push(TreeNode {
                    name,
                    path: child_path,
                    logical_size: size,
                    files: 0,
                    subdirs: 0,
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }

        if unsafe { FindNextFileW(handle, &mut find_data) } == 0 {
            break;
        }
    }

    unsafe { FindClose(handle) };

    entries
}

fn normalize(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}
