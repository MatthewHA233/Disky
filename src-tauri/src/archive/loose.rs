//! 档案馆文件夹树里一个位置下没单独列出的东西：小到不单独显示的子文件夹、没记进摘要的文件和小文件夹。
//! 用户点开时现场列这一层：摘要里记着的用摘要的大小（和树上的数一致），没记的去磁盘上数；
//! 归到软件名下的扣掉，加起来和树上「不到 50 MB 的文件和文件夹」那一行对得上。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::digest::{self, DriveDigest};
use super::inventory::{Claimed, MIN_NODE};
use crate::scanner::walk::{enumerate_dir, scan_tree, should_skip_dir, ScanProgress};
use crate::scanner::{is_strict_ancestor, join_path, ROOT, SEP};

/// 最多列多少项，其余的合成一行
const LIMIT: usize = 100;
/// 不到这个大小的不一项项列，只统计几项、一共多大
const TINY: u64 = 1024 * 1024;

#[derive(Debug, Serialize, PartialEq)]
pub struct LooseItem {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// 够大、本该单独列在树上，却没记进摘要：`rescan`（摘要旧了，重扫这块盘就有）/ `deep`（太深，扫描不记这么深）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrecorded: Option<&'static str>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LooseList {
    /// 从大到小，都不小于 1MB
    pub items: Vec<LooseItem>,
    /// 超出条数没列出来的还有几项、一共多大
    pub more: usize,
    pub more_size: u64,
    /// 不到 1MB 的几项、一共多大
    pub tiny: usize,
    pub tiny_size: u64,
}

fn parent_of(path: &str) -> &str {
    match path.rfind(SEP) {
        Some(0) => &path[..1],
        Some(i) => &path[..i],
        None => "",
    }
}

/// `dir` 在它那块盘上是第几层（盘根为 0）；几块盘都管时按最具体的那块
fn depth_in(digests: &[DriveDigest], dir: &str) -> u8 {
    let root = digests.iter().map(|d| d.root.as_str()).filter(|r| r.eq_ignore_ascii_case(dir) || is_strict_ancestor(r, dir)).max_by_key(|r| r.len());
    let rel = root.map_or("", |r| &dir[r.len()..]);
    rel.split(SEP).filter(|c| !c.is_empty()).count().min(u8::MAX as usize) as u8
}

/// 没记进摘要的一项够不够大、为什么没记
fn unrecorded(is_dir: bool, depth: u8, size: u64) -> Option<&'static str> {
    (size >= MIN_NODE).then(|| if digest::records(is_dir, depth) { "rescan" } else { "deep" })
}

