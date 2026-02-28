use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::TreeNode;

// ---------------------------------------------------------------------------
// Win32 API bindings
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win32 {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    pub const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut _;
    pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    // CreateFileW flags
    pub const FILE_LIST_DIRECTORY: u32 = 0x0001;
    pub const FILE_SHARE_READ: u32 = 0x1;
    pub const FILE_SHARE_WRITE: u32 = 0x2;
    pub const FILE_SHARE_DELETE: u32 = 0x4;
    pub const OPEN_EXISTING: u32 = 3;
    pub const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    // GetFileInformationByHandleEx info classes (Win 8+)
    pub const FILE_FULL_DIR_INFO_CLASS: u32 = 14;
    pub const FILE_FULL_DIR_RESTART_INFO_CLASS: u32 = 15;

    /// Matches the Windows FILE_FULL_DIR_INFO layout.
    /// `file_name` is a flexible-array member; real data extends past `[u16; 1]`.
    #[repr(C)]
    pub struct FileFullDirInfo {
        pub next_entry_offset: u32,
        pub file_index: u32,
        pub creation_time: i64,
        pub last_access_time: i64,
        pub last_write_time: i64,
        pub change_time: i64,
        pub end_of_file: i64,
        pub allocation_size: i64,
        pub file_attributes: u32,
        pub file_name_length: u32,
        pub ea_size: u32,
        pub file_name: [u16; 1],
    }

    extern "system" {
        pub fn CreateFileW(
            lp_file_name: *const u16,
            dw_desired_access: u32,
            dw_share_mode: u32,
            lp_security_attributes: *mut std::ffi::c_void,
            dw_creation_disposition: u32,
            dw_flags_and_attributes: u32,
            h_template_file: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        pub fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        pub fn GetFileInformationByHandleEx(
            h_file: *mut std::ffi::c_void,
            file_information_class: u32,
            lp_file_information: *mut std::ffi::c_void,
            dw_buffer_size: u32,
        ) -> i32;
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }
}

// ---------------------------------------------------------------------------
// Progress tracking (lockless via atomics)
// ---------------------------------------------------------------------------

pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub dirs_scanned: AtomicU64,
    pub total_size: AtomicU64,
}

// ---------------------------------------------------------------------------
// Thread-local 256 KB buffer for batch directory enumeration.
// Each rayon worker thread gets its own buffer — no contention.
// ---------------------------------------------------------------------------

const DIR_BUF_SIZE: usize = 1024 * 1024; // 1 MB — more entries per syscall

thread_local! {
    static DIR_BUF: RefCell<Vec<u8>> = RefCell::new(vec![0u8; DIR_BUF_SIZE]);
}

// ---------------------------------------------------------------------------
// Lightweight entry parsed from the OS buffer
// ---------------------------------------------------------------------------

struct RawEntry {
    name: String,
    size: u64,
    is_dir: bool,
}

// ---------------------------------------------------------------------------
// Batch directory enumeration (GetFileInformationByHandleEx)
//
// Fills a 256 KB buffer per syscall — returns hundreds/thousands of entries
// at once, drastically reducing kernel transitions vs FindFirstFile loop.
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn enumerate_dir(dir_path: &str) -> Vec<RawEntry> {
    use win32::*;

    // Drive roots like "C:" need a trailing backslash for CreateFileW
    let open_path = if dir_path.len() == 2 && dir_path.as_bytes()[1] == b':' {
        format!("{}\\", dir_path)
    } else {
        dir_path.to_string()
    };
    let wide = to_wide(&open_path);

    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut first = true;

    DIR_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        loop {
            let class = if first {
                FILE_FULL_DIR_RESTART_INFO_CLASS
            } else {
                FILE_FULL_DIR_INFO_CLASS
            };
            first = false;

            let ok = unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    class,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                )
            };
            if ok == 0 {
                break; // ERROR_NO_MORE_FILES or other — stop enumeration
            }

            // Walk the linked list of entries inside the buffer
            let mut offset = 0usize;
            loop {
                let ptr = unsafe { buf.as_ptr().add(offset) as *const FileFullDirInfo };
                let info = unsafe { &*ptr };

                let name_chars = info.file_name_length as usize / 2;
                let name_slice = unsafe {
                    std::slice::from_raw_parts(info.file_name.as_ptr(), name_chars)
                };

                let attrs = info.file_attributes;

                // Skip "." and ".." using u16 comparison (no allocation)
                let is_dot = (name_chars == 1 && name_slice[0] == b'.' as u16)
                    || (name_chars == 2
                        && name_slice[0] == b'.' as u16
                        && name_slice[1] == b'.' as u16);

                if !is_dot && (attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0) {
                    let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
                    let size = if is_dir { 0 } else { info.end_of_file as u64 };
                    entries.push(RawEntry {
                        name: String::from_utf16_lossy(name_slice),
                        size,
                        is_dir,
                    });
                }

                if info.next_entry_offset == 0 {
                    break;
                }
                offset += info.next_entry_offset as usize;
            }
        }
    });

    unsafe { CloseHandle(handle) };
    entries
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Recursively scan a directory tree using batch Win32 API + rayon parallelism.
pub fn scan_tree(root: &str, progress: &Arc<ScanProgress>) -> TreeNode {
    let norm = normalize(root);
    let name = norm
        .rsplit_once('\\')
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| norm.clone());

    scan_dir(&norm, &name, progress)
}

