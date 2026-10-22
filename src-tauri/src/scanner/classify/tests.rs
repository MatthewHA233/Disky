use super::*;
use crate::scanner::{NewChild, ROOT};

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

#[cfg(windows)]
const ROOT_PATH: &str = "C:";
#[cfg(not(windows))]
const ROOT_PATH: &str = "/data";

/// 用嵌套描述建树：D(名称, 子项) 是目录，F(名称, 字节) 是文件
enum S {
    D(&'static str, Vec<S>),
    F(&'static str, u64),
}
use S::{D, F};

fn fill(t: &mut ScanTree, id: NodeId, spec: Vec<S>) -> (u64, u64, u64) {
    let first = t.push_children(id, spec.iter().map(|s| match s {
        D(name, _) => NewChild { name, size: 0, is_dir: true },
        F(name, size) => NewChild { name, size: *size, is_dir: false },
    }));
    let (mut size, mut files, mut dirs) = (0, 0, 0);
    for (i, s) in spec.into_iter().enumerate() {
        let cid = first + i as NodeId;
        match s {
            D(_, kids) => {
                let (s2, f2, d2) = fill(t, cid, kids);
                size += s2;
                files += f2;
                dirs += 1 + d2;
            }
            F(_, sz) => {
                size += sz;
                files += 1;
            }
        }
    }
    t.set_totals(id, size, files, dirs);
    (size, files, dirs)
}

fn build(spec: Vec<S>) -> ScanTree {
    let mut t = ScanTree::new(ROOT_PATH);
    fill(&mut t, ROOT, spec);
    t
}

fn p(parts: &[&str]) -> String {
    parts.iter().fold(ROOT_PATH.to_string(), |acc, s| join_path(&acc, s))
}

fn at<'a>(fps: &'a [Footprint], path: &str) -> &'a Footprint {
    fps.iter()
        .find(|f| f.path == path && !f.remainder)
        .unwrap_or_else(|| panic!("没有找到 {path}，实际：{:#?}", fps.iter().map(|f| &f.path).collect::<Vec<_>>()))
}

fn rest_of<'a>(fps: &'a [Footprint], path: &str) -> Option<&'a Footprint> {
    fps.iter().find(|f| f.path == path && f.remainder)
}

fn home(kids: Vec<S>) -> S {
    D("Users", vec![D("me", kids)])
}

#[test]
fn program_files_are_programs_named_by_vendor() {
    let t = build(vec![D("Program Files", vec![
        D("Adobe", vec![F("ps.exe", 100 * MB)]),
        D("Microsoft VS Code", vec![F("code.exe", 50 * MB)]),
    ])]);
    let fps = classify(&t);
    let adobe = at(&fps, &p(&["Program Files", "Adobe"]));
    assert_eq!((adobe.kind, adobe.owner.as_str()), (Kind::Prog, "Adobe"));
    assert_eq!(at(&fps, &p(&["Program Files", "Microsoft VS Code"])).owner, "VS Code");
}

#[test]
fn appdata_vendor_splits_cache_from_data() {
    let t = build(vec![home(vec![D("AppData", vec![D("Roaming", vec![D("Code", vec![
        D("CachedData", vec![F("a", 30 * MB)]),
        D("User", vec![F("settings", 40 * MB)]),
        D("logs", vec![F("l", 10 * MB)]),
    ])])])])]);
    let fps = classify(&t);
    let code = p(&["Users", "me", "AppData", "Roaming", "Code"]);
    let cached = at(&fps, &join_path(&code, "CachedData"));
    assert_eq!((cached.kind, cached.owner.as_str(), cached.size), (Kind::Cache, "VS Code", 30 * MB));
    assert_eq!(at(&fps, &join_path(&code, "logs")).kind, Kind::Cache);
    // User 目录不是缓存，深入后没有更细的规则；深层不出碎片，汇总到 Code 顶层的其余数据
    assert!(rest_of(&fps, &join_path(&code, "User")).is_none());
    let rest = rest_of(&fps, &code).expect("Code 的其余数据");
    assert_eq!((rest.kind, rest.size, rest.owner.as_str()), (Kind::Data, 40 * MB, "VS Code"));
}

