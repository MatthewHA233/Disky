//! 摘要的实时更新：盘上某处有变化时只重数那一处，换掉它在摘要里的条目，差量传给上级。
//!
//! 重数哪里（`unit_for`）：找摘要记着的、离变化最近的目录 A（盘根也算）。
//! - 变的就是 A 自己（删了、改名了）：整个重数 A。
//! - A 下面那一层整盘扫描时都记（A 在前 5 层，或是 npm 全局包这种不管多深都记的位置）：只数 A 这一层——
//!   直接放着的文件、没记过的子目录（原来都不到 1MB，整个数），记着的子目录沿用摘要里的大小。
//!   用户目录里的一个配置文件变了，不用把整个用户目录重新数一遍。
//! - 再往下的普通目录本来就不记、大小不知道：整个重数 A。
//!
//! 整个重数时跨过「够不够记」的门槛，按「不记的算 0」处理：长过门槛加上全部、跌破门槛减掉全部，
//! 误差有上限，不会来回几次就越积越多。

use std::collections::{HashMap, HashSet};

use super::*;
use crate::scanner::walk::{enumerate_dir, scan_tree, should_skip_dir, ScanProgress};
use crate::scanner::{path_file_name, SEP};

/// 要重数的位置
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unit {
    /// 整个重数这个位置和它下面的全部
    Whole(String),
    /// 只数这个记着的目录（或盘根）这一层
    Layer(String),
}

impl Unit {
    pub fn path(&self) -> &str {
        match self {
            Unit::Whole(p) | Unit::Layer(p) => p,
        }
    }
}

/// 整个重数一个位置的结果
pub struct Measured {
    pub size: u64,
    /// 子树里 exe 的最浅层数（相对这个位置）
    pub exe_depth: u8,
    /// 按整盘扫描同样的规则，这个位置自己（够大的话）和它下面要记的条目
    pub entries: Vec<DigestEntry>,
}

/// 只数一层的结果
#[derive(Default)]
pub struct Layer {
    /// 直接放着的文件的大小合计
    files: u64,
    /// 直接放着的 exe（大小, 名字）
    exes: Vec<(u64, String)>,
    /// 直接放着的大文件要记的条目
    file_entries: Vec<DigestEntry>,
    /// 没记过的子目录，各自整个数过
    kids: Vec<(String, Measured)>,
    /// 还在的、记着的子目录：大小在写进摘要时从摘要里取（可能刚被同一批更新过）
    recorded: Vec<String>,
}

pub enum Remeasure {
    /// 已经不在了
    Gone,
    Whole(Measured),
    Layer(Layer),
    /// 读不出来（没权限、符号链接）：这次不动它
    Unknown,
}

/// 摘要因为一次重数改了多少
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub delta: i64,
    /// 多了或少了条目（新出现、删掉的位置）
    pub reshaped: bool,
}

/// 盘根为 0；`path` 必须在 `root` 里面
fn depth_of(root: &str, path: &str) -> u8 {
    let rel = path.get(root.len()..).unwrap_or("").trim_start_matches(SEP);
    if rel.is_empty() { 0 } else { rel.split(SEP).count().min(u8::MAX as usize) as u8 }
}

/// 按 `visit` 的规则，从盘根一路往下推出这个位置的 force（npm 全局包、conda 环境下面不管多深都记）
fn force_at(root: &str, path: &str) -> u8 {
    let rel = path.get(root.len()..).unwrap_or("").trim_start_matches(SEP);
    let mut force = 0u8;
    let mut parent = String::new();
    let mut cur = path_file_name(root);
    for name in rel.split(SEP).filter(|c| !c.is_empty()) {
        force = forced_levels(&parent, &cur).max(force.saturating_sub(1));
        parent = std::mem::replace(&mut cur, name.to_string());
    }
    force
}

fn parent(path: &str) -> Option<&str> {
    let (head, _) = path.rsplit_once(SEP)?;
    Some(if head.is_empty() { &path[..1] } else { head })
}

fn is_child_of(dir: &str, path: &str) -> bool {
    parent(path).is_some_and(|p| p.eq_ignore_ascii_case(dir))
}

