//! 用手写的 C / D 两块盘摘要验证识别结果（Windows 路径规则）。
#![cfg(windows)]

use super::*;

const GB: u64 = 1024 * MB;

fn e(path: &str, size: u64, exe_depth: u8, exes: &[&str]) -> DigestEntry {
    DigestEntry {
        path: path.to_string(),
        size,
        depth: path.matches('\\').count() as u8,
        is_dir: true,
        exe_depth,
        exes: exes.iter().map(|s| s.to_string()).collect(),
    }
}

fn file(path: &str, size: u64) -> DigestEntry {
    DigestEntry { path: path.to_string(), size, depth: path.matches('\\').count() as u8, is_dir: false, exe_depth: NO_EXE, exes: Vec::new() }
}

const N: u8 = NO_EXE;

fn drives() -> Vec<DriveDigest> {
    let c = vec![
        e("C:\\Windows", 20 * GB, 0, &["explorer.exe"]),
        e("C:\\Windows\\Temp", GB, N, &[]),
        e("C:\\Windows\\Installer", GB, N, &[]),
        e("C:\\Windows\\Installer\\$PatchCache$", 500 * MB, N, &[]),
        file("C:\\pagefile.sys", 8 * GB),
        e("C:\\Program Files (x86)", 4 * GB, 1, &[]),
        e("C:\\Program Files (x86)\\Steam", 3 * GB, 0, &["steam.exe", "uninstall.exe"]),
        e("C:\\Program Files (x86)\\Steam\\appcache", 200 * MB, N, &[]),
        e("C:\\Program Files", 8 * GB, 1, &[]),
        e("C:\\Program Files\\Tencent", 1600 * MB, 1, &[]),
        e("C:\\Program Files\\Tencent\\QQNT", GB, 0, &["QQ.exe"]),
        e("C:\\Program Files\\Tencent\\WeMeet", 500 * MB, 0, &["wemeetapp.exe"]),
        e("C:\\Program Files\\Battle.net", 20 * MB, N, &[]),
        e("C:\\Program Files\\Common Files", GB, 1, &[]),
        e("C:\\Program Files\\NVIDIA Corporation", 900 * MB, 1, &[]),
        e("C:\\Program Files\\Microsoft Office", 2 * GB, 1, &[]),
        e("C:\\Program Files\\Microsoft Office\\root", 2 * GB, 0, &["WINWORD.EXE"]),
        e("C:\\Users\\me\\AppData\\Local\\Microsoft", 700 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Microsoft\\Teams", 300 * MB, N, &[]),
        e("C:\\Users", 30 * GB, 1, &[]),
        e("C:\\Users\\me", 30 * GB, 1, &[]),
        e("C:\\Users\\me\\AppData", 20 * GB, 1, &[]),
        e("C:\\Users\\me\\AppData\\Local", 12 * GB, 1, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Steam", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Steam\\htmlcache", 250 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\nvm", 2 * GB, 1, &[]),
        e("C:\\Users\\me\\AppData\\Local\\nvm\\v20.18.1", 1536 * MB, 0, &["node.exe"]),
        e("C:\\Users\\me\\AppData\\Roaming\\npm", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\@anthropic-ai", 200 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\@anthropic-ai\\claude-code", 200 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\http-server", 50 * MB, N, &[]),
        e("C:\\Users\\me\\.vscode", 800 * MB, N, &[]),
        e("C:\\Users\\me\\.vscode\\extensions", 700 * MB, N, &[]),
        e("C:\\Users\\me\\.local", 300 * MB, N, &[]),
        e("C:\\Users\\me\\.local\\share", 300 * MB, N, &[]),
        e("C:\\Users\\me\\.local\\share\\claude", 300 * MB, N, &[]),
        e("C:\\Users\\me\\.claude", 100 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Temp", GB, 0, &["setup.exe"]),
        e("C:\\Users\\me\\AppData\\Local\\NVIDIA", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Programs", 2 * GB, 0, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Programs\\cursor", GB, 0, &["Cursor.exe"]),
        e("C:\\Users\\me\\AppData\\Local\\MysteryVendor", 700 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Programs\\Microsoft VS Code", 400 * MB, 0, &["Code.exe"]),
        e("C:\\Users\\me\\AppData\\Local\\Packages", 400 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Packages\\PythonSoftwareFoundation.Python", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Packages\\PythonSoftwareFoundation.Python\\LocalCache", 250 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Packages\\PythonSoftwareFoundation.Python\\AC", 20 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Packages\\PythonSoftwareFoundation.Python\\AC\\INetCache", 20 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Battle.net", 30 * MB, N, &[]),
        e("C:\\Users\\me\\code", 2 * GB, N, &[]),
        e("C:\\Users\\me\\code\\web", 2 * GB, N, &[]),
        e("C:\\Users\\me\\code\\web\\.cache", 600 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming", 8 * GB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Tencent", GB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Tencent\\QQ", 600 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Tencent\\WeMeet", 100 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\io.github.clash-verge-rev.clash-verge-rev", 50 * MB, N, &[]),
        e("C:\\Users\\me\\.cursor", 500 * MB, N, &[]),
        e("C:\\Users\\me\\Documents", 5 * GB, N, &[]),
        e("C:\\Users\\me\\Documents\\Tencent Files", GB, N, &[]),
        // 商店装的第三方软件、它们在 Packages 和 Roaming 里的数据；微软自家的包留在应用商店应用
        e("C:\\Program Files\\WindowsApps", 2 * GB, 1, &[]),
        e("C:\\Program Files\\WindowsApps\\Claude_1.2.0.0_x64__pzs8sxrjxfjjc", 500 * MB, 1, &[]),
        e("C:\\Program Files\\WindowsApps\\Claude_1.2.0.0_x64__pzs8sxrjxfjjc\\app", 500 * MB, 0, &["Claude.exe"]),
        e("C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.1.0.0_x64__2p2nqsd0c76g0", 300 * MB, 1, &[]),
        e("C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.2.0.0_x64__2p2nqsd0c76g0", 200 * MB, 1, &[]),
        e("C:\\Program Files\\WindowsApps\\Microsoft.Windows.Photos_2026.1.0.0_x64__8wekyb3d8bbwe", 800 * MB, 0, &["Photos.exe"]),
        e("C:\\Users\\me\\AppData\\Local\\Packages\\Claude_pzs8sxrjxfjjc", 100 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Claude", 200 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\Codex", 100 * MB, N, &[]),
        // WPS 装在用户目录里，登记项只有图标位置；数据目录用的是厂商名
        e("C:\\Users\\me\\AppData\\Local\\Kingsoft", 1400 * MB, 1, &[]),
        e("C:\\Users\\me\\AppData\\Local\\Kingsoft\\WPS Office", 1400 * MB, 0, &["ksolaunch.exe"]),
        e("C:\\Users\\me\\AppData\\Roaming\\kingsoft", GB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\kingsoft\\wps", 600 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Roaming\\kingsoft\\office6", 400 * MB, N, &[]),
        // 启动器里装的游戏的数据
        e("C:\\Users\\me\\AppData\\Local\\miHoYo", 50 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\miHoYo\\原神", 50 * MB, N, &[]),
        // 老版 QQ 和 QQNT 都有 QQ.exe，Roaming\Tencent\QQ 归名字就叫 QQ 的那个
        e("C:\\Program Files (x86)\\Tencent", 200 * MB, 1, &[]),
        e("C:\\Program Files (x86)\\Tencent\\QQ", 200 * MB, 0, &["QQ.exe"]),
        // 名字带版本号的数据目录、Squirrel 装的软件
        e("C:\\Users\\me\\AppData\\Local\\Programs\\Anki", 200 * MB, 0, &["anki.exe"]),
        e("C:\\Users\\me\\AppData\\Roaming\\Anki2", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\T2GP Launcher", 300 * MB, 1, &[]),
        e("C:\\Users\\me\\AppData\\Local\\T2GP Launcher\\app-1.15.0.6071", 280 * MB, 0, &["T2GP Launcher.exe"]),
        e("C:\\Users\\me\\AppData\\Roaming\\T2GP Launcher", 36 * MB, N, &[]),
    ];
    let d = vec![
        e("D:\\SteamLibrary", 50 * GB, 3, &[]),
        e("D:\\SteamLibrary\\steamapps", 50 * GB, 2, &[]),
        e("D:\\SteamLibrary\\steamapps\\shadercache", 2 * GB, N, &[]),
        e("D:\\Program Files", 800 * MB, 1, &[]),
        e("D:\\Program Files\\miHoYo Launcher", 400 * MB, 0, &["launcher.exe"]),
        e("D:\\Program Files\\Obsidian", 300 * MB, 0, &["Obsidian.exe"]),
        e("D:\\Program Files\\Clash Verge", 100 * MB, 0, &["clash-verge.exe"]),
        e("D:\\my files", 100 * GB, 3, &[]),
        e("D:\\my files\\game", 40 * GB, 2, &[]),
        e("D:\\my files\\notes", 100 * MB, N, &[]),
        e("D:\\Godot_v4.4.1-stable_win64", 100 * MB, 0, &["Godot_v4.4.1-stable_win64.exe"]),
        e("D:\\xwechat_files", 3 * GB, N, &[]),
        e("D:\\Temp", 2 * GB, 0, &["installer.exe"]),
        e("D:\\Games", 6 * GB, 1, &[]),
        e("D:\\Games\\ROMs", 5 * GB, N, &[]),
        e("D:\\Games\\Dolphin", GB, 0, &["Dolphin.exe"]),
        e("D:\\dev", 3 * GB, 1, &[]),
        e("D:\\dev\\LLVM", 2 * GB, 1, &[]),
        e("D:\\dev\\.cargo", GB, 1, &[]),
        e("D:\\Windows", 10 * GB, 1, &[]),
        e("D:\\Windows\\System32", 4 * GB, 0, &["cmd.exe"]),
        e("D:\\Java", 400 * MB, 2, &[]),
        e("D:\\Java\\jdk-21.0.11+10", 400 * MB, 1, &[]),
        e("D:\\Java\\jdk-21.0.11+10\\bin", 50 * MB, 0, &["java.exe"]),
        e("D:\\Strawberry", 800 * MB, 2, &[]),
        e("D:\\Strawberry\\perl", 300 * MB, 1, &[]),
        e("D:\\Photos", 300 * MB, N, &[]),
        e("D:\\Photos\\Strawberry", 300 * MB, N, &[]),
    ];
    let mut c_digest = DriveDigest { version: 1, root: "C:".into(), scanned_at: 1, total: 100 * GB, modified_at: 1, entries: c };
    let mut d_digest = DriveDigest { version: 1, root: "D:".into(), scanned_at: 2, total: 200 * GB, modified_at: 2, entries: d };
    c_digest.entries.sort_by(|a, b| a.path.cmp(&b.path));
    d_digest.entries.sort_by(|a, b| a.path.cmp(&b.path));
    vec![c_digest, d_digest]
}

fn installed() -> Vec<InstalledApp> {
    vec![
        InstalledApp {
            name: "Steam".into(),
            publisher: "Valve Corporation".into(),
            location: Some("C:\\Program Files (x86)\\Steam".into()),
            icon: None,
        },
        // 安装位置不存在的登记项忽略
        InstalledApp { name: "Ghost".into(), publisher: String::new(), location: Some("E:\\Ghost".into()), icon: None },
        InstalledApp {
            name: "WPS Office (12.1.0.28505)".into(),
            publisher: "Kingsoft Corp.".into(),
            location: None,
            icon: Some("C:\\Users\\me\\AppData\\Local\\Kingsoft\\WPS Office\\ksolaunch.exe".into()),
        },
        // 启动器和它装的游戏登记在同一个目录：名字取主程序那条，游戏名也能认数据
        InstalledApp {
            name: "原神".into(),
            publisher: "miHoYo Co.,Ltd".into(),
            location: Some("D:\\Program Files\\miHoYo Launcher".into()),
            icon: Some("D:\\Program Files\\miHoYo Launcher\\1.15.2.359\\ico\\hk4e_cn.ico".into()),
        },
        InstalledApp {
            name: "米哈游启动器".into(),
            publisher: "miHoYo Co.,Ltd".into(),
            location: None,
            icon: Some("D:\\Program Files\\miHoYo Launcher\\launcher.exe".into()),
        },
        InstalledApp {
            name: "腾讯 QQ".into(),
            publisher: "Tencent".into(),
            location: Some("C:\\Program Files (x86)\\Tencent\\QQ".into()),
            icon: None,
        },
    ]
}

fn env() -> Env {
    Env { system_root: Some("C:\\Windows".into()), temp_dirs: vec!["D:\\Temp".into()], toolchain_vars: vec![("CARGO_HOME".into(), "D:\\dev\\.cargo".into())] }
}

fn run() -> Inventory {
    build(&drives(), &installed(), &env(), &[])
}

fn app<'a>(inv: &'a Inventory, name: &str) -> &'a AppItem {
    inv.apps
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("没有 {name}，实际：{:?}", inv.apps.iter().map(|a| &a.name).collect::<Vec<_>>()))
}

fn part<'a>(a: &'a AppItem, path: &str) -> &'a Part {
    a.parts
        .iter()
        .find(|p| p.path == path)
        .unwrap_or_else(|| panic!("{} 没有 {path}，实际：{:?}", a.name, a.parts.iter().map(|p| &p.path).collect::<Vec<_>>()))
}

#[test]
fn registered_app_collects_parts_across_drives() {
    let inv = run();
    let steam = app(&inv, "Steam");
    assert_eq!(steam.hint.as_deref(), Some("game"));
    assert!(!steam.uninstalled);
    assert_eq!(steam.icon.as_deref(), Some("C:\\Program Files (x86)\\Steam\\steam.exe"));
    let prog = part(steam, "C:\\Program Files (x86)\\Steam");
    assert_eq!((prog.kind, prog.size), (Kind::Prog, 3 * GB - 200 * MB), "缓存从程序目录里扣掉");
    assert_eq!(part(steam, "C:\\Program Files (x86)\\Steam\\appcache").kind, Kind::Cache);
    assert_eq!(part(steam, "C:\\Users\\me\\AppData\\Local\\Steam").size, 50 * MB);
    assert_eq!(part(steam, "C:\\Users\\me\\AppData\\Local\\Steam\\htmlcache").kind, Kind::Cache);
    let lib = part(steam, "D:\\SteamLibrary");
    assert_eq!((lib.kind, lib.size), (Kind::Regen, 48 * GB));
    assert_eq!(part(steam, "D:\\SteamLibrary\\steamapps\\shadercache").kind, Kind::Cache);
    assert!(inv.apps.iter().all(|a| a.name != "Ghost"));
}

#[test]
fn vendor_folder_is_split_into_apps_with_their_data() {
    let inv = run();
    let qq = app(&inv, "QQ");
    assert_eq!(qq.hint.as_deref(), Some("chat"));
    assert_eq!(qq.icon.as_deref(), Some("C:\\Program Files\\Tencent\\QQNT\\QQ.exe"));
    assert_eq!(part(qq, "C:\\Users\\me\\AppData\\Roaming\\Tencent\\QQ").kind, Kind::Data);
    assert_eq!(part(qq, "C:\\Users\\me\\Documents\\Tencent Files").kind, Kind::Data);
    let meet = app(&inv, "腾讯会议");
    part(meet, "C:\\Users\\me\\AppData\\Roaming\\Tencent\\WeMeet");
    let office = app(&inv, "Microsoft Office");
    assert!(
        office.parts.iter().all(|p| !p.path.contains("AppData\\Local\\Microsoft")),
        "厂商目录不能因为名字开头一样就整个归给 Office"
    );
}

#[test]
fn program_dir_without_exe_is_residue() {
    let inv = run();
    let bnet = app(&inv, "Battle.net");
    assert!(bnet.uninstalled);
    assert!(bnet.icon.is_none());
    assert!(bnet.parts.iter().filter(|p| p.path.contains("Program Files")).all(|p| p.kind == Kind::Residue));
}

#[test]
fn system_toolchains_and_portable_apps() {
    let inv = run();
    let win = app(&inv, "Windows");
    assert_eq!(win.hint.as_deref(), Some("system"));
    assert_eq!(part(win, "C:\\Windows\\Temp").kind, Kind::Cache, "系统目录只认 Temp");
    assert!(win.parts.iter().all(|p| !p.path.contains("$PatchCache$")));
    part(app(&inv, "虚拟内存与休眠文件"), "C:\\pagefile.sys");
    part(app(&inv, "Windows 组件"), "C:\\Program Files\\Common Files");
    let nv = app(&inv, "NVIDIA 驱动");
    part(nv, "C:\\Users\\me\\AppData\\Local\\NVIDIA");
    assert_eq!(part(app(&inv, "nvm（Node.js 版本管理）"), "C:\\Users\\me\\AppData\\Local\\nvm").kind, Kind::Regen);
    assert_eq!(part(app(&inv, "临时文件"), "C:\\Users\\me\\AppData\\Local\\Temp").kind, Kind::Cache);
    let godot = app(&inv, "Godot");
    assert_eq!(godot.hint.as_deref(), Some("dev"));
    assert!(inv.apps.iter().all(|a| a.name != "Temp"), "盘根的 Temp 不是软件");
    let moved_temp = part(app(&inv, "临时文件"), "D:\\Temp");
    assert!(moved_temp.contents_only && moved_temp.kind == Kind::Cache, "TEMP 改到 D 盘时认那里");
}

#[test]
fn data_dirs_match_by_name_and_reverse_domain() {
    let inv = run();
    let cursor = app(&inv, "cursor");
    assert_eq!(part(cursor, "C:\\Users\\me\\.cursor").kind, Kind::Data);
    let clash = app(&inv, "Clash Verge");
    part(clash, "C:\\Users\\me\\AppData\\Roaming\\io.github.clash-verge-rev.clash-verge-rev");
    part(app(&inv, "Obsidian"), "D:\\Program Files\\Obsidian");
}

#[test]
fn review_safety_cases() {
    let inv = run();
    // 应用商店应用的 LocalCache 是应用的真实数据，不算缓存；INetCache 算
    let store = app(&inv, "应用商店应用");
    assert!(store.parts.iter().all(|p| !(p.path.ends_with("LocalCache") && p.kind == Kind::Cache)));
    assert_eq!(part(store, "C:\\Users\\me\\AppData\\Local\\Packages\\PythonSoftwareFoundation.Python\\AC\\INetCache").kind, Kind::Cache);
    // 软件容器里没有 exe 的资料目录不当软件；有 exe 的照常认
    assert!(inv.apps.iter().all(|a| a.name != "ROMs"));
    app(&inv, "Dolphin");
    // ~\code 是用户的项目，不归给 VS Code，里面的 .cache 也不当缓存
    let code = app(&inv, "Microsoft VS Code");
    assert!(code.parts.iter().all(|p| !p.path.starts_with("C:\\Users\\me\\code")));
    // 卸载残留只有程序目录变成残留，按名字认出的数据目录还是数据
    let bnet = app(&inv, "Battle.net");
    assert_eq!(part(bnet, "C:\\Users\\me\\AppData\\Roaming\\Battle.net").kind, Kind::Data);
    // 受保护的位置标成不能删；临时文件夹只清空里面
    let steam = app(&inv, "Steam");
    assert!(!part(steam, "C:\\Program Files (x86)\\Steam\\appcache").deletable);
    assert!(part(steam, "C:\\Users\\me\\AppData\\Local\\Steam\\htmlcache").deletable);
    let temp = part(app(&inv, "临时文件"), "C:\\Users\\me\\AppData\\Local\\Temp");
    assert!(temp.deletable && temp.contents_only);
    assert!(!part(app(&inv, "Windows"), "C:\\Windows\\Temp").deletable);
}

#[test]
fn dev_environment_items_are_split_out() {
    let inv = run();
    // 每个 Node 版本一项，从 nvm 里扣掉
    let node = app(&inv, "Node.js 20.18.1");
    assert_eq!((node.category.as_str(), node.hint.as_deref()), ("devenv", Some("env-node")));
    assert_eq!(part(app(&inv, "nvm（Node.js 版本管理）"), "C:\\Users\\me\\AppData\\Local\\nvm").size, 512 * MB);
    // npm 装的 Claude Code、原生安装、~\.claude 合成一项命令行工具
    let cc = app(&inv, "Claude Code");
    assert_eq!((cc.category.as_str(), cc.hint.as_deref()), ("devenv", Some("env-cli")));
    part(cc, "C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\@anthropic-ai\\claude-code");
    part(cc, "C:\\Users\\me\\.local\\share\\claude");
    part(cc, "C:\\Users\\me\\.claude");
    assert_eq!(app(&inv, "http-server").publisher, "npm 全局包");
    // 编辑器扩展单独一项，编辑器自己还是软件
    let ext = app(&inv, "VS Code 扩展");
    assert_eq!((ext.category.as_str(), ext.hint.as_deref()), ("devenv", Some("env-ext")));
    assert_eq!(app(&inv, "Microsoft VS Code").category, "software");
    // 环境变量指到 D:\dev 的工具链、放在 D:\dev 里的 LLVM
    part(app(&inv, "Rust 工具链"), "D:\\dev\\.cargo");
    assert_eq!(app(&inv, "LLVM").category, "devenv");
    // 盘上随手放的 JDK、Strawberry Perl；同名的照片文件夹里没有 perl 不认
    let jdk = app(&inv, "JDK 21.0.11");
    assert_eq!((jdk.category.as_str(), jdk.hint.as_deref()), ("devenv", Some("env-lang")));
    part(jdk, "D:\\Java\\jdk-21.0.11+10");
    part(app(&inv, "Strawberry Perl"), "D:\\Strawberry");
    assert!(inv.apps.iter().all(|a| a.parts.iter().all(|p| !p.path.starts_with("D:\\Photos"))));
    // 别的盘上的整套 Windows 认成系统，但不当残留
    let other = app(&inv, "另一个 Windows 系统");
    assert_eq!((other.hint.as_deref(), part(other, "D:\\Windows").kind), (Some("system"), Kind::Prog));
    // 个人文件夹按名字给出建议
    let d = inv.tree.iter().find(|n| n.path == "D:").unwrap();
    let my = d.kids.iter().find(|k| k.name == "my files").unwrap();
    assert_eq!(my.kids[0].hint.as_deref(), Some("p-game"));
}

#[test]
fn store_packages_launchers_and_vendor_data() {
    let inv = run();
    // 商店装的第三方软件各成一项，几个版本并在一起；Packages、Roaming 里的数据跟过去
    let claude = app(&inv, "Claude");
    part(claude, "C:\\Program Files\\WindowsApps\\Claude_1.2.0.0_x64__pzs8sxrjxfjjc");
    part(claude, "C:\\Users\\me\\AppData\\Local\\Packages\\Claude_pzs8sxrjxfjjc");
    part(claude, "C:\\Users\\me\\AppData\\Roaming\\Claude");
    assert_eq!(claude.icon.as_deref(), Some("C:\\Program Files\\WindowsApps\\Claude_1.2.0.0_x64__pzs8sxrjxfjjc\\app\\Claude.exe"));
    let codex = app(&inv, "Codex");
    assert_eq!(codex.publisher, "OpenAI");
    assert_eq!(codex.parts.iter().filter(|p| p.path.contains("WindowsApps")).count(), 2);
    part(codex, "C:\\Users\\me\\AppData\\Roaming\\Codex");
    assert!(inv.apps.iter().all(|a| !a.name.contains("Photos")), "微软自家的包不单列");
    let store = app(&inv, "应用商店应用");
    assert_eq!(part(store, "C:\\Program Files\\WindowsApps").size, 2 * GB - 500 * MB - 300 * MB - 200 * MB, "拆出去的从商店里扣掉");
    // WPS 只登记了图标：按图标退回用户目录里的安装目录，厂商名的数据目录归它
    let wps = app(&inv, "WPS Office");
    part(wps, "C:\\Users\\me\\AppData\\Local\\Kingsoft");
    part(wps, "C:\\Users\\me\\AppData\\Roaming\\kingsoft");
    // 启动器和游戏登记在同一目录：名字取主程序那条，游戏名也留着认数据
    part(app(&inv, "米哈游启动器"), "C:\\Users\\me\\AppData\\Local\\miHoYo\\原神");
    // 打平时名字就是目录名的胜出
    part(app(&inv, "QQ"), "C:\\Users\\me\\AppData\\Roaming\\Tencent\\QQ");
    // 名字后面带版本号、Squirrel 装的软件
    part(app(&inv, "Anki"), "C:\\Users\\me\\AppData\\Roaming\\Anki2");
    let t2 = app(&inv, "T2GP Launcher");
    part(t2, "C:\\Users\\me\\AppData\\Local\\T2GP Launcher");
    part(t2, "C:\\Users\\me\\AppData\\Roaming\\T2GP Launcher");
}

#[test]
fn overrides_win_over_rules() {
    let steam_key = app(&run(), "Steam").key.clone();
    let ov = |path: &str, key: Option<&str>, kind: Kind| Override {
        path: path.into(),
        app_key: key.map(str::to_string),
        kind,
        origin: "user".into(),
        reason: String::new(),
    };
    let inv = build(&drives(), &installed(), &env(), &[
        // 没认出来的文件夹指给 Steam；规则认成缓存的改成数据；规则认给 Battle.net 的说不是它的
        ov("C:\\Users\\me\\AppData\\Local\\MysteryVendor", Some(&steam_key), Kind::Data),
        ov("C:\\Users\\me\\AppData\\Local\\Steam\\htmlcache", Some(&steam_key), Kind::Data),
        ov("C:\\Users\\me\\AppData\\Roaming\\Battle.net", None, Kind::Data),
        // 说不是缓存的，找缓存时不再按名字认回来
        ov("D:\\SteamLibrary\\steamapps\\shadercache", None, Kind::Data),
        // 指向这次没认出的软件就整条跳过，规则原来的判断照旧
        ov("C:\\Users\\me\\AppData\\Roaming\\Tencent\\QQ", Some("app:gone"), Kind::Data),
    ]);
    assert!(app(&inv, "Steam").parts.iter().all(|p| !p.path.ends_with("shadercache")));
    part(app(&inv, "QQ"), "C:\\Users\\me\\AppData\\Roaming\\Tencent\\QQ");
    let steam = app(&inv, "Steam");
    let mystery = part(steam, "C:\\Users\\me\\AppData\\Local\\MysteryVendor");
    assert_eq!((mystery.kind, mystery.origin.as_deref()), (Kind::Data, Some("user")));
    assert_eq!(part(steam, "C:\\Users\\me\\AppData\\Local\\Steam\\htmlcache").kind, Kind::Data);
    assert_eq!(part(steam, "C:\\Users\\me\\AppData\\Local\\Steam").size, 50 * MB, "改了成分照样从外层扣掉");
    assert!(app(&inv, "Battle.net").parts.iter().all(|p| !p.path.ends_with("Roaming\\Battle.net")));
    let c = inv.tree.iter().find(|n| n.path == "C:").unwrap();
    let me = &c.kids.iter().find(|k| k.name == "Users").unwrap().kids[0];
    let local = &me.kids.iter().find(|k| k.name == "AppData").unwrap().kids.iter().find(|k| k.name == "Local").unwrap().kids;
    assert!(local.iter().all(|k| k.name != "MysteryVendor"), "指定了归属就不在待整理里");
}

#[test]
fn claiming_an_items_own_folder_merges_the_whole_item() {
    // 软件改名重装：旧目录 SOLO LEVELING SYSTEM 里没有 exe 了，AppData 里按旧名找到的数据还挂在旧条目上
    let c = vec![
        e("C:\\Users", 2 * GB, N, &[]),
        e("C:\\Users\\me", 2 * GB, N, &[]),
        e("C:\\Users\\me\\AppData", GB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local", GB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\solevup", 300 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\solo-leveling-system", 150 * MB, N, &[]),
        e("C:\\Users\\me\\AppData\\Local\\leftover", 50 * MB, N, &[]),
    ];
    let d = vec![
        e("D:\\Program Files", GB, 1, &[]),
        e("D:\\Program Files\\Solevup", 240 * MB, 0, &["Solevup.exe"]),
        e("D:\\Program Files\\SOLO LEVELING SYSTEM", 40 * MB, N, &[]),
        e("D:\\Program Files\\SOLO LEVELING SYSTEM\\E", 40 * MB, N, &[]),
    ];
    let digests = vec![
        DriveDigest { version: 1, root: "C:".into(), scanned_at: 1, total: 10 * GB, modified_at: 1, entries: c },
        DriveDigest { version: 1, root: "D:".into(), scanned_at: 1, total: 10 * GB, modified_at: 1, entries: d },
    ];
    let before = build(&digests, &[], &env(), &[]);
    let old = app(&before, "SOLO LEVELING SYSTEM");
    part(old, "C:\\Users\\me\\AppData\\Local\\solo-leveling-system");
    let (solevup, old_key) = (app(&before, "Solevup").key.clone(), old.key.clone());

    let ov = |path: &str, key: &str| Override {
        path: path.into(),
        app_key: Some(key.into()),
        kind: Kind::Data,
        origin: "ai".into(),
        reason: "旧版留下的".into(),
    };
    let inv = build(&digests, &[], &env(), &[
        // 记录里的大小写和磁盘上不一样
        ov("D:\\Program Files\\solo leveling system", &solevup),
        // 之后还有指向旧条目的记录：跟着并过去
        ov("C:\\Users\\me\\AppData\\Local\\leftover", &old_key),
    ]);
    assert!(inv.apps.iter().all(|a| a.name != "SOLO LEVELING SYSTEM"), "旧条目整个并进去，不再单列");
    let s = app(&inv, "Solevup");
    let folder = part(s, "D:\\Program Files\\SOLO LEVELING SYSTEM");
    assert_eq!((folder.kind, folder.origin.as_deref()), (Kind::Data, Some("ai")));
    assert_eq!(part(s, "C:\\Users\\me\\AppData\\Local\\solo-leveling-system").kind, Kind::Data);
    part(s, "C:\\Users\\me\\AppData\\Local\\leftover");
    assert!(!s.uninstalled);
}

#[test]
fn unclaimed_tree_keeps_only_unknown_space() {
    let inv = run();
    let d = inv.tree.iter().find(|n| n.path == "D:").unwrap();
    let names: Vec<&str> = d.kids.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, vec!["my files", "Games", "xwechat_files", "Photos"], "按剩余大小降序，已归属的不出现");
    let my = &d.kids[0];
    assert_eq!(my.rest, 100 * GB);
    assert_eq!(my.kids.len(), 1, "小于 200MB 的不列");
    assert_eq!(my.kids[0].name, "game");
    assert_eq!(d.rest, 200 * GB - 50 * GB - 800 * MB - 100 * MB - GB - 2 * GB - 3 * GB - 10 * GB - 400 * MB - 800 * MB);

    let c = inv.tree.iter().find(|n| n.path == "C:").unwrap();
    let users = c.kids.iter().find(|k| k.name == "Users").unwrap();
    let me = &users.kids[0];
    let local = &me.kids.iter().find(|k| k.name == "AppData").unwrap().kids.iter().find(|k| k.name == "Local").unwrap().kids;
    assert!(local.iter().any(|k| k.name == "MysteryVendor"), "认不出的留在待整理");
    assert!(me.kids.iter().any(|k| k.name == "Documents" && k.rest == 4 * GB), "Documents 里认出的部分扣掉");
}

/// 在本机真实磁盘上跑一遍：扫描、生成摘要并存到应用数据目录、识别软件，结果写成 JSON。
/// 手动运行：设置 DISKY_REAL_DRIVES=C:,D: 和 DISKY_REAL_OUT=<输出文件>，加 --ignored。
#[test]
#[ignore]
fn real_machine() {
    use crate::archive::{digest, registry, rules::BOX_SEEDS};
    use crate::scanner::walk::{scan_tree_progressive, ScanProgress};

    let drives = std::env::var("DISKY_REAL_DRIVES").unwrap_or_else(|_| "C:".into());
    let out = std::env::var("DISKY_REAL_OUT").expect("DISKY_REAL_OUT");
    let dir = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.disky.app").join("archive");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    for d in drives.split(',').filter(|d| !d.is_empty()) {
        let t0 = std::time::Instant::now();
        let tree = scan_tree_progressive(d, &ScanProgress::default(), |_| {});
        let dg = DriveDigest::build(&tree, now);
        eprintln!("{d}: {} 条，扫描 {:?}", dg.entries.len(), t0.elapsed());
        digest::save(&dir, &dg).unwrap();
    }
    let digests = digest::load_all(&dir);
    let installed = registry::installed_apps();
    let t0 = std::time::Instant::now();
    let inv = build(&digests, &installed, &Env::current(), &[]);
    eprintln!("登记的软件 {} 个，识别出 {} 项，用时 {:?}", installed.len(), inv.apps.len(), t0.elapsed());
    let boxes: Vec<serde_json::Value> = BOX_SEEDS
        .iter()
        .map(|(id, name, color, category)| serde_json::json!({ "id": id, "name": name, "color": color, "builtin": true, "category": category }))
        .collect();
    let model = serde_json::json!({ "drives": inv.drives, "apps": inv.apps, "tree": inv.tree, "boxes": boxes, "user": [], "ai": [], "checks": [] });
    std::fs::write(&out, serde_json::to_string(&model).unwrap()).unwrap();
}
