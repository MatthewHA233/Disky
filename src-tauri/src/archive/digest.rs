//! 每块盘的目录摘要。
//!
//! 扫描树只在内存里保留当前一块盘，档案馆却要跨盘汇总，所以每次扫描完成（或打开扫描记录）时
//! 从扫描树提炼一份摘要存到磁盘：浅层目录的大小、子树里有没有 exe、名字像缓存的深层目录。
//! 一块盘几千到两三万条，几 MB 以内。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::scanner::classify::is_cache_name;
use crate::scanner::{is_strict_ancestor, join_path, NodeId, ScanTree, ROOT};

const MB: u64 = 1024 * 1024;
/// 小于该大小的目录不记
const MIN_DIR: u64 = MB;
/// 记到第几层（盘根为 0）：`C:\Users\me\AppData\Local\Programs\cursor` 是第 6 层
const MAX_DEPTH: u8 = 6;
/// 更深的目录只记名字像缓存、且不小于该大小的
const MIN_DEEP_CACHE: u64 = 5 * MB;
/// 浅层的大文件（pagefile.sys、视频）也记下，方便归类和显示
const MIN_FILE: u64 = 50 * MB;
const MAX_FILE_DEPTH: u8 = 3;
/// 每个目录最多记几个 exe 名（找图标、判断软件是否已卸载用）
const MAX_EXES: usize = 6;
const VERSION: u32 = 3;

/// 子树里没有 exe
pub const NO_EXE: u8 = u8::MAX;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DigestEntry {
    pub path: String,
    pub size: u64,
    /// 盘根为 0
    pub depth: u8,
    pub is_dir: bool,
    /// 子树里最浅的 exe 相对本目录的层数：0 = 本目录下直接有 exe；没有为 `NO_EXE`
    pub exe_depth: u8,
    /// 本目录下直接放着的 exe（按大小降序）
    pub exes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveDigest {
    pub version: u32,
    /// 扫描根路径（Windows 形如 `C:`）
    pub root: String,
    /// 扫描完成时间（Unix 秒）
    pub scanned_at: i64,
    /// 扫描到的总大小
    pub total: u64,
    /// 最后一次改动时间（扫描、或删除后同步）：打开更早的扫描记录不能覆盖它
    pub modified_at: i64,
    /// 按路径排序
    pub entries: Vec<DigestEntry>,
}

impl DriveDigest {
    pub fn build(tree: &ScanTree, scanned_at: i64) -> Self {
        let mut entries = Vec::new();
        visit(tree, ROOT, tree.root_path(), 0, 0, &mut entries);
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        DriveDigest {
            version: VERSION,
            root: tree.root_path().to_string(),
            scanned_at,
            total: tree.node(ROOT).size(),
            modified_at: scanned_at,
            entries,
        }
    }

    /// 删除某个位置后同步摘要：去掉它和它下面的条目，祖先回减大小。
    /// `size` 用于摘要里没有记这个位置（太小或太深）的情况。
    pub fn forget(&mut self, path: &str, size: u64) -> bool {
        if !covers(&self.root, path) {
            return false;
        }
        let removed = self
            .entries
            .iter()
            .find(|e| e.path.eq_ignore_ascii_case(path))
            .map_or(size, |e| e.size);
        self.entries
            .retain(|e| !(e.path.eq_ignore_ascii_case(path) || is_strict_ancestor(path, &e.path)));
        for e in self.entries.iter_mut().filter(|e| is_strict_ancestor(&e.path, path)) {
            e.size = e.size.saturating_sub(removed);
        }
        self.total = self.total.saturating_sub(removed);
        true
    }
}

/// 不管多深都要记下的子目录层数：npm 全局包（npm\node_modules\pkg、@scope\pkg）、conda 环境
fn forced_levels(parent: &str, name: &str) -> u8 {
    let n = name.to_ascii_lowercase();
    let p = parent.to_ascii_lowercase();
    if n == "node_modules" && p == "npm" {
        2
    } else if n == "envs" && matches!(p.as_str(), "conda" | ".conda" | "anaconda3" | "miniconda3") {
        1
    } else {
        0
    }
}

/// 递归统计 `id` 的子项，满足条件的目录写进 `out`；返回子树里 exe 的最浅层数。
/// `force`：本目录不管多深都记（> 0），子目录继续记 force - 1 层
fn visit(tree: &ScanTree, id: NodeId, path: &str, depth: u8, force: u8, out: &mut Vec<DigestEntry>) -> u8 {
    let mut exe_depth = NO_EXE;
    let mut exes: Vec<(u64, &str)> = Vec::new();
    let child_depth = depth.saturating_add(1);
    let parent_name = crate::scanner::path_file_name(path.rsplit_once(crate::scanner::SEP).map_or("", |(h, _)| h));
    let kids_force = forced_levels(&parent_name, tree.name(id)).max(force.saturating_sub(1));
    for child in tree.children(id) {
        let node = tree.node(child);
        let name = tree.name(child);
        if node.is_dir() {
            let sub = visit(tree, child, &join_path(path, name), child_depth, kids_force, out);
            exe_depth = exe_depth.min(sub.saturating_add(1));
            continue;
        }
        if is_exe(name) {
            exe_depth = 0;
            exes.push((node.size(), name));
        }
        if node.size() >= MIN_FILE && child_depth <= MAX_FILE_DEPTH {
            out.push(DigestEntry {
                path: join_path(path, name),
                size: node.size(),
                depth: child_depth,
                is_dir: false,
                exe_depth: NO_EXE,
                exes: Vec::new(),
            });
        }
    }
    let size = tree.node(id).size();
    let wanted = id != ROOT
        && size >= MIN_DIR
        && (depth <= MAX_DEPTH || force > 0 || (size >= MIN_DEEP_CACHE && is_cache_name(&tree.name(id).to_ascii_lowercase())));
    if wanted {
        exes.sort_by(|a, b| b.0.cmp(&a.0));
        out.push(DigestEntry {
            path: path.to_string(),
            size,
            depth,
            is_dir: true,
            exe_depth,
            exes: exes.into_iter().take(MAX_EXES).map(|(_, n)| n.to_string()).collect(),
        });
    }
    exe_depth
}

/// 按字节比较后缀：文件名里有中文时按字符下标切片会切到半个字
fn is_exe(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() > 4 && b[b.len() - 4..].eq_ignore_ascii_case(b".exe")
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// 只给整块盘做摘要：扫描某个文件夹得到的树会和整盘的摘要重叠
pub fn is_volume_root(path: &str) -> bool {
    if cfg!(windows) {
        path.len() == 2 && path.as_bytes()[0].is_ascii_alphabetic() && path.ends_with(':')
    } else {
        path == "/" || path.strip_prefix("/Volumes/").is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
    }
}

/// `root` 是否包含 `path`（含相等）
fn covers(root: &str, path: &str) -> bool {
    root.eq_ignore_ascii_case(path) || is_strict_ancestor(root, path)
}

// ---------------------------------------------------------------------------
// 存取：app 数据目录下 archive/digest-<盘>.bin
// ---------------------------------------------------------------------------

fn file_key(root: &str) -> String {
    let key: String = root
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let key = key.trim_matches('_');
    if key.is_empty() { "root".into() } else { key.to_string() }
}

fn digest_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("digest-{}.bin", file_key(root)))
}

