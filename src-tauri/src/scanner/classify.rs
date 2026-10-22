//! 「性质」透镜的规则分类器：在扫描树上按位置规则判定每块空间是什么性质、属于哪个软件。
//!
//! - 规则只看路径各段的名字（ASCII 大小写不敏感），与分隔符无关；Windows 与 macOS 的常见位置都覆盖。
//! - 结果互不重叠：一个目录被判定后不再细分。
//! - 没被规则单独认出的部分，作为所在位置的「其余文件」计入，各足迹大小之和等于被遍历到的空间。
//! - 只深入少数已知位置；通用位置里小目录不再往下找，整盘分类是毫秒级。

use serde::Serialize;

use super::{join_path, NodeId, ScanTree, ROOT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// 程序本体：卸载才会释放
    Prog,
    /// 你的数据：用户自己产生的
    Data,
    /// 缓存：可以安全清理
    Cache,
    /// 可重新下载：游戏、依赖包、扩展、编辑器
    Regen,
    /// 安装包与残留：装完就没用了
    Residue,
    /// 系统：不要动
    System,
}

impl Kind {
    /// 和序列化的名字对应（prog、data……）
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "prog" => Kind::Prog,
            "data" => Kind::Data,
            "cache" => Kind::Cache,
            "regen" => Kind::Regen,
            "residue" => Kind::Residue,
            "system" => Kind::System,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Footprint {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub kind: Kind,
    pub owner: String,
    pub is_dir: bool,
    /// true：所在位置里没被规则单独认出的其余文件，path 指向那个位置
    pub remainder: bool,
}

/// 小于该大小的不单独列出，并入所在位置的其余文件
pub const MIN_FOOTPRINT: u64 = 5 * 1024 * 1024;
/// 通用位置里，小于该大小的目录不再往下找
const MIN_DESCEND: u64 = 50 * 1024 * 1024;
const MAX_GENERIC_DEPTH: u8 = 6;
const MAX_VENDOR_DEPTH: u8 = 3;

/// 遍历时所处的位置，决定子项按什么规则判定
#[derive(Debug, Clone)]
enum Zone {
    Root,
    Windows,
    WinUpdate,
    /// 每个子目录是一个软件的程序本体（Program Files、Programs、/Applications）
    Programs,
    ProgramData,
    Users,
    Home,
    AppData,
    AppDataScope { local: bool },
    /// 某厂商在 AppData / Application Support 下的目录：找出其中的缓存，其余算数据
    Vendor { owner: String, depth: u8 },
    /// 文档、图片、视频、桌面等用户文件夹：每个子目录单独列出
    UserFolder { owner: &'static str },
    Downloads,
    Steam,
    /// ~/.vscode、~/.cursor：extensions 可重新下载
    DotDev { owner: String },
    MacLibrary,
    MacCaches,
    /// 每个子目录是一个独立条目（UWP Packages、macOS Containers）
    PerApp { kind: Kind },
    Generic { owner: String, depth: u8 },
}

enum Decision {
    Emit(Kind, String),
    Descend(Zone),
    Aggregate,
}

impl Zone {
    /// 该位置里没被单独认出的部分算什么性质、记在谁名下
    fn remainder(&self) -> Option<(Kind, String)> {
        Some(match self {
            Zone::Root => return None,
            Zone::Windows => (Kind::System, "Windows".into()),
            Zone::WinUpdate => (Kind::System, "Windows 更新".into()),
            Zone::Programs => (Kind::Prog, "程序目录".into()),
            Zone::ProgramData => (Kind::Data, "ProgramData".into()),
            Zone::Users | Zone::Home => (Kind::Data, "用户目录".into()),
            Zone::AppData | Zone::AppDataScope { .. } => (Kind::Data, "AppData".into()),
            // 深层不单独出「其余」，冒泡到厂商 / 通用目录的顶层汇总成一项，避免碎片
            Zone::Vendor { depth, .. } | Zone::Generic { depth, .. } if *depth > 0 => return None,
            Zone::Vendor { owner, .. } | Zone::DotDev { owner } | Zone::Generic { owner, .. } => (Kind::Data, owner.clone()),
            Zone::UserFolder { owner } => (Kind::Data, (*owner).into()),
            Zone::Downloads => (Kind::Data, "下载".into()),
            Zone::Steam => (Kind::Regen, "Steam".into()),
            Zone::MacLibrary => (Kind::Data, "资源库".into()),
            Zone::MacCaches => (Kind::Cache, "缓存".into()),
            Zone::PerApp { kind } => (*kind, "其他".into()),
        })
    }
}

pub(crate) fn is_cache_name(n: &str) -> bool {
    n.contains("cache")
        || n.ends_with(" temp")
        || matches!(n, "temp" | "tmp" | "logs" | "crashpad" | "crashdumps" | "crash reports" | "deriveddata" | "__pycache__")
}

/// 版本号命名的目录（12.1.0.28505、app-1.2.3、v25.8.1）通常是装在用户目录里的程序本体。
/// 不带前缀时至少三段（x.y.z）：Office 的「16.0」这类两段目录里装的是数据和缓存。
fn is_version_name(n: &str) -> bool {
    let (rest, prefixed) = match n.strip_prefix("app-").or_else(|| n.strip_prefix('v')) {
        Some(r) => (r, true),
        None => (n, false),
    };
    let dots = rest.matches('.').count();
    rest.starts_with(|c: char| c.is_ascii_digit())
        && rest.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        && dots >= if prefixed { 1 } else { 2 }
}

fn is_installer(n: &str) -> bool {
    [".exe", ".msi", ".msix", ".appx", ".iso", ".dmg", ".pkg"].iter().any(|ext| n.ends_with(ext))
}

/// 常见厂商 / 软件目录名 → 展示名；认不出就用原名
pub(crate) fn vendor_name(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    let n = n.strip_suffix(".app").unwrap_or(&n);
    let known = match n {
        "tencent" => "腾讯",
        "netease" => "网易",
        "bytedance" => "字节跳动",
        "larkshell" | "feishu" | "lark" => "飞书",
        "baidu" => "百度",
        "kingsoft" => "WPS",
        "jianyingpro" => "剪映",
        "bilibili" => "哔哩哔哩",
        "mihoyo" => "miHoYo",
        "code" | "microsoft vs code" => "VS Code",
        "docker" | "dockerdesktop" | "docker desktop" => "Docker",
        "unity" | "unityhub" => "Unity",
        "epic games" | "epicgameslauncher" => "Epic Games",
        "mozilla" => "Firefox",
        "windowsapps" => "应用商店应用",
        "common files" => "公共组件",
        _ => "",
    };
    if known.is_empty() {
        name.strip_suffix(".app").unwrap_or(name).to_string()
    } else {
        known.to_string()
    }
}

/// 用户文件夹里几个常见软件的数据目录
fn user_folder_owner(n: &str) -> Option<&'static str> {
    Some(match n {
        "wechat files" | "xwechat_files" => "微信",
        "tencent files" => "QQ",
        "wps cloud files" => "WPS",
        _ => return None,
    })
}

fn dev_tool(n: &str) -> Option<(Kind, &'static str)> {
    Some(match n {
        ".cache" => (Kind::Cache, "通用缓存"),
        ".npm" => (Kind::Cache, "npm"),
        ".yarn" => (Kind::Cache, "Yarn"),
        ".pnpm-store" => (Kind::Cache, "pnpm"),
        ".gradle" => (Kind::Regen, "Gradle"),
        ".m2" => (Kind::Regen, "Maven"),
        ".cargo" => (Kind::Regen, "Cargo"),
        ".rustup" => (Kind::Regen, "Rustup"),
        ".nuget" => (Kind::Regen, "NuGet"),
        "go" => (Kind::Regen, "Go"),
        ".conda" | "anaconda3" | "miniconda3" => (Kind::Regen, "Conda"),
        _ => return None,
    })
}

fn decide(zone: &Zone, name: &str, is_dir: bool, size: u64) -> Decision {
    use Decision::{Aggregate, Descend, Emit};
    let n = name.to_ascii_lowercase();
    match zone {
        Zone::Root => match n.as_str() {
            "windows" if is_dir => Descend(Zone::Windows),
            "program files" | "program files (x86)" | "applications" if is_dir => Descend(Zone::Programs),
            "programdata" if is_dir => Descend(Zone::ProgramData),
            "users" if is_dir => Descend(Zone::Users),
            "library" if is_dir => Descend(Zone::MacLibrary),
            "$recycle.bin" | ".trashes" => Emit(Kind::Residue, "回收站".into()),
            "pagefile.sys" | "swapfile.sys" => Emit(Kind::System, "虚拟内存".into()),
            "hiberfil.sys" => Emit(Kind::System, "休眠文件".into()),
            "system volume information" | "recovery" => Emit(Kind::System, "系统还原".into()),
            "$windows.~bt" | "$windows.~ws" | "windows.old" => Emit(Kind::Residue, "系统升级残留".into()),
            "system" | "private" => Emit(Kind::System, "macOS".into()),
            _ if is_dir => Descend(Zone::Generic { owner: name.to_string(), depth: 0 }),
            _ => Aggregate,
        },
        Zone::Windows => match n.as_str() {
            "temp" => Emit(Kind::Cache, "Windows 临时文件".into()),
            "softwaredistribution" => Descend(Zone::WinUpdate),
            _ => Aggregate,
        },
        Zone::WinUpdate => match n.as_str() {
            "download" => Emit(Kind::Cache, "Windows 更新".into()),
            _ => Aggregate,
        },
        Zone::Programs => if is_dir { Emit(Kind::Prog, vendor_name(name)) } else { Aggregate },
        Zone::ProgramData => match n.as_str() {
            "package cache" => Emit(Kind::Cache, "安装程序缓存".into()),
            _ if is_dir => Emit(Kind::Data, vendor_name(name)),
            _ => Aggregate,
        },
        Zone::Users => if is_dir { Descend(Zone::Home) } else { Aggregate },
        Zone::Home => {
            if let Some((kind, owner)) = dev_tool(&n) {
                return Emit(kind, owner.into());
            }
            match n.as_str() {
                "appdata" => Descend(Zone::AppData),
                "documents" => Descend(Zone::UserFolder { owner: "文档" }),
                "pictures" => Descend(Zone::UserFolder { owner: "图片" }),
                "videos" | "movies" => Descend(Zone::UserFolder { owner: "视频" }),
                "music" => Descend(Zone::UserFolder { owner: "音乐" }),
                "desktop" => Descend(Zone::UserFolder { owner: "桌面" }),
                "downloads" => Descend(Zone::Downloads),
                "library" => Descend(Zone::MacLibrary),
                ".trash" => Emit(Kind::Residue, "废纸篓".into()),
                ".vscode" | ".vscode-insiders" => Descend(Zone::DotDev { owner: "VS Code".into() }),
                ".cursor" => Descend(Zone::DotDev { owner: "Cursor".into() }),
                _ if is_dir => Descend(Zone::Generic { owner: name.to_string(), depth: 0 }),
                _ => Aggregate,
            }
        }
        Zone::AppData => match n.as_str() {
            "local" => Descend(Zone::AppDataScope { local: true }),
            "roaming" | "locallow" => Descend(Zone::AppDataScope { local: false }),
            _ => Aggregate,
        },
        Zone::AppDataScope { local } => match n.as_str() {
            "temp" if *local => Emit(Kind::Cache, "临时文件".into()),
            "programs" if *local => Descend(Zone::Programs),
            "packages" if *local => Descend(Zone::PerApp { kind: Kind::Data }),
            "npm-cache" => Emit(Kind::Cache, "npm".into()),
            "pnpm" => Emit(Kind::Regen, "pnpm".into()),
            "nvm" => Emit(Kind::Regen, "Node.js 版本".into()),
            "crashdumps" => Emit(Kind::Cache, "崩溃转储".into()),
            _ if is_dir => Descend(Zone::Vendor { owner: vendor_name(name), depth: 0 }),
            _ => Aggregate,
        },
        Zone::Vendor { owner, depth } => {
            if !is_dir {
                Aggregate
            } else if is_cache_name(&n) {
                Emit(Kind::Cache, owner.clone())
            } else if n == "node_modules" {
                Emit(Kind::Regen, owner.clone())
            } else if is_version_name(&n) {
                Emit(Kind::Prog, owner.clone())
            } else if *depth < MAX_VENDOR_DEPTH && size >= MIN_FOOTPRINT {
                Descend(Zone::Vendor { owner: owner.clone(), depth: depth + 1 })
            } else {
                Aggregate
            }
        }
        Zone::UserFolder { owner } => {
            if is_dir {
                Emit(Kind::Data, user_folder_owner(&n).unwrap_or(*owner).to_string())
            } else {
                Aggregate
            }
        }
        Zone::Downloads => {
            if !is_dir && is_installer(&n) {
                Emit(Kind::Residue, "下载的安装包".into())
            } else if is_dir {
                Emit(Kind::Data, "下载".into())
            } else {
                Aggregate
            }
        }
        Zone::Steam => match n.as_str() {
            "common" => Emit(Kind::Regen, "Steam 游戏".into()),
            "workshop" => Emit(Kind::Regen, "Steam 创意工坊".into()),
            "shadercache" => Emit(Kind::Cache, "Steam".into()),
            "downloading" | "temp" => Emit(Kind::Residue, "Steam".into()),
            _ => Aggregate,
        },
        Zone::DotDev { owner } => match n.as_str() {
            "extensions" => Emit(Kind::Regen, owner.clone()),
            _ => Aggregate,
        },
        Zone::MacLibrary => match n.as_str() {
            "caches" => Descend(Zone::MacCaches),
            "application support" => Descend(Zone::AppDataScope { local: false }),
            "containers" | "group containers" => Descend(Zone::PerApp { kind: Kind::Data }),
            "developer" => Descend(Zone::Generic { owner: "Xcode".into(), depth: 0 }),
            "logs" => Emit(Kind::Cache, "日志".into()),
            _ if is_dir => Emit(Kind::Data, vendor_name(name)),
            _ => Aggregate,
        },
        Zone::MacCaches => if is_dir { Emit(Kind::Cache, vendor_name(name)) } else { Aggregate },
        Zone::PerApp { kind } => if is_dir { Emit(*kind, vendor_name(name)) } else { Aggregate },
        Zone::Generic { owner, depth } => {
            if !is_dir {
                return Aggregate;
            }
            match n.as_str() {
                "node_modules" | ".venv" | "venv" => Emit(Kind::Regen, owner.clone()),
                "steamapps" => Descend(Zone::Steam),
                _ if is_cache_name(&n) => Emit(Kind::Cache, owner.clone()),
                _ if *depth < MAX_GENERIC_DEPTH && size >= MIN_DESCEND => {
                    Descend(Zone::Generic { owner: owner.clone(), depth: depth + 1 })
                }
                _ => Aggregate,
            }
        }
    }
}

/// 递归判定 id 的子项，返回已计入足迹的字节数
fn visit(tree: &ScanTree, id: NodeId, path: &str, zone: &Zone, out: &mut Vec<Footprint>) -> u64 {
    let mut accounted = 0;
    for child in tree.children(id) {
        let node = tree.node(child);
        let name = tree.name(child);
        match decide(zone, name, node.is_dir(), node.size()) {
            Decision::Emit(kind, owner) => {
                if node.size() >= MIN_FOOTPRINT {
                    out.push(Footprint {
                        path: join_path(path, name),
                        name: name.to_string(),
                        size: node.size(),
                        kind,
                        owner,
                        is_dir: node.is_dir(),
                        remainder: false,
                    });
                    accounted += node.size();
                }
            }
            Decision::Descend(sub) => accounted += visit(tree, child, &join_path(path, name), &sub, out),
            Decision::Aggregate => {}
        }
    }
    let rest = tree.node(id).size().saturating_sub(accounted);
    match zone.remainder() {
        Some((kind, owner)) if rest >= MIN_FOOTPRINT => {
            out.push(Footprint { path: path.to_string(), name: "其余文件".into(), size: rest, kind, owner, is_dir: true, remainder: true });
            accounted + rest
        }
        _ => accounted,
    }
}

/// 把整棵扫描树按性质归类，按大小降序返回
pub fn classify(tree: &ScanTree) -> Vec<Footprint> {
    let mut out = Vec::new();
    visit(tree, ROOT, tree.root_path(), &Zone::Root, &mut out);
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

#[cfg(test)]
mod tests;
