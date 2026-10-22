use super::*;
use crate::scanner::NewChild;

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
    forget_paths(&dir, &[(p(&["Apps"]), 0)]).unwrap();
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