fn at_or_under(unit: &str, path: &str) -> bool {
    path.eq_ignore_ascii_case(unit) || is_strict_ancestor(unit, path)
}

/// 位置在不在：Some(true) 在，Some(false) 不在，None 读不出来（没权限等），这时当它还在
#[cfg(windows)]
pub fn exists(path: &str) -> Option<bool> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, INVALID_FILE_ATTRIBUTES};
    // \\?\ 前缀：深层缓存目录的路径可能超过 260 个字符
    let full = if path.ends_with(':') { format!("\\\\?\\{path}\\") } else { format!("\\\\?\\{path}") };
    let wide: Vec<u16> = full.encode_utf16().chain(Some(0)).collect();
    if unsafe { GetFileAttributesW(wide.as_ptr()) } != INVALID_FILE_ATTRIBUTES {
        return Some(true);
    }
    match unsafe { GetLastError() } {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Some(false),
        _ => None,
    }
}

/// 位置在不在：Some(true) 在，Some(false) 不在，None 读不出来（没权限等），这时当它还在
#[cfg(not(windows))]
pub fn exists(path: &str) -> Option<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

/// 只数一层时，没记过的小子目录上次数出来的大小和 exe 层数（目录、子目录都是小写）。
/// 这次没有变化的子目录直接用，不用重新走一遍：系统临时目录下面常有成千上万个
#[derive(Default)]
pub struct LayerCache(HashMap<String, HashMap<String, (u64, u8)>>);

impl LayerCache {
    /// 这块盘漏了变化：记下的大小不一定对了
    pub fn forget_root(&mut self, root: &str) {
        self.0.retain(|dir, _| !(dir.eq_ignore_ascii_case(root) || is_strict_ancestor(root, dir)));
    }

    /// 整个重数过的位置：里面记下的作废
    fn forget_under(&mut self, path: &str) {
        self.0.retain(|dir, _| !(dir.eq_ignore_ascii_case(path) || is_strict_ancestor(path, dir)));
    }
}

/// 重数盘 `root` 上的一个位置。`dirs` 是摘要记着的目录（`dir_set`）；`changed` 是这次落到这个位置的变化路径，
/// 只数一层时只有它们所在的子目录要重新走，其余的用 `cache` 里上次的
pub fn measure(root: &str, unit: &Unit, dirs: &HashSet<String>, changed: &[String], cache: &mut LayerCache) -> Remeasure {
    match unit {
        Unit::Whole(path) => {
            cache.forget_under(&path.to_ascii_lowercase());
            measure_whole(root, path)
        }
        Unit::Layer(dir) => {
            let touched: HashSet<String> = changed.iter().filter_map(|c| first_level(dir, c)).map(str::to_ascii_lowercase).collect();
            let key = dir.to_ascii_lowercase();
            let known = cache.0.remove(&key).unwrap_or_default();
            let (m, fresh) = measure_layer(root, dir, dirs, &known, &touched);
            if let Some(fresh) = fresh {
                cache.0.insert(key, fresh);
            }
            m
        }
    }
}

fn measure_whole(root: &str, path: &str) -> Remeasure {
    match exists(path) {
        Some(false) => return Remeasure::Gone,
        None => return Remeasure::Unknown,
        Some(true) => {}
    }
    let Ok(meta) = std::fs::symlink_metadata(path) else { return Remeasure::Unknown };
    // 扫描时跳过符号链接和挂载点，这里也不数
    if meta.file_type().is_symlink() {
        return Remeasure::Unknown;
    }
    let depth = depth_of(root, path);
    if !meta.is_dir() {
        let size = crate::scanner::file_disk_size(&meta);
        let entries = if size >= MIN_FILE && depth <= MAX_FILE_DEPTH {
            vec![DigestEntry { path: path.to_string(), size, depth, is_dir: false, exe_depth: NO_EXE, exes: Vec::new() }]
        } else {
            Vec::new()
        };
        return Remeasure::Whole(Measured { size, exe_depth: NO_EXE, entries });
    }
    let tree = scan_tree(path, &ScanProgress::default());
    let mut entries = Vec::new();
    let exe_depth = visit(&tree, ROOT, path, depth, force_at(root, path), true, &mut entries);
    Remeasure::Whole(Measured { size: tree.node(ROOT).size(), exe_depth, entries })
}

