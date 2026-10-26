#[cfg(windows)]
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rayon::prelude::*;

use super::{join_path, normalize_path, DirEntry, NewChild, NodeId, ScanTree, ROOT};

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

#[derive(Default)]
pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub dirs_scanned: AtomicU64,
    pub total_size: AtomicU64,
}

// ---------------------------------------------------------------------------
// Thread-local 256 KB buffer for batch directory enumeration.
// Each rayon worker thread gets its own buffer — no contention.
// ---------------------------------------------------------------------------

#[cfg(windows)]
const DIR_BUF_SIZE: usize = 1024 * 1024; // 1 MB — more entries per syscall

#[cfg(windows)]
thread_local! {
    static DIR_BUF: RefCell<Vec<u8>> = RefCell::new(vec![0u8; DIR_BUF_SIZE]);
}

// ---------------------------------------------------------------------------
// Lightweight entry parsed from the OS buffer
// ---------------------------------------------------------------------------

pub(crate) struct RawEntry {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
}

// ---------------------------------------------------------------------------
// Batch directory enumeration (GetFileInformationByHandleEx)
//
// Fills a 256 KB buffer per syscall — returns hundreds/thousands of entries
// at once, drastically reducing kernel transitions vs FindFirstFile loop.
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub(crate) fn enumerate_dir(dir_path: &str) -> Vec<RawEntry> {
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

/// Unix 实现：readdir + lstat。跳过符号链接（对应 Windows 跳过 reparse point），
/// 避免循环引用与重复计数。
#[cfg(unix)]
pub(crate) fn enumerate_dir(dir_path: &str) -> Vec<RawEntry> {
    let rd = match std::fs::read_dir(dir_path) {
        Ok(r) => r,
        Err(_) => return Vec::new(), // 无权限/已删除 — 与 Windows 行为一致，静默跳过
    };
    rd.filter_map(|entry| {
        let entry = entry.ok()?;
        let ft = entry.file_type().ok()?;
        if ft.is_symlink() {
            return None;
        }
        let is_dir = ft.is_dir();
        let size = if is_dir {
            0
        } else {
            entry
                .metadata()
                .map(|m| super::file_disk_size(&m))
                .unwrap_or(0)
        };
        Some(RawEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            size,
            is_dir,
        })
    })
    .collect()
}

