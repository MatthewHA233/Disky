use super::*;
use super::live::{LayerCache, Measured, Unit};
use crate::scanner::walk::{scan_tree, ScanProgress};
use crate::scanner::{normalize_path, NewChild, SEP};
use std::collections::HashSet;

#[cfg(windows)]
const ROOT_PATH: &str = "C:";
#[cfg(not(windows))]
const ROOT_PATH: &str = "/";

/// D(名称, 子项) 是目录，F(名称, 字节) 是文件
enum S {
    D(&'static str, Vec<S>),
    F(&'static str, u64),
}
use S::{D, F};

fn fill(t: &mut ScanTree, id: NodeId, spec: Vec<S>) -> u64 {
    let first = t.push_children(id, spec.iter().map(|s| match s {
        D(name, _) => NewChild { name, size: 0, is_dir: true },
        F(name, size) => NewChild { name, size: *size, is_dir: false },
    }));
    let mut size = 0;
    for (i, s) in spec.into_iter().enumerate() {
        match s {
            D(_, kids) => size += fill(t, first + i as NodeId, kids),
            F(_, sz) => size += sz,
        }
    }
    t.set_totals(id, size, 0, 0);
    size
}

fn p(parts: &[&str]) -> String {
    parts.iter().fold(ROOT_PATH.to_string(), |acc, s| join_path(&acc, s))
}

fn sample() -> DriveDigest {
    let mut t = ScanTree::new(ROOT_PATH);
    fill(
        &mut t,
        ROOT,
        vec![
            D("Apps", vec![
                D("Tool", vec![F("tool.exe", 3 * MB), F("unins000.exe", MB), F("data.bin", 10 * MB)]),
                D("Gone", vec![F("readme.txt", 2 * MB)]),
                D("Deep", vec![D("a", vec![D("b", vec![F("deep.exe", 2 * MB)])])]),
            ]),
            D("u1", vec![D("u2", vec![D("u3", vec![D("u4", vec![D("u5", vec![D("u6", vec![
                D("Cache", vec![F("c.bin", 8 * MB)]),
                D("Other", vec![F("o.bin", 8 * MB)]),
            ])])])])])]),
            F("big.iso", 60 * MB),
            F("small.txt", 10),
        ],
    );
    DriveDigest::build(&t, 42)
}

fn entry<'a>(d: &'a DriveDigest, path: &str) -> &'a DigestEntry {
    d.entries.iter().find(|e| e.path == path).unwrap_or_else(|| panic!("没有 {path}"))
}

#[test]
fn records_sizes_exes_and_exe_depth() {
    let d = sample();
    assert_eq!(d.scanned_at, 42);
    assert_eq!(d.total, 3 * MB + MB + 10 * MB + 2 * MB + 2 * MB + 16 * MB + 60 * MB + 10);
    let tool = entry(&d, &p(&["Apps", "Tool"]));
    assert_eq!(tool.exe_depth, 0);
    assert_eq!(tool.exes, vec!["tool.exe", "unins000.exe"], "按大小降序");
    assert_eq!(entry(&d, &p(&["Apps", "Gone"])).exe_depth, NO_EXE);
    assert_eq!(entry(&d, &p(&["Apps", "Deep"])).exe_depth, 2);
    assert_eq!(entry(&d, &p(&["Apps"])).exe_depth, 1);
    assert!(entry(&d, &p(&["big.iso"])).size == 60 * MB && !entry(&d, &p(&["big.iso"])).is_dir);
    assert!(d.entries.iter().all(|e| !e.path.ends_with("small.txt")));
}

#[test]
fn deep_dirs_only_kept_when_cache_like() {
    let d = sample();
    let deep = p(&["u1", "u2", "u3", "u4", "u5", "u6"]);
    assert!(d.entries.iter().any(|e| e.path == deep), "第 6 层要记");
    assert!(d.entries.iter().any(|e| e.path == join_path(&deep, "Cache")), "更深的缓存要记");
    assert!(!d.entries.iter().any(|e| e.path == join_path(&deep, "Other")), "更深的普通目录不记");
}