/// 列 `dir` 这一层。`skip`：树上单独列着、或者放进了箱子的子位置；
/// `known`：树上有、小到不单独显示的子位置，用前端扣过箱子后的大小。两者都是小写路径
pub fn list(dir: &str, digests: &[DriveDigest], claimed: &Claimed, skip: &HashSet<String>, known: &HashMap<String, u64>) -> LooseList {
    let recorded: HashMap<String, u64> = digests
        .iter()
        .flat_map(|d| d.entries.iter())
        .filter(|e| parent_of(&e.path).eq_ignore_ascii_case(dir))
        .map(|e| (e.path.to_ascii_lowercase(), e.size))
        .collect();
    let depth = depth_in(digests, dir).saturating_add(1);
    let all: Vec<LooseItem> = enumerate_dir(dir)
        .into_iter()
        .filter_map(|raw| {
            let path = join_path(dir, &raw.name);
            let l = path.to_ascii_lowercase();
            if skip.contains(&l) || claimed.whole.contains(&l) || (raw.is_dir && should_skip_dir(&path)) {
                return None;
            }
            let below = claimed.below.get(&l).copied().unwrap_or(0);
            let (size, seen) = match (known.get(&l), recorded.get(&l)) {
                (Some(&s), _) => (s, true),
                (None, Some(&s)) => (s.saturating_sub(below), true),
                (None, None) if raw.is_dir => (scan_tree(&path, &ScanProgress::default()).node(ROOT).size().saturating_sub(below), false),
                (None, None) => (raw.size, false),
            };
            let unrecorded = if seen { None } else { unrecorded(raw.is_dir, depth, size) };
            (size > 0).then_some(LooseItem { path, name: raw.name, is_dir: raw.is_dir, size, unrecorded })
        })
        .collect();
    let (mut items, tiny): (Vec<LooseItem>, Vec<LooseItem>) = all.into_iter().partition(|i| i.size >= TINY);
    items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    let rest = items.split_off(items.len().min(LIMIT));
    LooseList {
        more: rest.len(),
        more_size: rest.iter().map(|i| i.size).sum(),
        tiny: tiny.len(),
        tiny_size: tiny.iter().map(|i| i.size).sum(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::digest::{DigestEntry, NO_EXE};
    use crate::scanner::{file_disk_size, normalize_path};

    const MB: u64 = 1024 * 1024;

    #[test]
    fn lists_one_level_minus_what_is_shown_or_claimed() {
        let base = std::env::temp_dir().join(format!("disky-loose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let write = |parts: &[&str], len: usize| {
            let p = parts.iter().fold(base.clone(), |p, s| p.join(s));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![1u8; len]).unwrap();
        };
        for name in ["Shown", "Mid", "Part", "Claimed", "Known"] {
            write(&[name, "x.bin"], 10);
        }
        write(&["small", "x.bin"], 5000);
        write(&["loose.txt"], 3000);
        let root = normalize_path(&base.to_string_lossy());
        let at = |name: &str| join_path(&root, name);
        let lower = |name: &str| at(name).to_ascii_lowercase();
        // 摘要里记着的大小故意和磁盘上的不一样：证明用的是摘要里的
        let entry = |name: &str, size: u64| DigestEntry { path: at(name), size, depth: 1, is_dir: true, exe_depth: NO_EXE, exes: Vec::new() };
        let digests = vec![DriveDigest {
            version: 4,
            root: root.clone(),
            scanned_at: 1,
            total: 0,
            modified_at: 1,
            entries: vec![entry("Shown", 900 * MB), entry("Mid", 40 * MB), entry("Part", 80 * MB), entry("Claimed", 70 * MB), entry("Known", 60 * MB)],
        }];
        let claimed = Claimed { whole: [lower("Claimed")].into(), below: [(lower("Part"), 30 * MB)].into() };
        let l = list(&root, &digests, &claimed, &[lower("Shown")].into(), &[(lower("Known"), 20 * MB)].into());

        let got: Vec<(&str, u64)> = l.items.iter().map(|i| (i.name.as_str(), i.size)).collect();
        assert_eq!(got, [("Part", 50 * MB), ("Mid", 40 * MB), ("Known", 20 * MB)], "归走的扣掉；树上扣过箱子的用前端的大小");
        assert!(l.items.iter().all(|i| i.unrecorded.is_none()), "记着的不算没记");
        let small = scan_tree(&at("small"), &ScanProgress::default()).node(ROOT).size();
        let file = file_disk_size(&std::fs::metadata(base.join("loose.txt")).unwrap());
        assert_eq!((l.tiny, l.tiny_size), (2, small + file), "没记的去磁盘上数，不到 1MB 的只统计");
        assert_eq!((l.more, l.more_size), (0, 0));
        assert_eq!(depth_in(&digests, &at("Part")), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn says_why_a_big_item_is_not_on_the_tree() {
        assert_eq!(unrecorded(false, 7, 60 * MB), Some("rescan"), "旧版只记到第 3 层的文件，重扫能补上");
        assert_eq!(unrecorded(true, 7, 60 * MB), Some("deep"), "文件夹只记到第 6 层");
        assert_eq!(unrecorded(false, 8, 60 * MB), Some("deep"));
        assert_eq!(unrecorded(true, 7, 40 * MB), None, "不到 50MB 的本来就合在一起");
    }
}
