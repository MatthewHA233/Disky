//! 和上一次扫描比，每个软件、每个文件夹变大还是变小了多少。
//! 用各盘「上次」的摘要（digest::save_rotating 留下的）比，不用另存全量的扫描树。

use std::collections::HashMap;

use serde::Serialize;

use super::digest::DriveDigest;
use super::inventory::{FolderNode, Inventory};
use crate::scanner::is_strict_ancestor;

/// 变化小于这个的不报（扫描时正在写的日志、缓存总会有点浮动）
const MIN_CHANGE: i64 = 10 * 1024 * 1024;

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Growth {
    /// 软件 key → 比上次扫描多了多少字节（变小为负）
    pub apps: HashMap<String, i64>,
    /// 文件夹 key（dir:路径）→ 同上
    pub folders: HashMap<String, i64>,
    /// 盘根 → 上次扫描的时间（Unix 秒）
    pub since: HashMap<String, i64>,
}

fn sizes(digests: &[DriveDigest]) -> HashMap<String, u64> {
    digests.iter().flat_map(|d| d.entries.iter()).map(|e| (e.path.to_ascii_lowercase(), e.size)).collect()
}

pub fn compute(inv: &Inventory, current: &[DriveDigest], prev: &[DriveDigest]) -> Growth {
    let cur = sizes(current);
    let old = sizes(prev);
    let roots: Vec<String> = prev.iter().map(|d| d.root.to_ascii_lowercase()).collect();
    // 这块盘有上次的摘要才比；位置在上次里没有，说明那时还不到 1 MB，算从 0 长起来的
    let comparable = |path: &str| {
        let l = path.to_ascii_lowercase();
        roots.iter().any(|r| *r == l || is_strict_ancestor(r, &l))
    };
    let delta = |path: &str| {
        let l = path.to_ascii_lowercase();
        cur.get(&l).copied().unwrap_or(0) as i64 - old.get(&l).copied().unwrap_or(0) as i64
    };

    let mut g = Growth { since: prev.iter().map(|d| (d.root.clone(), d.scanned_at)).collect(), ..Default::default() };
    for app in &inv.apps {
        // 只算最外层的位置：里面嵌着的（缓存）已经含在外层的大小里了
        let outer = app.parts.iter().filter(|p| comparable(&p.path) && !app.parts.iter().any(|q| is_strict_ancestor(&q.path, &p.path)));
        let (n, d) = outer.fold((0, 0i64), |(n, d), p| (n + 1, d + delta(&p.path)));
        if n > 0 && d.abs() >= MIN_CHANGE {
            g.apps.insert(app.key.clone(), d);
        }
    }
    fn walk(n: &FolderNode, out: &mut HashMap<String, i64>, comparable: &dyn Fn(&str) -> bool, delta: &dyn Fn(&str) -> i64) {
        if n.depth > 0 && comparable(&n.path) {
            let d = delta(&n.path);
            if d.abs() >= MIN_CHANGE {
                out.insert(n.key.clone(), d);
            }
        }
        n.kids.iter().for_each(|k| walk(k, out, comparable, delta));
    }
    inv.tree.iter().for_each(|r| walk(r, &mut g.folders, &comparable, &delta));
    g
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::archive::digest::{DigestEntry, NO_EXE};
    use crate::archive::inventory::{AppItem, DriveInfo, Part};
    use crate::scanner::classify::Kind;

    const MB: u64 = 1024 * 1024;

    fn digest(root: &str, at: i64, entries: &[(&str, u64)]) -> DriveDigest {
        DriveDigest {
            version: 3,
            root: root.into(),
            scanned_at: at,
            total: 1 << 40,
            modified_at: at,
            entries: entries
                .iter()
                .map(|(p, s)| DigestEntry { path: p.to_string(), size: *s, depth: p.matches('\\').count() as u8, is_dir: true, exe_depth: NO_EXE, exes: Vec::new() })
                .collect(),
        }
    }

    fn part(path: &str, size: u64) -> Part {
        Part { path: path.into(), kind: Kind::Data, size, deletable: true, contents_only: false, origin: None, reason: None }
    }

    #[test]
    fn compares_outer_parts_and_folders() {
        let app = |key: &str, parts: Vec<Part>| AppItem {
            key: key.into(),
            name: key.into(),
            publisher: String::new(),
            icon: None,
            uninstalled: false,
            hint: None,
            category: "software".into(),
            parts,
        };
        let inv = Inventory {
            drives: vec![DriveInfo { root: "C:".into(), scanned_at: 2, total: 1 << 40 }],
            apps: vec![
                // 外层 Steam 长了 3 GB，里面的 htmlcache 不重复算
                app("app:steam", vec![part("C:\\Steam", 0), part("C:\\Steam\\htmlcache", 0)]),
                // 新装的（上次还没有）、几乎没变的、在没有上次摘要的盘上的
                app("app:new", vec![part("C:\\New", 0)]),
                app("app:same", vec![part("C:\\Same", 0)]),
                app("app:d", vec![part("D:\\Game", 0)]),
            ],
            tree: vec![FolderNode {
                key: "dir:C:".into(),
                path: "C:".into(),
                name: "C:".into(),
                is_dir: true,
                depth: 0,
                rest: 0,
                hint: None,
                kids: vec![FolderNode { key: "dir:C:\\Photos".into(), path: "C:\\Photos".into(), name: "Photos".into(), is_dir: true, depth: 1, rest: 0, hint: None, kids: Vec::new() }],
            }],
        };
        let current = [
            digest("C:", 2, &[("C:\\Steam", 5000 * MB), ("C:\\Steam\\htmlcache", 900 * MB), ("C:\\New", 300 * MB), ("C:\\Same", 501 * MB), ("C:\\Photos", 100 * MB)]),
            digest("D:", 2, &[("D:\\Game", 9000 * MB)]),
        ];
        let prev = [digest("C:", 1, &[("C:\\Steam", 2000 * MB), ("C:\\Steam\\htmlcache", 100 * MB), ("C:\\Same", 500 * MB), ("C:\\Photos", 400 * MB)])];
        let g = compute(&inv, &current, &prev);
        assert_eq!(g.apps.get("app:steam"), Some(&(3000 * MB as i64)));
        assert_eq!(g.apps.get("app:new"), Some(&(300 * MB as i64)), "上次还没有的从 0 算");
        assert!(!g.apps.contains_key("app:same"), "变化太小不报");
        assert!(!g.apps.contains_key("app:d"), "D 盘没有上次的摘要");
        assert_eq!(g.folders.get("dir:C:\\Photos"), Some(&(-(300 * MB as i64))), "变小为负");
        assert!(!g.folders.contains_key("dir:C:"), "盘根不报");
        assert_eq!(g.since.get("C:"), Some(&1));
    }
}
