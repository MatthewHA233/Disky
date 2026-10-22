use super::*;

#[cfg(windows)]
const ROOT_PATH: &str = "C:";
#[cfg(not(windows))]
const ROOT_PATH: &str = "/data";

fn p(parts: &[&str]) -> String {
    parts.iter().fold(ROOT_PATH.to_string(), |acc, s| join_path(&acc, s))
}

fn child<'a>(name: &'a str, size: u64, is_dir: bool) -> NewChild<'a> {
    NewChild { name, size, is_dir }
}

/// 按扫描口径自底向上回填目录聚合值，返回 (size, files, subdirs)。
fn recompute(tree: &mut ScanTree, id: NodeId) -> (u64, u64, u64) {
    let kids: Vec<NodeId> = tree.children(id).collect();
    let (mut size, mut files, mut subdirs) = (0, 0, 0);
    for k in kids {
        if tree.node(k).is_dir() {
            let (s, f, d) = recompute(tree, k);
            size += s;
            files += f;
            subdirs += 1 + d;
        } else {
            size += tree.node(k).size();
            files += 1;
        }
    }
    tree.set_totals(id, size, files, subdirs);
    (size, files, subdirs)
}

/// ROOT
/// ├── a.txt        10
/// ├── Dir1/
/// │   ├── b.bin   100
/// │   └── sub/
/// │       └── c.log 5
/// └── dir2/        (空)
fn sample() -> ScanTree {
    let mut t = ScanTree::new(ROOT_PATH);
    let first = t.push_children(
        ROOT,
        [child("a.txt", 10, false), child("Dir1", 0, true), child("dir2", 0, true)],
    );
    let dir1 = first + 1;
    let d1 = t.push_children(dir1, [child("b.bin", 100, false), child("sub", 0, true)]);
    t.push_children(d1 + 1, [child("c.log", 5, false)]);
    recompute(&mut t, ROOT);
    t
}