/// 和整盘扫描用同一个列目录的函数：跳过的东西（reparse point、符号链接）、文件大小的算法都一样。
/// 没记过的子目录：没变过、上次数过的用 `known` 里的，其余整个数；连同这次的结果一起交回去留给下次
fn measure_layer(
    root: &str,
    dir: &str,
    dirs: &HashSet<String>,
    known: &HashMap<String, (u64, u8)>,
    touched: &HashSet<String>,
) -> (Remeasure, Option<HashMap<String, (u64, u8)>>) {
    match exists(dir) {
        Some(false) => return (Remeasure::Gone, None),
        None => return (Remeasure::Unknown, None),
        Some(true) => {}
    }
    let depth = depth_of(root, dir);
    let mut layer = Layer::default();
    let mut fresh: HashMap<String, (u64, u8)> = HashMap::new();
    for raw in enumerate_dir(dir) {
        let path = join_path(dir, &raw.name);
        if raw.is_dir {
            if should_skip_dir(&path) {
                continue;
            }
            let lower = path.to_ascii_lowercase();
            if dirs.contains(&lower) {
                layer.recorded.push(path);
                continue;
            }
            let reuse = known.get(&lower).filter(|_| !touched.contains(&lower));
            let m = match reuse {
                Some(&(size, exe_depth)) => Measured { size, exe_depth, entries: Vec::new() },
                None => match measure_whole(root, &path) {
                    Remeasure::Whole(m) => m,
                    _ => continue,
                },
            };
            // 长到够记了的会被记进摘要，下次就是记着的子目录；只留还没记的
            if m.entries.is_empty() {
                fresh.insert(lower, (m.size, m.exe_depth));
            }
            layer.kids.push((path, m));
            continue;
        }
        layer.files += raw.size;
        if is_exe(&raw.name) {
            layer.exes.push((raw.size, raw.name.clone()));
        }
        if raw.size >= MIN_FILE && depth + 1 <= MAX_FILE_DEPTH {
            layer.file_entries.push(DigestEntry { path, size: raw.size, depth: depth + 1, is_dir: false, exe_depth: NO_EXE, exes: Vec::new() });
        }
    }
    (Remeasure::Layer(layer), Some(fresh))
}