#[test]
fn vendor_version_dirs_are_programs_and_node_modules_regenerable() {
    let t = build(vec![home(vec![D("AppData", vec![D("Local", vec![
        D("Kingsoft", vec![D("WPS Office", vec![D("12.1.0.28505", vec![F("wps.exe", 900 * MB)])])]),
        D("Discord", vec![D("app-1.0.9", vec![F("d.exe", 200 * MB)])]),
        D("nvm", vec![D("v25.8.1", vec![F("node.exe", 80 * MB)])]),
        D("SomeTool", vec![D("node_modules", vec![F("m", 60 * MB)])]),
    ])])])]);
    let fps = classify(&t);
    let local = p(&["Users", "me", "AppData", "Local"]);
    let wps = at(&fps, &["Kingsoft", "WPS Office", "12.1.0.28505"].iter().fold(local.clone(), |a, s| join_path(&a, s)));
    assert_eq!((wps.kind, wps.owner.as_str()), (Kind::Prog, "WPS"));
    assert_eq!(at(&fps, &join_path(&join_path(&local, "Discord"), "app-1.0.9")).kind, Kind::Prog);
    assert_eq!(at(&fps, &join_path(&local, "nvm")).kind, Kind::Regen);
    assert_eq!(at(&fps, &join_path(&join_path(&local, "SomeTool"), "node_modules")).kind, Kind::Regen);
}

#[test]
fn version_name_detection() {
    for yes in ["12.1.0.28505", "app-1.2.3", "v25.8.1", "2.0.1", "app-1.0"] {
        assert!(is_version_name(yes), "{yes}");
    }
    for no in ["user data", "16.0", "1.0", "v8", "2024", "app-data", "1080p.video"] {
        assert!(!is_version_name(no), "{no}");
    }
}

#[test]
fn user_folders_list_children_with_known_owners() {
    let t = build(vec![home(vec![D("Documents", vec![
        D("WeChat Files", vec![F("img", 200 * MB)]),
        D("项目", vec![F("doc", 20 * MB)]),
        F("note.txt", MB),
    ])])]);
    let fps = classify(&t);
    let docs = p(&["Users", "me", "Documents"]);
    let wechat = at(&fps, &join_path(&docs, "WeChat Files"));
    assert_eq!((wechat.kind, wechat.owner.as_str()), (Kind::Data, "微信"));
    assert_eq!(at(&fps, &join_path(&docs, "项目")).owner, "文档");
    // 1 MB 的零散文件不足以单独成项
    assert!(rest_of(&fps, &docs).is_none());
}

#[test]
fn downloads_installers_are_residue() {
    let t = build(vec![home(vec![D("Downloads", vec![
        F("setup_v2.exe", 300 * MB),
        D("photos", vec![F("p", 50 * MB)]),
        F("data.zip", 20 * MB),
    ])])]);
    let fps = classify(&t);
    let dl = p(&["Users", "me", "Downloads"]);
    let setup = at(&fps, &join_path(&dl, "setup_v2.exe"));
    assert_eq!((setup.kind, setup.owner.as_str()), (Kind::Residue, "下载的安装包"));
    assert_eq!(at(&fps, &join_path(&dl, "photos")).kind, Kind::Data);
    // 压缩包可能是用户资料，不当成安装包
    assert_eq!(rest_of(&fps, &dl).map(|f| (f.kind, f.size)), Some((Kind::Data, 20 * MB)));
}

#[test]
fn steam_library_and_node_modules_are_regenerable() {
    let t = build(vec![
        D("SteamLibrary", vec![D("steamapps", vec![
            D("common", vec![F("game", 2 * GB)]),
            D("shadercache", vec![F("s", 100 * MB)]),
        ])]),
        D("code", vec![D("proj", vec![
            D("node_modules", vec![F("m", 300 * MB)]),
            D("src", vec![F("main.ts", MB)]),
        ])]),
    ]);
    let fps = classify(&t);
    let apps = p(&["SteamLibrary", "steamapps"]);
    let common = at(&fps, &join_path(&apps, "common"));
    assert_eq!((common.kind, common.owner.as_str()), (Kind::Regen, "Steam 游戏"));
    assert_eq!(at(&fps, &join_path(&apps, "shadercache")).kind, Kind::Cache);
    let nm = at(&fps, &p(&["code", "proj", "node_modules"]));
    assert_eq!((nm.kind, nm.owner.as_str()), (Kind::Regen, "code"));
}