fn names(entries: &[DirEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

#[test]
fn totals_follow_scan_semantics() {
    let t = sample();
    let root = t.node(ROOT);
    assert_eq!(root.size(), 115);
    assert_eq!(root.files(), 3);
    assert_eq!(root.subdirs(), 3);
    assert_eq!(t.node_count(), 7);
}

#[test]
fn find_is_case_insensitive_and_respects_boundaries() {
    let t = sample();
    assert_eq!(t.find(ROOT_PATH), Some(ROOT));
    let sub = t.find(&p(&["dir1", "SUB"])).expect("sub");
    assert_eq!(t.name(sub), "sub");
    assert!(t.find(&p(&["Dir1", "missing"])).is_none());
    assert!(t.find(&p(&["Dir1x"])).is_none());
    assert!(t.find(&format!("{ROOT_PATH}x")).is_none());
}

#[test]
fn path_of_rebuilds_canonical_path() {
    let t = sample();
    let c = t.find(&p(&["DIR1", "sub", "C.LOG"])).unwrap();
    assert_eq!(t.path_of(c), p(&["Dir1", "sub", "c.log"]));
    assert_eq!(t.path_of(ROOT), ROOT_PATH);
}

#[test]
fn child_entries_sorted_by_size_and_paged() {
    let t = sample();
    let (total, all) = t.child_entries(ROOT, 0, usize::MAX);
    assert_eq!(total, 3);
    assert_eq!(names(&all), ["Dir1", "a.txt", "dir2"]);
    assert_eq!(all[0].path, p(&["Dir1"]));
    assert_eq!(all[0].logical_size, 105);
    assert_eq!(all[0].files, 2);
    assert_eq!(all[0].subdirs, 1);

    let (total, page) = t.child_entries(ROOT, 1, 1);
    assert_eq!(total, 3);
    assert_eq!(names(&page), ["a.txt"]);

    let (_, beyond) = t.child_entries(ROOT, 10, 5);
    assert!(beyond.is_empty());
}

#[test]
fn remove_propagates_to_ancestors() {
    let mut t = sample();
    let sub = t.find(&p(&["Dir1", "sub"])).unwrap();
    assert!(t.remove(sub));
    assert!(t.find(&p(&["Dir1", "sub"])).is_none());

    let dir1 = t.node(t.find(&p(&["Dir1"])).unwrap());
    assert_eq!((dir1.size(), dir1.files(), dir1.subdirs()), (100, 1, 0));
    let root = t.node(ROOT);
    assert_eq!((root.size(), root.files(), root.subdirs()), (110, 2, 2));
    assert_eq!(t.node_count(), 5);

    let a = t.find(&p(&["a.txt"])).unwrap();
    assert!(t.remove(a));
    assert_eq!(t.node(ROOT).files(), 1);
    assert!(!t.remove(ROOT));
}

#[test]
fn replace_children_grafts_and_propagates() {
    let mut t = sample();
    let mut fresh = ScanTree::new(&p(&["Dir1"]));
    let first = fresh.push_children(
        ROOT,
        [child("new.iso", 1000, false), child("nested", 0, true)],
    );
    fresh.push_children(first + 1, [child("x", 1, false), child("y", 2, false)]);
    recompute(&mut fresh, ROOT);

    let dir1 = t.find(&p(&["Dir1"])).unwrap();
    assert!(t.replace_children(dir1, fresh));

    assert!(t.find(&p(&["Dir1", "sub"])).is_none());
    let y = t.find(&p(&["Dir1", "nested", "y"])).unwrap();
    assert_eq!(t.node(y).size(), 2);
    // 整块平移后的父指针与名字偏移仍然正确
    assert_eq!(t.path_of(y), p(&["Dir1", "nested", "y"]));
    let (_, kids) = t.child_entries(t.find(&p(&["Dir1"])).unwrap(), 0, usize::MAX);
    assert_eq!(names(&kids), ["new.iso", "nested"]);

    let dir1 = t.node(t.find(&p(&["Dir1"])).unwrap());
    assert_eq!((dir1.size(), dir1.files(), dir1.subdirs()), (1003, 3, 1));
    let root = t.node(ROOT);
    assert_eq!((root.size(), root.files(), root.subdirs()), (1013, 4, 3));
    // 旧子树 3 个节点成为垃圾，新增 4 个
    assert_eq!(t.node_count(), 8);
}

#[test]
fn replace_children_rejects_files_and_swaps_root() {
    let mut t = sample();
    let a = t.find(&p(&["a.txt"])).unwrap();
    assert!(!t.replace_children(a, ScanTree::new(&p(&["a.txt"]))));
    assert_eq!(t.node_count(), 7);

    let mut fresh = ScanTree::new(ROOT_PATH);
    fresh.push_children(ROOT, [child("only.bin", 42, false)]);
    recompute(&mut fresh, ROOT);
    assert!(t.replace_children(ROOT, fresh));
    assert_eq!(t.node_count(), 2);
    assert_eq!(t.node(ROOT).size(), 42);
    assert_eq!(t.root_path(), ROOT_PATH);
    assert!(t.find(&p(&["only.bin"])).is_some());
}

#[test]
fn find_backtracks_across_case_variants() {
    // 区分大小写的卷上 Build/ 与 build/ 可以并存，且 Build 排在前面
    let mut t = ScanTree::new(ROOT_PATH);
    let first = t.push_children(ROOT, [child("Build", 0, true), child("build", 0, true)]);
    t.push_children(first, [child("readme", 1, false)]);
    t.push_children(first + 1, [child("out", 2, false)]);
    recompute(&mut t, ROOT);

    let out = t.find(&p(&["build", "out"])).expect("应回溯到 build/");
    assert_eq!(t.path_of(out), p(&["build", "out"]));
    // 精确匹配优先：两个候选都存在时选字节完全相同的那个
    assert_eq!(t.find(&p(&["build"])), Some(first + 1));
    assert_eq!(t.find(&p(&["Build"])), Some(first));
    // 只有大小写不同的写法时仍能找到
    assert_eq!(t.find(&p(&["BUILD", "OUT"])), Some(out));
}

#[test]
fn compacted_keeps_only_live_nodes() {
    let mut t = sample();
    let dir1 = t.find(&p(&["Dir1"])).unwrap();
    t.remove(dir1);
    let c = t.compacted();
    assert_eq!(c.nodes.len(), 3);
    assert_eq!(c.garbage, 0);
    assert_eq!(c.node(ROOT).size(), 10);
    let (_, entries) = c.child_entries(ROOT, 0, usize::MAX);
    assert_eq!(names(&entries), ["a.txt", "dir2"]);
}

#[test]
fn walk_visits_with_parent_paths_and_can_stop() {
    let t = sample();
    let mut seen = Vec::new();
    t.walk(ROOT, |id, parent| {
        seen.push(join_path(parent, t.name(id)));
        Visit::Descend
    });
    seen.sort();
    let mut expected = vec![
        p(&["a.txt"]),
        p(&["Dir1"]),
        p(&["Dir1", "b.bin"]),
        p(&["Dir1", "sub"]),
        p(&["Dir1", "sub", "c.log"]),
        p(&["dir2"]),
    ];
    expected.sort();
    assert_eq!(seen, expected);

    let mut count = 0;
    t.walk(ROOT, |_, _| {
        count += 1;
        if count == 2 {
            Visit::Stop
        } else {
            Visit::Skip
        }
    });
    assert_eq!(count, 2);
}

#[test]
fn encode_decode_roundtrip() {
    let mut t = sample();
    let a = t.find(&p(&["a.txt"])).unwrap();
    t.remove(a); // 带垃圾编码，应自动整理
    let bytes = t.encode();
    assert!(ScanTree::is_encoded(&bytes));

    let d = ScanTree::decode(&bytes).expect("decode");
    assert_eq!(d.root_path(), ROOT_PATH);
    assert_eq!(d.node_count(), 6);
    assert_eq!(d.node(ROOT).size(), 105);
    let c = d.find(&p(&["dir1", "sub", "c.log"])).unwrap();
    assert_eq!(d.path_of(c), p(&["Dir1", "sub", "c.log"]));
}

#[test]
fn decode_rejects_corrupted_input() {
    let bytes = sample().encode();

    assert!(ScanTree::decode(&bytes[..bytes.len() - 1]).is_err());
    assert!(ScanTree::decode(b"nope").is_err());

    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    assert!(ScanTree::decode(&bad_magic).is_err());

    // 把第 2 个节点（a.txt）的 parent 改成自身下标 1，制造自环
    let header = 4 + 4 + 4 + ROOT_PATH.len() + 8 + 8;
    let parent_off = header + super::codec::NODE_BYTES + 8 + 4 + 4;
    let mut cyclic = bytes.clone();
    cyclic[parent_off..parent_off + 4].copy_from_slice(&1u32.to_le_bytes());
    assert!(ScanTree::decode(&cyclic).is_err());
}