#[test]
fn forget_removes_subtree_and_shrinks_ancestors() {
    let mut d = sample();
    let before = d.total;
    assert!(d.forget(&p(&["Apps", "Tool"]), 0));
    assert!(!d.entries.iter().any(|e| e.path.starts_with(&p(&["Apps", "Tool"]))));
    assert_eq!(entry(&d, &p(&["Apps"])).size, 2 * MB + 2 * MB);
    assert_eq!(d.total, before - 14 * MB);
    // 摘要里没有的位置按传入的大小扣
    assert!(d.forget(&p(&["Apps", "Deep", "a", "b", "deep.exe"]), 2 * MB));
    assert_eq!(entry(&d, &p(&["Apps", "Deep"])).size, 0);
    #[cfg(windows)]
    assert!(!d.forget("D:\\x", 1), "别的盘不动");
}

#[test]
fn save_load_and_keep_newer() {
    let dir = std::env::temp_dir().join(format!("disky-digest-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut d = sample();
    save(&dir, &d).unwrap();
    assert_eq!(load_all(&dir).len(), 1);
    d.scanned_at = 10;
    assert!(!save_if_newer(&dir, &d).unwrap(), "旧的不覆盖新的");
    assert_eq!(load(&dir, ROOT_PATH).unwrap().scanned_at, 42);
    forget_paths(&dir, &[(p(&["Apps"]), 0)], &HashSet::new()).unwrap();
    assert!(load(&dir, ROOT_PATH).unwrap().entries.iter().all(|e| !e.path.starts_with(&p(&["Apps"]))));
    // 删除同步之后，再打开一份比这次删除早的扫描记录，不能把删掉的东西带回来
    let mut reopened = sample();
    reopened.scanned_at = 100;
    assert!(!save_if_newer(&dir, &reopened).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rotating_keeps_a_previous_scan_as_baseline() {
    let dir = std::env::temp_dir().join(format!("disky-digest-rotate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let at = |t: i64| DriveDigest { scanned_at: t, ..sample() };
    save_rotating(&dir, &at(1_000)).unwrap();
    assert!(load_prev_all(&dir).is_empty(), "第一次扫描没有上次");
    save_rotating(&dir, &at(2_000)).unwrap();
    assert_eq!(load_prev_all(&dir)[0].scanned_at, 1_000, "还没有上次时直接留");
    save_rotating(&dir, &at(3_000)).unwrap();
    assert_eq!(load_prev_all(&dir)[0].scanned_at, 1_000, "连着重扫不冲掉基准");
    save_rotating(&dir, &at(3_000 + ROTATE_AFTER_SECS)).unwrap();
    assert_eq!(load_prev_all(&dir)[0].scanned_at, 3_000, "隔得够久才换");
    assert_eq!(load_all(&dir).len(), 1, "上次的不算在当前摘要里");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn npm_global_packages_are_kept_at_any_depth() {
    let mut t = ScanTree::new(ROOT_PATH);
    fill(&mut t, ROOT, vec![D("a", vec![D("b", vec![D("c", vec![D("d", vec![D("npm", vec![D("node_modules", vec![
        D("pnpm", vec![F("x.js", 3 * MB)]),
        D("@scope", vec![D("tool", vec![F("y.js", 2 * MB)])]),
    ])])])])])])]);
    let d = DriveDigest::build(&t, 1);
    let nm = p(&["a", "b", "c", "d", "npm", "node_modules"]);
    assert!(d.entries.iter().any(|e| e.path == join_path(&nm, "pnpm")), "第 7 层的包也要记");
    assert!(d.entries.iter().any(|e| e.path == join_path(&join_path(&nm, "@scope"), "tool")), "带作用域的包也要记");
}

#[test]
fn live_unit_counts_one_level_when_the_level_below_is_fully_recorded() {
    let d = sample();
    let dirs = d.dir_set();
    let unit = |path: String| d.unit_for(&dirs, &path);
    let layer = |parts: &[&str]| Some(Unit::Layer(p(parts)));
    let whole = |path: String| Some(Unit::Whole(path));
    // 记着的目录里的文件新建、删掉、变大，或者下面新建、删掉了没记过的小目录：只数这一层
    assert_eq!(unit(p(&["Apps", "Tool", "data.bin"])), layer(&["Apps", "Tool"]));
    assert_eq!(unit(p(&["Apps", "Tool", "new", "a", "b.txt"])), layer(&["Apps", "Tool"]));
    assert_eq!(unit(p(&["Apps", "Tool", "tiny"])), layer(&["Apps", "Tool"]));
    // 记着的目录自己删掉、改名了
    assert_eq!(unit(p(&["Apps", "Gone"])), whole(p(&["Apps", "Gone"])));
    // 第 7 层的普通目录本来就不记，大小不知道：整个重数第 6 层
    let deep = p(&["u1", "u2", "u3", "u4", "u5", "u6"]);
    assert_eq!(unit(join_path(&join_path(&deep, "Other"), "o.bin")), whole(deep.clone()));
    // 深层的缓存目录里的文件
    let cache = join_path(&deep, "Cache");
    assert_eq!(unit(join_path(&cache, "c.bin")), whole(cache));
    // 盘根下直接放着的东西：数盘根这一层，整盘大小就对上了
    assert_eq!(unit(p(&["Fresh", "x.txt"])), Some(Unit::Layer(ROOT_PATH.into())));
    assert_eq!(unit(p(&["big.iso"])), Some(Unit::Layer(ROOT_PATH.into())));
    assert_eq!(unit(ROOT_PATH.to_string()), None);
    #[cfg(windows)]
    assert_eq!(unit("D:\\Apps".into()), None, "别的盘不归它管");
}

#[test]
fn live_unit_inside_npm_global_packages() {
    let mut t = ScanTree::new(ROOT_PATH);
    fill(&mut t, ROOT, vec![D("a", vec![D("b", vec![D("c", vec![D("d", vec![D("npm", vec![D("node_modules", vec![
        D("pnpm", vec![F("x.js", 3 * MB)]),
    ])])])])])])]);
    let d = DriveDigest::build(&t, 1);
    let nm = p(&["a", "b", "c", "d", "npm", "node_modules"]);
    // node_modules 在第 6 层，但下面的包不管多深都记：没记过的包原来不到 1MB，数 node_modules 这一层就行
    let path = join_path(&join_path(&nm, "newpkg"), "i.js");
    assert_eq!(d.unit_for(&d.dir_set(), &path), Some(Unit::Layer(nm)));
}

fn measured(entries: Vec<DigestEntry>, size: u64) -> Measured {
    let exe_depth = entries.first().map_or(NO_EXE, |e| e.exe_depth);
    Measured { size, exe_depth, entries }
}

fn dir_entry(path: String, size: u64, depth: u8) -> DigestEntry {
    DigestEntry { path, size, depth, is_dir: true, exe_depth: NO_EXE, exes: Vec::new() }
}

#[test]
fn whole_remeasure_moves_the_difference_up() {
    let mut d = sample();
    let total = d.total;
    let tool = p(&["Apps", "Tool"]);
    // 重数之后 Tool 从 14MB 变成 20MB：上级和整盘一起加 6MB
    let mut e = entry(&d, &tool).clone();
    e.size = 20 * MB;
    let c = d.replace_unit(&tool, Some(measured(vec![e.clone()], 20 * MB))).unwrap();
    assert_eq!((c.delta, c.reshaped), (6 * MB as i64, false));
    assert_eq!(entry(&d, &p(&["Apps"])).size, 20 * MB + 2 * MB + 2 * MB);
    assert_eq!(d.total, total + 6 * MB);
    assert!(d.replace_unit(&tool, Some(measured(vec![e], 20 * MB))).is_none(), "没变就不算改动");

    // 记着的目录跌破 1MB：不再记，按不记的算 0 减掉全部；再长回来加上全部，来回一次不漂
    let gone = p(&["Apps", "Gone"]);
    let c = d.replace_unit(&gone, Some(measured(Vec::new(), MB / 2))).unwrap();
    assert_eq!((c.delta, c.reshaped), (-2 * MB as i64, true));
    let c = d.replace_unit(&gone, Some(measured(vec![dir_entry(gone.clone(), 2 * MB, 2)], 2 * MB))).unwrap();
    assert_eq!(c.delta, 2 * MB as i64);
    assert_eq!(d.total, total + 6 * MB);
    // 原来没记、现在也不够记：都算 0
    assert!(d.replace_unit(&p(&["Apps", "New"]), Some(measured(Vec::new(), MB / 2))).is_none());

    // 记着的删掉了
    let c = d.replace_unit(&gone, None).unwrap();
    assert_eq!((c.delta, c.reshaped), (-2 * MB as i64, true));
    assert!(d.replace_unit(&gone, None).is_none(), "已经去掉的不重复扣");
    assert_eq!(d.total, total + 4 * MB);
    assert!(d.entries.windows(2).all(|w| w[0].path < w[1].path), "条目保持按路径排序");
}

#[test]
fn missing_paths_then_forget_them() {
    let mut d = sample();
    let total = d.total;
    let gone = [p(&["Apps", "Gone"]), p(&["u1"])];
    let exists = |path: &str| -> Option<bool> {
        if path == p(&["Apps", "Tool"]) {
            return None; // 读不出来（没权限）的当它还在
        }
        Some(!gone.iter().any(|g| path == g || is_strict_ancestor(g, path)))
    };
    let missing = d.missing_paths(exists).unwrap();
    assert_eq!(missing, vec![p(&["Apps", "Gone"]), p(&["u1"])], "下面的不重复列");
    assert_eq!(d.forget_all(&missing), Some(Change { delta: -18 * MB as i64, reshaped: true }));
    assert!(d.entries.iter().all(|e| !e.path.starts_with(&p(&["u1"]))));
    assert!(d.entries.iter().any(|e| e.path == p(&["Apps", "Tool"])));
    assert_eq!(entry(&d, &p(&["Apps"])).size, 14 * MB + 2 * MB);
    assert_eq!(d.total, total - 18 * MB);
    assert!(d.missing_paths(exists).is_none(), "再核对一遍没有了");
    assert!(d.forget_all(&missing).is_none(), "已经去掉的不重复扣");
    // 整块盘没接上，或者同一个盘符换了一块盘（盘根下大半都不在）：一条都不动
    let d = sample();
    assert!(d.missing_paths(|_| Some(false)).is_none());
    assert!(d.missing_paths(|path| Some(path == ROOT_PATH)).is_none());
}

/// 在一个临时目录里搭一块「盘」：整盘扫描一次，改动之后按实时更新的办法重数，结果要和重新整盘扫描一样
#[test]
fn live_updates_match_a_fresh_full_scan() {
    let root = normalize_path(&std::env::temp_dir().join(format!("disky-live-{}", std::process::id())).to_string_lossy());
    let _ = std::fs::remove_dir_all(&root);
    let at = |parts: &[&str]| parts.iter().fold(root.clone(), |acc, s| join_path(&acc, s));
    let write = |parts: &[&str], size: usize| {
        let path = at(parts);
        std::fs::create_dir_all(path.rsplit_once(SEP).unwrap().0).unwrap();
        std::fs::write(path, vec![7u8; size]).unwrap();
    };
    write(&["A", "big.bin"], 2 * MB as usize);
    write(&["A", "tool.exe"], 10);
    write(&["A", "K", "small.txt"], 100_000);
    write(&["A", "B", "x.bin"], 3 * MB as usize);
    write(&["A", "C", "y.bin"], 2 * MB as usize);
    write(&["top.txt"], 1000);
    let scan = || DriveDigest::build(&scan_tree(&root, &ScanProgress::default()), 1);
    let mut d = scan();
    assert!(d.entries.iter().any(|e| e.path == at(&["A", "B"])));

    // 删一个文件、加一个文件、删掉没记过的小目录、新建一个大目录、删掉记着的目录、记着的目录里加东西、盘根下加文件
    std::fs::remove_file(at(&["A", "big.bin"])).unwrap();
    write(&["A", "new.bin"], 1536 * 1024);
    std::fs::remove_dir_all(at(&["A", "K"])).unwrap();
    write(&["A", "N", "f.bin"], 2 * MB as usize);
    std::fs::remove_dir_all(at(&["A", "B"])).unwrap();
    write(&["A", "C", "z.bin"], MB as usize);
    write(&["top2.txt"], 5000);
    let changed = [
        at(&["A", "big.bin"]), at(&["A", "new.bin"]), at(&["A", "K", "small.txt"]), at(&["A", "K"]),
        at(&["A", "N"]), at(&["A", "N", "f.bin"]), at(&["A", "B"]), at(&["A", "C", "z.bin"]), at(&["top2.txt"]),
    ];
    let mut cache = LayerCache::default();
    let replay = |d: &mut DriveDigest, changed: &[String], cache: &mut LayerCache| {
        let dirs = d.dir_set();
        let mut units: Vec<Unit> = changed.iter().filter_map(|c| d.unit_for(&dirs, c)).collect();
        // 深的先写；同一个位置只数一次
        units.sort_by(|a, b| b.path().len().cmp(&a.path().len()).then_with(|| a.path().cmp(b.path())));
        units.dedup();
        let measured: Vec<(Unit, Remeasure)> = units
            .into_iter()
            .map(|u| {
                let mine: Vec<String> = changed.iter().filter(|c| d.unit_for(&dirs, c).as_ref() == Some(&u)).cloned().collect();
                let m = measure(&root, &u, &dirs, &mine, cache);
                (u, m)
            })
            .collect();
        for (u, m) in measured {
            d.apply(&u, m);
        }
    };
    replay(&mut d, &changed, &mut cache);
    let fresh = scan();
    assert_eq!(d.total, fresh.total, "整盘大小对上");
    assert_eq!(d.entries, fresh.entries, "每一条都和重新扫描一样");

    // 第二轮：只有一个小子目录变了，别的小子目录用上次数的；藏一个没报的变化，证明真没重新走
    write(&["A", "S1", "a.txt"], 1000);
    write(&["A", "S2", "b.txt"], 2000);
    replay(&mut d, &[at(&["A", "S1"]), at(&["A", "S2"])], &mut cache);
    write(&["A", "S2", "unreported.txt"], 3000);
    write(&["A", "S1", "a2.txt"], 4000);
    replay(&mut d, &[at(&["A", "S1", "a2.txt"])], &mut cache);
    let fresh = scan();
    assert_eq!(d.total, fresh.total - 3000, "S2 用的是上次数的，没报的变化不算");
    write(&["A", "S2", "reported.txt"], 1);
    replay(&mut d, &[at(&["A", "S2", "reported.txt"])], &mut cache);
    assert_eq!(d.total, scan().total, "报了变化的子目录重新走");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn measure_reads_a_folder_like_a_full_scan() {
    let root = normalize_path(&std::env::temp_dir().join(format!("disky-measure-{}", std::process::id())).to_string_lossy());
    let _ = std::fs::remove_dir_all(&root);
    let app = join_path(&root, "app");
    std::fs::create_dir_all(join_path(&app, "data")).unwrap();
    std::fs::write(join_path(&join_path(&app, "data"), "blob.bin"), vec![7u8; 2 * MB as usize]).unwrap();
    std::fs::write(join_path(&app, "tool.exe"), b"MZ").unwrap();
    let none = HashSet::new();
    let mut cache = LayerCache::default();
    let Remeasure::Whole(m) = measure(&root, &Unit::Whole(app.clone()), &none, &[], &mut cache) else { panic!("应该还在") };
    assert!(m.size >= 2 * MB);
    let me = m.entries.iter().find(|e| e.path == app).expect("位置自己要记");
    assert_eq!((me.depth, me.exe_depth, me.exes.clone()), (1, 0, vec!["tool.exe".to_string()]));
    assert!(m.entries.iter().any(|e| e.path == join_path(&app, "data") && e.depth == 2));
    assert!(matches!(measure(&root, &Unit::Whole(join_path(&root, "missing")), &none, &[], &mut cache), Remeasure::Gone));
    assert_eq!(exists(&app), Some(true));
    assert_eq!(exists(&join_path(&root, "missing")), Some(false));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn exe_suffix_with_non_ascii_names() {
    assert!(is_exe("微信.exe"));
    assert!(is_exe("Setup.EXE"));
    assert!(!is_exe("语言包"));
    assert!(!is_exe("说明.txt"));
    assert!(!is_exe(".exe"));
}

#[test]
fn volume_roots() {
    if cfg!(windows) {
        assert!(is_volume_root("C:"));
        assert!(!is_volume_root("C:\\Users"));
    } else {
        assert!(is_volume_root("/"));
        assert!(is_volume_root("/Volumes/Data"));
        assert!(!is_volume_root("/Users"));
    }
}