fn sorted(mut v: Vec<DigestEntry>) -> Vec<DigestEntry> {
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

fn compare(before: Vec<DigestEntry>, after: Vec<DigestEntry>, delta: i64) -> Option<Change> {
    let (before, after) = (sorted(before), sorted(after));
    let reshaped = before.len() != after.len() || before.iter().zip(&after).any(|(a, b)| a.path != b.path);
    (before != after || delta != 0).then_some(Change { delta, reshaped })
}

impl DriveDigest {
    /// 这块盘管不管这个位置（盘根本身不算）
    pub fn covers(&self, path: &str) -> bool {
        is_strict_ancestor(&self.root, path)
    }

    /// 记着的目录（小写），给 `unit_for`、`measure` 查
    pub fn dir_set(&self) -> HashSet<String> {
        self.entries.iter().filter(|e| e.is_dir).map(|e| e.path.to_ascii_lowercase()).collect()
    }

    /// `path` 变了之后要重数哪里；不归这块盘管时返回 None
    pub fn unit_for(&self, dirs: &HashSet<String>, path: &str) -> Option<Unit> {
        if !self.covers(path) {
            return None;
        }
        // 往上找摘要记着的最深的目录；child 是它下面、通往 path 的那一层
        let mut child: Option<&str> = None;
        let mut cur = path;
        let anchor = loop {
            if cur.len() <= self.root.len() {
                break self.root.as_str();
            }
            if dirs.contains(&cur.to_ascii_lowercase()) {
                break cur;
            }
            child = Some(cur);
            cur = parent(cur)?;
        };
        let Some(child) = child else {
            // 记着的位置自己变了（删了、改名了）
            return Some(Unit::Whole(anchor.to_string()));
        };
        // 下面那一层整盘扫描时都记：没记过的就是原来不到 1MB，只数这一层就对得上
        let next_level_known = depth_of(&self.root, anchor) < MAX_DEPTH || force_at(&self.root, child) > 0;
        Some(if next_level_known { Unit::Layer(anchor.to_string()) } else { Unit::Whole(anchor.to_string()) })
    }

    /// 写进一次重数的结果；没有变化返回 None
    pub fn apply(&mut self, unit: &Unit, m: Remeasure) -> Option<Change> {
        match m {
            Remeasure::Unknown => None,
            Remeasure::Gone => self.replace_unit(unit.path(), None),
            Remeasure::Whole(m) => self.replace_unit(unit.path(), Some(m)),
            Remeasure::Layer(l) => self.replace_layer(unit.path(), l),
        }
    }

    /// 大小的差量传给 `path` 的各个上级和整盘；`exe_depth` 是 `path` 子树里 exe 的最浅层数
    fn propagate(&mut self, path: &str, delta: i64, exe_depth: u8) {
        let depth = depth_of(&self.root, path);
        for e in self.entries.iter_mut().filter(|e| is_strict_ancestor(&e.path, path)) {
            e.size = e.size.saturating_add_signed(delta);
            // 新装了 exe 的话上级的最浅层数跟着变浅；删掉 exe 变深的要等重扫
            if exe_depth != NO_EXE {
                e.exe_depth = e.exe_depth.min(exe_depth.saturating_add(depth.saturating_sub(e.depth)));
            }
        }
        self.total = self.total.saturating_add_signed(delta);
    }

    /// 整个重数的结果换掉 `unit` 和它下面的条目；`now` 为 None 表示已经不在了
    pub fn replace_unit(&mut self, unit: &str, now: Option<Measured>) -> Option<Change> {
        let old = self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(unit)).map(|e| e.size);
        let Some(m) = now else {
            let size = old?;
            self.forget(unit, 0);
            return Some(Change { delta: -(size as i64), reshaped: true });
        };
        let recorded_now = m.entries.iter().any(|e| e.path.eq_ignore_ascii_case(unit));
        // 原来没记、现在也不够记：都算 0，不动
        if old.is_none() && !recorded_now {
            return None;
        }
        let (before, keep): (Vec<DigestEntry>, Vec<DigestEntry>) =
            std::mem::take(&mut self.entries).into_iter().partition(|e| at_or_under(unit, &e.path));
        self.entries = keep;
        let after = m.entries;
        self.entries.extend(after.iter().cloned());
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        let delta = if recorded_now { m.size as i64 } else { 0 } - old.unwrap_or(0) as i64;
        self.propagate(unit, delta, m.exe_depth);
        compare(before, after, delta)
    }

    /// 只数一层的结果写进去：这一层直接放着的文件、没记过的子目录换成新的，
    /// 不在了的记着的子目录去掉，记着的子目录沿用摘要里的大小，合起来就是这个目录的新大小
    fn replace_layer(&mut self, dir: &str, l: Layer) -> Option<Change> {
        let is_root = dir.eq_ignore_ascii_case(&self.root);
        let old_entry = self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(dir)).cloned();
        // 这个目录刚被同一批里的别的更新去掉了（或者被删除同步去掉了）
        if !is_root && old_entry.is_none() {
            return None;
        }
        let old = if is_root { self.total } else { old_entry.as_ref().map_or(0, |e| e.size) };
        let recorded: HashSet<String> = l.recorded.iter().map(|p| p.to_ascii_lowercase()).collect();
        // 每条记录归这一层的哪个子项管；这一层的文件换成新列出来的，子目录记着且还在的留着，
        // 其余（不在了的、没记过重新数的）连同下面一起换掉。不在 dir 下面的不动
        let stale = |e: &DigestEntry| -> bool {
            if e.path.eq_ignore_ascii_case(dir) {
                return true;
            }
            let Some(kid) = first_level(dir, &e.path) else { return false };
            (kid.len() == e.path.len() && !e.is_dir) || !recorded.contains(&kid.to_ascii_lowercase())
        };
        let (before, keep): (Vec<DigestEntry>, Vec<DigestEntry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| stale(e));
        self.entries = keep;

        // 留下来的记着的子目录：大小、exe 层数按摘要里现在的
        let kept: HashMap<String, (u64, u8)> = self
            .entries
            .iter()
            .filter(|e| e.is_dir && is_child_of(dir, &e.path))
            .map(|e| (e.path.to_ascii_lowercase(), (e.size, e.exe_depth)))
            .collect();
        let kept_of = |p: &String| kept.get(&p.to_ascii_lowercase()).copied().unwrap_or((0, NO_EXE));
        let new_size = l.files + l.kids.iter().map(|(_, m)| m.size).sum::<u64>() + l.recorded.iter().map(|p| kept_of(p).0).sum::<u64>();
        let exe_depth = std::iter::once(if l.exes.is_empty() { NO_EXE } else { 0 })
            .chain(l.kids.iter().map(|(_, m)| m.exe_depth.saturating_add(1)))
            .chain(l.recorded.iter().map(|p| kept_of(p).1.saturating_add(1)))
            .min()
            .unwrap_or(NO_EXE);

        let mut after: Vec<DigestEntry> = l.file_entries;
        after.extend(l.kids.into_iter().flat_map(|(_, m)| m.entries));
        let delta = if is_root {
            new_size as i64 - old as i64
        } else if new_size >= MIN_DIR {
            let mut exes = l.exes;
            exes.sort_by(|a, b| b.0.cmp(&a.0));
            let prev = old_entry.expect("上面已经确认记着");
            after.push(DigestEntry {
                size: new_size,
                exe_depth,
                exes: exes.into_iter().take(MAX_EXES).map(|(_, n)| n).collect(),
                ..prev
            });
            new_size as i64 - old as i64
        } else {
            // 跌破门槛：不再记，按不记的算 0
            -(old as i64)
        };
        self.entries.extend(after.iter().cloned());
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        if is_root {
            self.total = new_size;
        } else {
            self.propagate(dir, delta, exe_depth);
        }
        compare(before, after, delta)
    }

    /// 摘要里已经不在磁盘上的位置（Disky 没开着时删的、监听漏掉的），只读磁盘不改摘要，可以在锁外面做。
    /// 整块盘没接上、或者看起来换了一块盘（盘根下记着的大半都不在了）时返回 None，免得把整块盘的摘要清空
    pub fn missing_paths(&self, exists: impl Fn(&str) -> Option<bool>) -> Option<Vec<String>> {
        if exists(&self.root) != Some(true) {
            return None;
        }
        let mut gone: Vec<String> = Vec::new();
        for e in &self.entries {
            if gone.iter().any(|g| is_strict_ancestor(g, &e.path)) {
                continue;
            }
            if exists(&e.path) == Some(false) {
                gone.push(e.path.clone());
            }
        }
        let top = self.entries.iter().filter(|e| e.depth == 1).count();
        let top_gone = gone.iter().filter(|g| depth_of(&self.root, g) == 1).count();
        // 核对的时候盘被拔了，或者同一个盘符换成了另一块盘
        if (top >= 2 && top_gone * 2 > top) || exists(&self.root) != Some(true) {
            return None;
        }
        (!gone.is_empty()).then_some(gone)
    }

    /// 去掉这些位置（`missing_paths` 找出来的）；一个都没去掉返回 None
    pub fn forget_all(&mut self, paths: &[String]) -> Option<Change> {
        let before = self.total;
        let mut removed = false;
        for p in paths {
            if self.entries.iter().any(|e| e.path.eq_ignore_ascii_case(p)) {
                removed |= self.forget(p, 0);
            }
        }
        removed.then(|| Change { delta: self.total as i64 - before as i64, reshaped: true })
    }
}

/// `path` 在 `dir` 下面时，它所在的 `dir` 的那个直接子项（可能就是它自己）
fn first_level<'a>(dir: &str, path: &'a str) -> Option<&'a str> {
    if !is_strict_ancestor(dir, path) {
        return None;
    }
    let rel = path[dir.len()..].trim_start_matches(SEP);
    let first = rel.split(SEP).next().unwrap_or("");
    Some(&path[..path.len() - rel.len() + first.len()])
}