/// macOS：扫描时跳过的特殊目录 —— 跳过「重复计数」「大小失真」「不属于本卷」的：
/// - /System/Volumes/Data：数据卷固件链接挂载，内容与 "/" 下的 /Users 等重复
/// - /System/Volumes/Update/mnt1（及 SFR/mnt1）：密封系统快照的二次挂载，与 "/" 重复
/// - /System/Volumes/Preboot：引导卷内是 APFS 克隆（实测文件级 43G / 卷实占 17G，
///   膨胀 2.5 倍），克隆无法经 stat 识别，且不可清理 —— 整卷跳过
/// - Hardware/iSCPreboot/xarts：Apple Silicon 内部容器（disk1）的小系统卷，不属于启动盘
/// - /Volumes：其他磁盘的挂载点（应单独扫描）；/dev：设备文件系统
/// VM（swap/休眠）、Update 本体大小真实，照常纳入统计。
#[cfg(target_os = "macos")]
pub(crate) fn should_skip_dir(path: &str) -> bool {
    matches!(
        path,
        "/System/Volumes/Data"
            | "/System/Volumes/Update/mnt1"
            | "/System/Volumes/Update/SFR/mnt1"
            | "/System/Volumes/Preboot"
            | "/System/Volumes/Hardware"
            | "/System/Volumes/iSCPreboot"
            | "/System/Volumes/xarts"
            | "/Volumes"
            | "/dev"
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn should_skip_dir(path: &str) -> bool {
    matches!(path, "/proc" | "/sys" | "/dev" | "/run")
}

#[cfg(windows)]
pub(crate) fn should_skip_dir(_path: &str) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// 递归扫描一个目录，返回以它为根的独立扫描树（刷新节点、监听器增量刷新用）。
pub fn scan_tree(root: &str, progress: &ScanProgress) -> ScanTree {
    let norm = normalize_path(root);
    let tree = Mutex::new(ScanTree::new(&norm));
    scan_into(&tree, ROOT, &norm, progress);
    into_tree(tree)
}

/// 整盘扫描：先列出根目录的直接子项并立即回调一次快照（子目录大小为 0），
/// 再并行深扫每个顶层目录，每完成一个回调一次最新快照，供前端渐进显示。
pub fn scan_tree_progressive<F>(root: &str, progress: &ScanProgress, on_snapshot: F) -> ScanTree
where
    F: Fn(Vec<DirEntry>) + Sync,
{
    let norm = normalize_path(root);
    let tree = Mutex::new(ScanTree::new(&norm));

    let (files, subdirs) = expand_dir(&tree, ROOT, &norm, progress);
    on_snapshot(top_level_snapshot(&tree));

    let sub_totals: Vec<Totals> = subdirs
        .par_iter()
        .map(|(id, path)| {
            let totals = scan_into(&tree, *id, path, progress);
            on_snapshot(top_level_snapshot(&tree));
            totals
        })
        .collect();

    finish_dir(&tree, ROOT, files, &sub_totals, progress);
    into_tree(tree)
}

// ---------------------------------------------------------------------------
// Parallel recursive scan into a shared arena
//
// 每个目录只加两次锁：列完子项后一次性追加（子项在 arena 中连续），
// 子目录全部扫完后回填自身聚合值。临界区都是微秒级，rayon 并行几乎不受影响。
// ---------------------------------------------------------------------------

/// 一个目录扫描完成后的聚合值。
#[derive(Clone, Copy, Default)]
struct Totals {
    size: u64,
    files: u64,
    subdirs: u64,
}

fn lock(tree: &Mutex<ScanTree>) -> MutexGuard<'_, ScanTree> {
    tree.lock().unwrap_or_else(PoisonError::into_inner)
}

fn into_tree(tree: Mutex<ScanTree>) -> ScanTree {
    let mut tree = tree.into_inner().unwrap_or_else(PoisonError::into_inner);
    tree.shrink_to_fit();
    tree
}

fn top_level_snapshot(tree: &Mutex<ScanTree>) -> Vec<DirEntry> {
    lock(tree).child_entries(ROOT, 0, usize::MAX).1
}

/// 深扫 `id` 对应的目录：写入全部后代并回填该目录的聚合值。
fn scan_into(tree: &Mutex<ScanTree>, id: NodeId, dir_path: &str, progress: &ScanProgress) -> Totals {
    let (files, subdirs) = expand_dir(tree, id, dir_path, progress);

    let sub_totals: Vec<Totals> = if subdirs.len() >= 2 {
        subdirs
            .par_iter()
            .map(|(cid, path)| scan_into(tree, *cid, path, progress))
            .collect()
    } else {
        subdirs
            .iter()
            .map(|(cid, path)| scan_into(tree, *cid, path, progress))
            .collect()
    };

    finish_dir(tree, id, files, &sub_totals, progress)
}

/// 列出目录的直接子项并连续写入树（子目录大小暂为 0），
/// 返回文件部分的合计与待深扫的子目录 (下标, 路径)。
fn expand_dir(
    tree: &Mutex<ScanTree>,
    id: NodeId,
    dir_path: &str,
    progress: &ScanProgress,
) -> (Totals, Vec<(NodeId, String)>) {
    let raw: Vec<RawEntry> = enumerate_dir(dir_path)
        .into_iter()
        .filter(|e| !(e.is_dir && should_skip_dir(&join_path(dir_path, &e.name))))
        .collect();

    let files = raw.iter().filter(|e| !e.is_dir).fold(Totals::default(), |acc, e| Totals {
        size: acc.size + e.size,
        files: acc.files + 1,
        subdirs: 0,
    });
    // Batch-update atomic counters for all files in this directory at once
    progress.files_scanned.fetch_add(files.files, Ordering::Relaxed);
    progress.total_size.fetch_add(files.size, Ordering::Relaxed);

    let first = lock(tree).push_children(
        id,
        raw.iter().map(|e| NewChild {
            name: &e.name,
            size: e.size,
            is_dir: e.is_dir,
        }),
    );

    let subdirs = raw
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_dir)
        .map(|(i, e)| (first + i as NodeId, join_path(dir_path, &e.name)))
        .collect();
    (files, subdirs)
}

/// 汇总文件合计与各子目录结果，回填到 `id`。
fn finish_dir(
    tree: &Mutex<ScanTree>,
    id: NodeId,
    files: Totals,
    sub_totals: &[Totals],
    progress: &ScanProgress,
) -> Totals {
    let totals = sub_totals.iter().fold(files, |acc, s| Totals {
        size: acc.size + s.size,
        files: acc.files + s.files,
        subdirs: acc.subdirs + 1 + s.subdirs,
    });
    lock(tree).set_totals(id, totals.size, totals.files, totals.subdirs);
    progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
    totals
}