/// Parallel recursive directory scanner.
#[cfg(windows)]
fn scan_dir(dir_path: &str, dir_name: &str, progress: &Arc<ScanProgress>) -> TreeNode {
    let raw = enumerate_dir(dir_path);

    let mut file_entries = Vec::new();
    let mut dir_entries = Vec::new();

    for entry in raw {
        if entry.is_dir {
            dir_entries.push(entry);
        } else {
            file_entries.push(entry);
        }
    }

    // --- files ---------------------------------------------------------------
    let mut total_size: u64 = 0;
    let file_count = file_entries.len() as u64;
    let mut children: Vec<TreeNode> = Vec::with_capacity(file_entries.len() + dir_entries.len());

    for f in &file_entries {
        total_size += f.size;
        children.push(TreeNode {
            name: f.name.clone(),
            path: format!("{}\\{}", dir_path, f.name),
            logical_size: f.size,
            files: 0,
            subdirs: 0,
            is_dir: false,
            children: Vec::new(),
        });
    }

    // Batch-update atomic counters for all files in this directory at once
    progress.files_scanned.fetch_add(file_count, Ordering::Relaxed);
    progress.total_size.fetch_add(total_size, Ordering::Relaxed);

    // --- subdirectories (parallel when >= 2) ---------------------------------
    let dir_results: Vec<TreeNode> = if dir_entries.len() >= 2 {
        use rayon::prelude::*;
        dir_entries
            .par_iter()
            .map(|d| {
                let child_path = format!("{}\\{}", dir_path, d.name);
                scan_dir(&child_path, &d.name, progress)
            })
            .collect()
    } else {
        dir_entries
            .iter()
            .map(|d| {
                let child_path = format!("{}\\{}", dir_path, d.name);
                scan_dir(&child_path, &d.name, progress)
            })
            .collect()
    };

    let mut total_subdirs: u64 = 0;
    let mut deep_files: u64 = 0;
    for child in dir_results {
        total_size += child.logical_size;
        deep_files += child.files;
        total_subdirs += 1 + child.subdirs;
        children.push(child);
    }

    progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);

    // Sort children by size descending
    children.sort_unstable_by(|a, b| b.logical_size.cmp(&a.logical_size));

    TreeNode {
        name: dir_name.to_string(),
        path: dir_path.to_string(),
        logical_size: total_size,
        files: file_count + deep_files,
        subdirs: total_subdirs,
        is_dir: true,
        children,
    }
}

/// Non-recursive scan: immediate children only (Phase 1 quick preview).
#[cfg(windows)]
pub fn shallow_scan(dir_path: &str) -> Vec<TreeNode> {
    let norm = normalize(dir_path);
    let raw = enumerate_dir(&norm);

    raw.into_iter()
        .map(|e| TreeNode {
            path: format!("{}\\{}", norm, e.name),
            name: e.name,
            logical_size: e.size,
            files: 0,
            subdirs: 0,
            is_dir: e.is_dir,
            children: Vec::new(),
        })
        .collect()
}

fn normalize(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}