#[test]
fn root_system_files_and_recycle_bin() {
    let t = build(vec![
        F("pagefile.sys", 8 * GB),
        D("$Recycle.Bin", vec![F("x", GB)]),
        D("Windows", vec![D("Temp", vec![F("t", 200 * MB)]), D("WinSxS", vec![F("w", 5 * GB)])]),
    ]);
    let fps = classify(&t);
    assert_eq!(at(&fps, &p(&["pagefile.sys"])).kind, Kind::System);
    assert_eq!(at(&fps, &p(&["$Recycle.Bin"])).kind, Kind::Residue);
    assert_eq!(at(&fps, &p(&["Windows", "Temp"])).kind, Kind::Cache);
    let win = rest_of(&fps, &p(&["Windows"])).expect("Windows 的其余部分");
    assert_eq!((win.kind, win.size), (Kind::System, 5 * GB));
}

#[test]
fn small_items_fold_into_remainder_and_sizes_add_up() {
    let t = build(vec![home(vec![D("AppData", vec![D("Local", vec![D("Foo", vec![
        F("a", 3 * MB),
        D("b", vec![F("b1", 2 * MB)]),
        F("c", 4 * MB),
    ])])])])]);
    let fps = classify(&t);
    let foo = p(&["Users", "me", "AppData", "Local", "Foo"]);
    assert_eq!(fps.len(), 1);
    let rest = rest_of(&fps, &foo).expect("Foo 的其余文件");
    assert_eq!((rest.kind, rest.size, rest.owner.as_str()), (Kind::Data, 9 * MB, "Foo"));
    assert_eq!(fps.iter().map(|f| f.size).sum::<u64>(), t.node(ROOT).size());
}

#[test]
fn results_do_not_overlap_and_are_sorted() {
    let t = build(vec![
        D("Program Files", vec![D("App", vec![F("a", 70 * MB)])]),
        home(vec![
            D("Documents", vec![D("Big", vec![F("b", 900 * MB)])]),
            D(".gradle", vec![F("g", 400 * MB)]),
        ]),
    ]);
    let fps = classify(&t);
    assert!(fps.windows(2).all(|w| w[0].size >= w[1].size));
    for a in &fps {
        for b in &fps {
            if !std::ptr::eq(a, b) && !a.remainder && !b.remainder {
                assert!(!crate::scanner::is_strict_ancestor(&a.path, &b.path), "{} 包含了 {}", a.path, b.path);
            }
        }
    }
    assert_eq!(fps.iter().map(|f| f.size).sum::<u64>(), t.node(ROOT).size());
}

#[test]
fn mac_library_and_applications() {
    let t = build(vec![
        D("Applications", vec![D("Xcode.app", vec![F("x", 10 * GB)])]),
        home(vec![D("Library", vec![
            D("Caches", vec![D("com.apple.Safari", vec![F("c", 500 * MB)])]),
            D("Application Support", vec![D("Google", vec![D("Chrome", vec![D("Default", vec![
                D("Cache", vec![F("c", 200 * MB)]),
                F("History", 50 * MB),
            ])])])]),
        ])]),
    ]);
    let fps = classify(&t);
    let xcode = at(&fps, &p(&["Applications", "Xcode.app"]));
    assert_eq!((xcode.kind, xcode.owner.as_str()), (Kind::Prog, "Xcode"));
    let lib = p(&["Users", "me", "Library"]);
    assert_eq!(at(&fps, &join_path(&join_path(&lib, "Caches"), "com.apple.Safari")).kind, Kind::Cache);
    let google = join_path(&join_path(&lib, "Application Support"), "Google");
    let default = ["Chrome", "Default"].iter().fold(google.clone(), |acc, s| join_path(&acc, s));
    let cache = at(&fps, &join_path(&default, "Cache"));
    assert_eq!((cache.kind, cache.owner.as_str()), (Kind::Cache, "Google"));
    // History 这类深层零散数据汇总到 Google 顶层
    assert_eq!(rest_of(&fps, &google).map(|f| f.size), Some(50 * MB));
}

#[test]
fn kind_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Kind::Regen).unwrap(), "\"regen\"");
    assert_eq!(serde_json::to_string(&Kind::System).unwrap(), "\"system\"");
}