pub fn save(dir: &Path, digest: &DriveDigest) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let blob = bincode::serialize(digest).map_err(|e| e.to_string())?;
    let target = digest_path(dir, &digest.root);
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, blob).map_err(|e| format!("写入摘要失败: {e}"))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("写入摘要失败: {e}"))
}

pub fn load(dir: &Path, root: &str) -> Option<DriveDigest> {
    let blob = std::fs::read(digest_path(dir, root)).ok()?;
    bincode::deserialize::<DriveDigest>(&blob)
        .ok()
        .filter(|d| d.version == VERSION)
}

/// 读出所有盘的摘要；损坏或旧版本的跳过
pub fn load_all(dir: &Path) -> Vec<DriveDigest> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<DriveDigest> = read
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("digest-") && name.ends_with(".bin")
        })
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|blob| bincode::deserialize::<DriveDigest>(&blob).ok())
        .filter(|d| d.version == VERSION)
        .collect();
    all.sort_by(|a, b| a.root.cmp(&b.root));
    all
}

/// 上一份摘要至少隔这么久才轮换成「上次」：连着重扫几次不会把比较的基准冲掉
const ROTATE_AFTER_SECS: i64 = 6 * 3600;

fn prev_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("prev-{}.bin", file_key(root)))
}

/// 存一次新扫描的摘要：原来那份留作「上次」，用来比较软件、文件夹的大小变化。
/// 还没有「上次」时直接留；已经有了，原来那份够旧才换
pub fn save_rotating(dir: &Path, digest: &DriveDigest) -> Result<(), String> {
    let current = digest_path(dir, &digest.root);
    let has_prev = prev_path(dir, &digest.root).exists();
    let rotate = load(dir, &digest.root)
        .is_some_and(|old| old.scanned_at < digest.scanned_at && (!has_prev || digest.scanned_at - old.scanned_at >= ROTATE_AFTER_SECS));
    if rotate {
        std::fs::copy(&current, prev_path(dir, &digest.root)).map_err(|e| format!("留存上次的摘要失败: {e}"))?;
    }
    save(dir, digest)
}

/// 各盘「上次」的摘要（没有的盘不在里面）
pub fn load_prev_all(dir: &Path) -> Vec<DriveDigest> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("prev-"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|blob| bincode::deserialize::<DriveDigest>(&blob).ok())
        .filter(|d| d.version == VERSION)
        .collect()
}

/// 新摘要比已存的旧（比如打开了一份早先的扫描记录）就不覆盖
pub fn save_if_newer(dir: &Path, digest: &DriveDigest) -> Result<bool, String> {
    if load(dir, &digest.root).is_some_and(|old| old.scanned_at.max(old.modified_at) > digest.scanned_at) {
        return Ok(false);
    }
    save(dir, digest).map(|_| true)
}

/// 删除了一些位置后同步对应盘的摘要
pub fn forget_paths(dir: &Path, items: &[(String, u64)]) -> Result<(), String> {
    let mut digests = load_all(dir);
    for d in digests.iter_mut() {
        let mut changed = false;
        for (path, size) in items {
            changed |= d.forget(path, *size);
        }
        if changed {
            d.modified_at = now_secs();
            save(dir, d)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
