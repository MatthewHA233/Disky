//! 把各盘摘要和已安装软件合成档案馆的清单：每个软件散在哪些位置、各是什么性质，
//! 以及没认出归属的目录树（「待整理」）。
//!
//! 顺序很重要，先认得越确定的越先占位：
//! 1. 系统自身（Windows 目录、虚拟内存、回收站、升级残留）
//! 2. 系统登记的已安装软件（安装位置）
//! 3. 各盘 Program Files、用户目录 Programs 里没登记的软件；没有 exe 的是卸载残留
//! 4. 盘根下的绿色软件
//! 5. AppData / ProgramData / 用户目录 / 盘根下的数据目录，按名字归到软件名下
//! 6. 各位置里的缓存子目录
//!
//! 同一块空间只归一个软件；嵌套的位置（软件目录里的缓存）在最后统一从外层扣掉。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::digest::{DigestEntry, DriveDigest, NO_EXE};
use super::registry::InstalledApp;
use super::rules::{self, norm};
use crate::scanner::classify::Kind;
use crate::scanner::{is_strict_ancestor, join_path, path_file_name, SEP};

const MB: u64 = 1024 * 1024;
const MIN_PART: u64 = MB;
/// 太小的软件不单独列出
const MIN_APP: u64 = 10 * MB;
/// 盘根下的目录只有 exe、名字又对不上时，超过这个大小就不当绿色软件（多半是用户自己的资料夹）
const PORTABLE_MAX: u64 = 5 * 1024 * MB;
const MIN_CACHE: u64 = 5 * MB;
/// 别名命中、或名字完全相同才算强匹配（用户目录、盘根下只认这种）
const ALIAS_MATCH: usize = 2000;
const STRONG_MATCH: usize = 900;
/// 待整理目录树只列出不小于该大小的位置
pub const MIN_NODE: u64 = 200 * MB;
const WINDOWS: bool = cfg!(windows);

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    /// 删除命令会不会拒绝（受保护的系统目录、里面还装着别的软件的位置都不能删）
    pub deletable: bool,
    /// 临时文件夹这类只能清空里面的东西、目录本身要留着
    pub contents_only: bool,
    /// 不是内置规则认的：user（用户指定）/ ai（AI 查过之后指定）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// AI 指定时给的依据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Part {
    fn new(path: String, kind: Kind, size: u64) -> Self {
        Part { path, kind, size, deletable: false, contents_only: false, origin: None, reason: None }
    }
}

/// 用户（或 AI 查过之后）指定的归属，重新识别时压过内置规则
#[derive(Debug, Clone)]
pub struct Override {
    pub path: String,
    /// None：不属于任何软件
    pub app_key: Option<String>,
    pub kind: Kind,
    /// user / ai
    pub origin: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppItem {
    /// 稳定标识（装箱结果按它记）
    pub key: String,
    pub name: String,
    pub publisher: String,
    /// 提取图标用的文件
    pub icon: Option<String>,
    /// 程序目录里没有可执行文件，系统里也没有安装记录：多半是卸载后留下的
    pub uninstalled: bool,
    /// 内置规则判断的箱子
    pub hint: Option<String>,
    /// 门类：software（软件）或 devenv（插件与开发环境）；没认出归属的文件夹另算个人文件
    pub category: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderNode {
    pub key: String,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: u8,
    /// 扣掉已归到软件名下的部分后剩下的大小
    pub rest: u64,
    /// 按名字猜的个人文件箱子（只作建议，不自动放）
    pub hint: Option<String>,
    pub kids: Vec<FolderNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriveInfo {
    pub root: String,
    pub scanned_at: i64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inventory {
    pub drives: Vec<DriveInfo>,
    pub apps: Vec<AppItem>,
    pub tree: Vec<FolderNode>,
}

/// 运行环境里影响识别的信息
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// Windows 目录（`C:\Windows`）；别的盘上的 Windows 目录不当系统
    pub system_root: Option<String>,
    /// TEMP / TMP 实际指向的临时文件夹（可能被改到别的盘）
    pub temp_dirs: Vec<String>,
    /// 工具链环境变量（CARGO_HOME 等）：(变量名, 路径)
    pub toolchain_vars: Vec<(String, String)>,
}

impl Env {
    pub fn current() -> Self {
        let mut temp_dirs: Vec<String> = ["TEMP", "TMP"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|s| crate::scanner::normalize_path(&s))
            .collect();
        temp_dirs.sort_by_key(|s| s.to_ascii_lowercase());
        temp_dirs.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let toolchain_vars = rules::ENV_TOOLCHAINS
            .iter()
            .filter_map(|(var, ..)| std::env::var(var).ok().map(|v| (var.to_string(), crate::scanner::normalize_path(&v))))
            .filter(|(_, v)| !v.is_empty())
            .collect();
        Env {
            system_root: std::env::var("SystemRoot").ok().map(|s| crate::scanner::normalize_path(&s)),
            temp_dirs,
            toolchain_vars,
        }
    }
}

pub fn build(digests: &[DriveDigest], installed: &[InstalledApp], env: &Env, overrides: &[Override]) -> Inventory {
    let mut b = Builder::new(digests);
    b.system_root = env.system_root.clone();
    b.temp_dirs = env.temp_dirs.iter().map(|t| lower(t)).collect();
    b.system(env);
    b.registered(installed);
    b.program_roots();
    b.store_packages();
    b.portable_apps(env);
    b.merge_same_name();
    b.devenv_apps();
    b.dev_envs(env);
    b.data_dirs(env);
    b.apply_overrides(overrides);
    b.caches();
    b.mark_uninstalled();
    b.finish(digests)
}

// ---------------------------------------------------------------------------
// 摘要索引
// ---------------------------------------------------------------------------

fn lower(p: &str) -> String {
    p.to_ascii_lowercase()
}

/// 上一级路径；Unix 根下的项返回 "/"
fn parent_of(path: &str) -> Option<&str> {
    let (head, _) = path.rsplit_once(SEP)?;
    Some(if head.is_empty() { &path[..1] } else { head })
}

fn base(path: &str) -> String {
    path_file_name(path)
}

struct Index<'a> {
    by_path: HashMap<String, &'a DigestEntry>,
    kids: HashMap<String, Vec<&'a DigestEntry>>,
}

impl<'a> Index<'a> {
    fn new(digests: &'a [DriveDigest]) -> Self {
        let mut by_path = HashMap::new();
        let mut kids: HashMap<String, Vec<&DigestEntry>> = HashMap::new();
        for e in digests.iter().flat_map(|d| d.entries.iter()) {
            by_path.insert(lower(&e.path), e);
            if let Some(p) = parent_of(&e.path) {
                kids.entry(lower(p)).or_default().push(e);
            }
        }
        for list in kids.values_mut() {
            list.sort_by(|a, b| b.size.cmp(&a.size));
        }
        Index { by_path, kids }
    }

    fn get(&self, path: &str) -> Option<&'a DigestEntry> {
        self.by_path.get(&lower(path)).copied()
    }

    fn dir(&self, path: &str) -> Option<&'a DigestEntry> {
        self.get(path).filter(|e| e.is_dir)
    }

    fn children(&self, path: &str) -> Vec<&'a DigestEntry> {
        self.kids.get(&lower(path)).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// 构建
// ---------------------------------------------------------------------------

struct Draft {
    key: String,
    name: String,
    publisher: String,
    icon: Option<String>,
    registered: bool,
    /// 内置的系统 / 工具链项：用途固定，也不做卸载判断
    fixed_purpose: Option<&'static str>,
    keys: Vec<String>,
    parts: Vec<Part>,
    uninstalled: bool,
    category: &'static str,
    /// 按名字认出属于插件与开发环境时的箱子
    hint_box: Option<&'static str>,
}

impl Draft {
    fn new(key: String, name: String) -> Self {
        Draft {
            key,
            name,
            publisher: String::new(),
            icon: None,
            registered: false,
            fixed_purpose: None,
            keys: Vec::new(),
            parts: Vec::new(),
            uninstalled: false,
            category: rules::SOFTWARE,
            hint_box: None,
        }
    }

    fn add_key(&mut self, raw: &str) {
        let k = norm(raw);
        if !rules::is_generic_key(&k) && !self.keys.contains(&k) {
            self.keys.push(k);
        }
    }
}

struct Builder<'a> {
    idx: Index<'a>,
    roots: Vec<String>,
    homes: Vec<String>,
    apps: Vec<Draft>,
    /// 已归属的位置（小写）→ 软件下标
    claimed: HashMap<String, usize>,
    /// 系统登记过的软件名（归一化），判断「已卸载」前要对一遍
    installed_names: Vec<String>,
    /// 从用户目录、文档、盘根按名字认出的位置：里面多半是用户自己的东西，不在里面找缓存
    no_cache: HashSet<String>,
    /// 用户 / AI 指定过归属的位置（小写）：找缓存时不再按名字改认
    pinned: HashSet<String>,
    system_root: Option<String>,
    /// 小写
    temp_dirs: Vec<String>,
}

impl<'a> Builder<'a> {
    fn new(digests: &'a [DriveDigest]) -> Self {
        let idx = Index::new(digests);
        let roots: Vec<String> = digests.iter().map(|d| d.root.clone()).collect();
        let skip_users = ["public", "default", "default user", "all users", "shared", "guest"];
        let homes = roots
            .iter()
            .flat_map(|r| idx.children(&join_path(r, "Users")))
            .filter(|e| e.is_dir && !skip_users.contains(&base(&e.path).to_ascii_lowercase().as_str()))
            .map(|e| e.path.clone())
            .collect();
        Builder {
            idx,
            roots,
            homes,
            apps: Vec::new(),
            claimed: HashMap::new(),
            installed_names: Vec::new(),
            no_cache: HashSet::new(),
            pinned: HashSet::new(),
            system_root: None,
            temp_dirs: Vec::new(),
        }
    }

    /// 某位置自身或它的上级是否已归属
    fn owner_of(&self, path: &str) -> Option<usize> {
        let mut cur = Some(path);
        while let Some(p) = cur {
            if let Some(&i) = self.claimed.get(&lower(p)) {
                return Some(i);
            }
            let up = parent_of(p);
            cur = if up == Some(p) { None } else { up };
        }
        None
    }

    fn is_claimed(&self, path: &str) -> bool {
        self.owner_of(path).is_some()
    }

    /// 把一个位置归给软件；已归属或太小的忽略
    fn claim(&mut self, app: usize, path: &str, kind: Kind) -> bool {
        let Some(e) = self.idx.get(path) else { return false };
        if e.size < MIN_PART || self.is_claimed(path) {
            return false;
        }
        self.claimed.insert(lower(path), app);
        self.apps[app].parts.push(Part::new(e.path.clone(), kind, e.size));
        true
    }

    fn push(&mut self, draft: Draft) -> usize {
        self.apps.push(draft);
        self.apps.len() - 1
    }

    /// 内置项（系统、工具链）：同 id 复用
    fn fixed(&mut self, id: &str, name: &str, purpose: &'static str) -> usize {
        let key = format!("app:rule:{id}");
        if let Some(i) = self.apps.iter().position(|a| a.key == key) {
            return i;
        }
        let mut d = Draft::new(key, name.to_string());
        d.fixed_purpose = Some(purpose);
        d.category = rules::category_of_box(purpose);
        d.add_key(name);
        self.push(d)
    }

    /// 嵌套归属：上级已经归给别的软件也照样记（nvm 下的各个 Node 版本、.cursor 里的扩展），
    /// 收尾时从外层扣掉。只拒绝重复的同一位置
    fn claim_inside(&mut self, app: usize, path: &str, kind: Kind) -> bool {
        let Some(e) = self.idx.get(path) else { return false };
        if e.size < MIN_PART || self.claimed.contains_key(&lower(path)) {
            return false;
        }
        self.claimed.insert(lower(path), app);
        self.apps[app].parts.push(Part::new(e.path.clone(), kind, e.size));
        true
    }

    /// 开发环境里按位置拆出来的一项（某个 Node 版本、某个 npm 全局包）：同 key 复用
    fn dev_item(&mut self, key: String, name: &str, box_id: &'static str, publisher: &str) -> usize {
        if let Some(i) = self.apps.iter().position(|a| a.key == key) {
            return i;
        }
        let mut d = Draft::new(key, name.to_string());
        d.fixed_purpose = Some(box_id);
        d.category = rules::DEVENV;
        d.publisher = publisher.to_string();
        d.add_key(name);
        self.push(d)
    }

    fn is_program_root(&self, path: &str) -> bool {
        let l = lower(path);
        self.program_roots_list().iter().any(|r| lower(r) == l)
    }

    fn program_roots_list(&self) -> Vec<String> {
        let mut out = Vec::new();
        if WINDOWS {
            for r in &self.roots {
                out.push(join_path(r, "Program Files"));
                out.push(join_path(r, "Program Files (x86)"));
            }
            for h in &self.homes {
                out.push(join_path(&join_path(&join_path(h, "AppData"), "Local"), "Programs"));
            }
        } else {
            for r in &self.roots {
                out.push(join_path(r, "Applications"));
            }
            for h in &self.homes {
                out.push(join_path(h, "Applications"));
            }
        }
        out
    }

    // 1. 系统自身 -------------------------------------------------------------

    fn system(&mut self, env: &Env) {
        if !WINDOWS {
            return;
        }
        if let Some(sys) = env.system_root.as_deref().filter(|s| self.idx.dir(s).is_some()) {
            let i = self.fixed("windows", "Windows", "system");
            self.claim(i, sys, Kind::Prog);
        }
        // TEMP 改到别处（D:\tmp）时那里才是真正在用的临时文件夹
        for t in env.temp_dirs.clone() {
            let sane = self.idx.dir(&t).is_some_and(|e| e.depth >= 1)
                && !self.homes.iter().any(|h| h.eq_ignore_ascii_case(&t))
                && !self.is_program_root(&t);
            if sane {
                let i = self.fixed("temp", "临时文件", "system");
                self.claim(i, &t, Kind::Cache);
            }
        }
        let roots = self.roots.clone();
        for r in &roots {
            for e in self.idx.children(r) {
                let name = base(&e.path).to_ascii_lowercase();
                let (id, label, kind) = match name.as_str() {
                    "pagefile.sys" | "hiberfil.sys" | "swapfile.sys" => ("swap", "虚拟内存与休眠文件", Kind::Prog),
                    "$recycle.bin" => ("recycle", "回收站", Kind::Residue),
                    "windows.old" | "$windows.~bt" | "$windows.~ws" | "$windows.~q" => ("upgrade", "系统升级残留", Kind::Residue),
                    "system volume information" | "recovery" | "$winreagent" => ("windows", "Windows", Kind::Prog),
                    // 别的盘上的整套系统（双系统、换盘前的旧系统）：认出来但不当残留，删不删用户自己判断
                    "windows" if !self.is_claimed(&e.path) && self.idx.dir(&join_path(&e.path, "System32")).is_some() => {
                        ("windows-other", "另一个 Windows 系统", Kind::Prog)
                    }
                    _ => continue,
                };
                let i = self.fixed(id, label, "system");
                self.claim(i, &e.path, kind);
            }
            let pkg = join_path(&join_path(r, "ProgramData"), "Package Cache");
            if self.idx.dir(&pkg).is_some() {
                let i = self.fixed("installer-cache", "安装程序缓存", "system");
                self.claim(i, &pkg, Kind::Data);
            }
        }
    }

    // 2. 已安装软件 -----------------------------------------------------------

    fn registered(&mut self, installed: &[InstalledApp]) {
        self.installed_names = installed.iter().map(|a| norm(&rules::clean_name(&a.name))).filter(|n| !n.is_empty()).collect();
        // 外层位置优先，嵌套的登记项（同一软件的组件）并进外层
        let mut by_loc: HashMap<String, (String, Vec<&InstalledApp>)> = HashMap::new();
        for a in installed {
            if let Some(loc) = self.locate(a) {
                by_loc.entry(lower(&loc)).or_insert_with(|| (loc, Vec::new())).1.push(a);
            }
        }
        // 同一目录登记了好几条（启动器和它装的游戏）：名字取跟目录名对得上的、图标就是目录里主程序的那条；
        // 其余几条的名字留着认数据（Local\miHoYo\原神）
        let mut list: Vec<(&InstalledApp, String, Vec<&str>)> = by_loc
            .into_values()
            .map(|(loc, apps)| {
                let dir = norm(&base(&loc));
                let main_exe = |a: &InstalledApp| {
                    a.icon.as_deref().is_some_and(|i| i.to_ascii_lowercase().ends_with(".exe") && parent_of(i).is_some_and(|p| p.eq_ignore_ascii_case(&loc)))
                };
                let best = apps
                    .iter()
                    .max_by_key(|a| {
                        let n = norm(&rules::clean_name(&a.name));
                        let named = rules::match_score(&n, &dir).max(rules::match_score(&rules::compact(&n), &rules::compact(&dir)));
                        (named > 0, main_exe(a), std::cmp::Reverse(a.name.chars().count()))
                    })
                    .copied()
                    .expect("至少一条");
                let names = apps.iter().map(|a| a.name.as_str()).collect();
                (best, loc, names)
            })
            .collect();
        // 登记在厂商目录上的（Autodesk 把 InstallLocation 写成整个 Autodesk 目录）：
        // 里面还有两个以上别的登记软件，就不把整个目录给它
        let named: Vec<(String, String)> = list.iter().map(|(a, l, _)| (lower(l), norm(&rules::clean_name(&a.name)))).collect();
        list.retain(|(a, l, _)| {
            let me = norm(&rules::clean_name(&a.name));
            let ll = lower(l);
            let others: HashSet<&String> = named.iter().filter(|(o, n)| is_strict_ancestor(&ll, o) && *n != me).map(|(_, n)| n).collect();
            others.len() < 2
        });
        list.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.1.cmp(&b.1)));
        for (app, loc, names) in list {
            if self.is_claimed(&loc) {
                continue;
            }
            let mut d = Draft::new(format!("app:{}", lower(&loc)), rules::clean_name(&app.name));
            d.publisher = app.publisher.clone();
            d.registered = true;
            d.icon = app.icon.clone().filter(|p| std::path::Path::new(p).exists());
            d.add_key(&d.name.clone());
            for n in names {
                d.add_key(n);
                d.add_key(&rules::clean_name(n));
            }
            self.add_dir_keys(&mut d, &loc);
            if d.icon.is_none() {
                d.icon = self.best_exe(&loc, &d.keys);
            }
            if let Some(stem) = d.icon.as_deref().map(exe_stem) {
                d.add_key(&stem);
            }
            let i = self.push(d);
            self.claim(i, &loc, Kind::Prog);
        }
    }

    /// 登记项的安装目录：优先 InstallLocation，没有就从图标位置推回程序目录。
    /// 登记的位置是厂商目录（装着好几个软件）时，收窄到图标所在的那个软件目录。
    fn locate(&self, app: &InstalledApp) -> Option<String> {
        let from_icon = || {
            let icon = app.icon.as_deref()?;
            let mut cur = parent_of(icon)?;
            let mut inner: Option<&str> = None;
            // 退到程序根目录下第一层；装在用户目录里的（AppData\Local\Kingsoft\WPS Office）退到 AppData 下第一层，
            // 那层是装着好几家软件的厂商目录就再往里一层
            while let Some(up) = parent_of(cur) {
                if self.is_program_root(up) {
                    return Some(cur.to_string());
                }
                if self.is_appdata_dir(up) {
                    return if rules::is_vendor_folder(&base(cur)) { inner.map(str::to_string) } else { Some(cur.to_string()) };
                }
                if up == cur {
                    break;
                }
                inner = Some(cur);
                cur = up;
            }
            None
        };
        let loc = app.location.clone().or_else(from_icon)?;
        let e = self.idx.dir(&loc)?;
        let bogus = self.roots.iter().any(|r| r.eq_ignore_ascii_case(&loc))
            || self.homes.iter().any(|h| h.eq_ignore_ascii_case(&loc))
            || self.is_program_root(&loc)
            || self.is_container(&loc)
            || e.depth < 2;
        if bogus {
            return None;
        }
        if let Some(icon) = app.icon.as_deref().filter(|i| is_strict_ancestor(&loc, i)) {
            let apps_inside = self
                .idx
                .children(&loc)
                .iter()
                .filter(|k| k.is_dir && k.exe_depth <= 2 && !rules::is_generic_subdir(&base(&k.path)))
                .count();
            if apps_inside >= 2 {
                if let Some(sub) = self.idx.children(&loc).into_iter().find(|k| k.is_dir && is_strict_ancestor(&k.path, icon)) {
                    return Some(sub.path.clone());
                }
            }
        }
        Some(e.path.clone())
    }

    /// 用户目录里的 AppData\Local、AppData\Roaming
    fn is_appdata_dir(&self, path: &str) -> bool {
        WINDOWS
            && self.homes.iter().any(|h| {
                let appdata = join_path(h, "AppData");
                ["Local", "Roaming"].iter().any(|s| join_path(&appdata, s).eq_ignore_ascii_case(path))
            })
    }

    /// AppData、文档、ProgramData 这类装着很多软件数据的目录：登记成安装位置也不采用
    fn is_container(&self, path: &str) -> bool {
        let l = lower(path);
        let mut set: Vec<String> = Vec::new();
        for h in &self.homes {
            let appdata = join_path(h, "AppData");
            for sub in ["Local", "Roaming", "LocalLow"] {
                set.push(lower(&join_path(&appdata, sub)));
            }
            set.push(lower(&appdata));
            set.push(lower(&join_path(h, "Documents")));
            set.push(lower(&join_path(&join_path(&join_path(h, "AppData"), "Local"), "Programs")));
        }
        for r in &self.roots {
            let pd = join_path(r, "ProgramData");
            set.push(lower(&join_path(&pd, "Microsoft")));
            set.push(lower(&pd));
        }
        set.contains(&l) || base(path).eq_ignore_ascii_case("Common Files")
    }

    /// 目录名作为软件的名字之一；`Application`、`bin` 这类泛名改用上一级
    fn add_dir_keys(&self, d: &mut Draft, path: &str) {
        let name = base(path);
        if rules::is_generic_key(&norm(&name)) || norm(&name) == "application" {
            if let Some(up) = parent_of(path) {
                d.add_key(&base(up));
            }
        } else {
            d.add_key(&name);
            d.add_key(&rules::clean_name(&name));
        }
    }

    /// 在软件目录里挑一个主程序当图标来源：名字像软件名的、浅层的优先，卸载 / 更新程序靠后
    fn best_exe(&self, dir: &str, keys: &[String]) -> Option<String> {
        let top = self.idx.dir(dir)?;
        let mut best: Option<(i64, String)> = None;
        let mut stack = vec![top];
        while let Some(e) = stack.pop() {
            let rel = i64::from(e.depth.saturating_sub(top.depth));
            for (rank, exe) in e.exes.iter().enumerate() {
                let stem = norm(&exe_stem(exe));
                let named = keys.iter().any(|k| rules::match_score(&stem, k) > 0 || rules::match_score(&rules::compact(&stem), &rules::compact(k)) > 0);
                let score = if named { 100 } else { 0 } - if is_helper_exe(&stem) { 60 } else { 0 } - rel * 4 - rank as i64;
                if best.as_ref().map_or(true, |(s, _)| score > *s) {
                    best = Some((score, join_path(&e.path, exe)));
                }
            }
            if rel < 2 {
                stack.extend(self.idx.children(&e.path).into_iter().filter(|k| k.is_dir && k.exe_depth != NO_EXE));
            }
        }
        best.filter(|(s, _)| *s > -40).map(|(_, p)| p)
    }

    // 3. 程序目录里没登记的软件 -------------------------------------------------

    fn program_roots(&mut self) {
        for root in self.program_roots_list() {
            self.scan_program_root(&root, true);
        }
    }

    /// `real`：系统的程序目录（Program Files 等）。盘根下的软件容器（D:\Games）里放着 ROM、存档、
    /// 模组这类没有 exe 的资料，只认有 exe 的子目录
    fn scan_program_root(&mut self, root: &str, real: bool) {
        for c in self.idx.children(root) {
            if !c.is_dir || self.is_claimed(&c.path) || (!real && c.exe_depth == NO_EXE) {
                continue;
            }
            let name = base(&c.path);
            if name.eq_ignore_ascii_case("WindowsApps") {
                let i = self.fixed("store", "应用商店应用", "system");
                self.claim(i, &c.path, Kind::Prog);
                continue;
            }
            if WINDOWS && rules::is_system_program_dir(&name) {
                let i = self.fixed("windows-components", "Windows 组件", "system");
                self.claim(i, &c.path, Kind::Prog);
                continue;
            }
            if rules::is_generic_subdir(&name) || rules::is_generic_key(&norm(&name)) {
                let i = self.fixed("shared-components", "公共组件", "system");
                self.claim(i, &c.path, Kind::Prog);
                continue;
            }
            if let Some(label) = rules::grouped_vendor(&name) {
                let i = self.fixed(&norm(label), label, "system");
                self.apps[i].add_key(&name);
                self.claim(i, &c.path, Kind::Prog);
                continue;
            }
            let kids: Vec<&DigestEntry> = self.idx.children(&c.path).into_iter().filter(|k| k.is_dir).collect();
            let holds_claimed = kids.iter().any(|k| self.is_claimed(&k.path)) || self.claimed.keys().any(|k| is_strict_ancestor(&lower(&c.path), k));
            // 厂商目录：自己下面没有 exe，几个像软件名的子目录里各有 exe（Tencent\QQNT、Tencent\WeMeet）
            let apps_inside = kids.iter().filter(|k| k.exe_depth <= 2 && !rules::is_generic_subdir(&base(&k.path))).count();
            let vendor = holds_claimed || (WINDOWS && c.exe_depth >= 1 && apps_inside >= 2);
            if vendor {
                let mut kids = kids;
                kids.sort_by_key(|k| base(&k.path).chars().count());
                for k in kids {
                    if self.is_claimed(&k.path) || (!real && k.exe_depth == NO_EXE) {
                        continue;
                    }
                    let inside = self.owners_inside(&k.path);
                    if inside.len() == 1 {
                        // 登记的位置在它里面（Edge\Application）：整个目录归那个软件
                        let i = inside[0];
                        let mut tmp = Draft::new(String::new(), String::new());
                        tmp.keys = std::mem::take(&mut self.apps[i].keys);
                        self.add_dir_keys(&mut tmp, &k.path);
                        self.apps[i].keys = tmp.keys;
                        // 里面原先登记的位置并进外层，不再单列
                        self.apps[i].parts.retain(|p| !is_strict_ancestor(&k.path, &p.path));
                        self.claimed.retain(|p, _| !is_strict_ancestor(&k.path, p));
                        self.claim(i, &k.path, Kind::Prog);
                    } else if !inside.is_empty() {
                        continue;
                    } else if let Some(i) = self.loose_owner(&base(&k.path)) {
                        // 同厂商目录里跟已认出软件同名开头的组件并过去（Edge → EdgeUpdate、EdgeWebView）
                        self.claim(i, &k.path, Kind::Prog);
                    } else {
                        self.unregistered(&k.path, Some(name.as_str()));
                    }
                }
            } else {
                self.unregistered(&c.path, None);
            }
        }
    }

    /// 某目录里面（不含自身）已归属位置的主人
    fn owners_inside(&self, path: &str) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .claimed
            .iter()
            .filter(|(p, _)| is_strict_ancestor(path, p))
            .map(|(_, &i)| i)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// 厂商目录里的组件：紧凑名以某个已认出软件的名字开头
    fn loose_owner(&self, name: &str) -> Option<usize> {
        let n = rules::compact(&norm(name));
        let hits: Vec<usize> = (0..self.apps.len())
            .filter(|&i| {
                self.apps[i].fixed_purpose.is_none()
                    && self.apps[i].keys.iter().any(|k| {
                        let k = rules::compact(k);
                        k.chars().count() >= 4 && n.starts_with(&k)
                    })
            })
            .collect();
        (hits.len() == 1).then(|| hits[0])
    }

    fn unregistered(&mut self, path: &str, vendor: Option<&str>) {
        let Some(e) = self.idx.dir(path) else { return };
        if e.size < MIN_PART {
            return;
        }
        let dir_name = base(path);
        let known = rules::display_name(&dir_name);
        let mut name = known.map(str::to_string).unwrap_or_else(|| rules::clean_name(&dir_name));
        if let (Some(v), None) = (vendor, known) {
            if rules::needs_vendor_prefix(&name) {
                let vendor_label = rules::display_name(v).map(str::to_string).unwrap_or_else(|| v.to_string());
                name = format!("{vendor_label} {name}");
            }
        }
        let mut d = Draft::new(format!("app:{}", lower(path)), name);
        d.publisher = vendor.map(str::to_string).unwrap_or_default();
        d.add_key(&d.name.clone());
        self.add_dir_keys(&mut d, path);
        d.icon = self.best_exe(path, &d.keys);
        if let Some(stem) = d.icon.as_deref().map(exe_stem) {
            d.add_key(&stem);
        }
        let i = self.push(d);
        self.claim(i, path, Kind::Prog);
    }

    // 4. 盘根下的绿色软件 -------------------------------------------------------

    fn portable_apps(&mut self, env: &Env) {
        if !WINDOWS {
            return;
        }
        let roots = self.roots.clone();
        for r in &roots {
            for c in self.idx.children(r) {
                if !c.is_dir || self.is_claimed(&c.path) || self.is_program_root(&c.path) {
                    continue;
                }
                let name = base(&c.path);
                if name.starts_with('$') || rules::is_not_app_root(&name) || is_system_dir(&c.path, env) {
                    continue;
                }
                if rules::is_software_container(&name) {
                    self.scan_program_root(&c.path, false);
                } else if c.exe_depth <= 1 && (self.exe_named_like(&c.path) || (c.exe_depth == 0 && c.size < PORTABLE_MAX)) {
                    self.unregistered(&c.path, None);
                }
            }
        }
    }

    /// 目录里（含下一层）有没有名字像目录名的 exe：「D:\ffmpeg\bin\ffmpeg.exe」是，「D:\my files\setup.exe」不是
    fn exe_named_like(&self, path: &str) -> bool {
        let mut d = Draft::new(String::new(), String::new());
        self.add_dir_keys(&mut d, path);
        let Some(top) = self.idx.dir(path) else { return false };
        std::iter::once(top)
            .chain(self.idx.children(path).into_iter().filter(|k| k.is_dir))
            .flat_map(|e| e.exes.iter())
            .any(|exe| {
                let stem = norm(&exe_stem(exe));
                d.keys.iter().any(|k| rules::match_score(&stem, k) > 0 || rules::compact(&stem).starts_with(&rules::compact(k)))
            })
    }

    // 合并 --------------------------------------------------------------------

    /// 同名软件（两块盘各装了一部分、两条登记项）合成一个
    fn merge_same_name(&mut self) {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut merged_into: Vec<Option<usize>> = vec![None; self.apps.len()];
        for i in 0..self.apps.len() {
            if self.apps[i].fixed_purpose.is_some() {
                continue;
            }
            let k = norm(&self.apps[i].name);
            match seen.get(&k) {
                Some(&j) => merged_into[i] = Some(j),
                None => {
                    seen.insert(k, i);
                }
            }
        }
        if merged_into.iter().all(Option::is_none) {
            return;
        }
        let old = std::mem::take(&mut self.apps);
        let mut remap = vec![0usize; old.len()];
        let mut kept: Vec<Draft> = Vec::new();
        let mut pending: Vec<(usize, Draft)> = Vec::new();
        for (i, d) in old.into_iter().enumerate() {
            match merged_into[i] {
                None => {
                    remap[i] = kept.len();
                    kept.push(d);
                }
                Some(j) => pending.push((j, d)),
            }
        }
        for (j, d) in pending {
            let target = &mut kept[remap[j]];
            target.parts.extend(d.parts);
            target.registered |= d.registered;
            if target.icon.is_none() {
                target.icon = d.icon;
            }
            for k in d.keys {
                if !target.keys.contains(&k) {
                    target.keys.push(k);
                }
            }
        }
        self.apps = kept;
        self.claimed.clear();
        for (i, a) in self.apps.iter().enumerate() {
            for p in &a.parts {
                self.claimed.insert(lower(&p.path), i);
            }
        }
    }

    // 4½. 插件与开发环境 -------------------------------------------------------

    /// 装在系统里的运行时、SDK、命令行工具（Python、Git、Go）归到插件与开发环境
    fn devenv_apps(&mut self) {
        for a in self.apps.iter_mut() {
            if a.fixed_purpose.is_some() {
                continue;
            }
            if let Some(b) = rules::devenv_box(&a.keys) {
                a.category = rules::DEVENV;
                a.hint_box = Some(b);
                if let Some(pretty) = rules::runtime_display(&a.name) {
                    a.name = pretty;
                }
            }
        }
    }

    /// 没有安装记录、但一个个都是独立工具的东西：每个 Node 版本、每个 npm 全局包、
    /// 每个编辑器的扩展、Claude Code 原生安装、环境变量指过去的工具链、D:\dev\LLVM 这类目录
    fn dev_envs(&mut self, env: &Env) {
        for (var, path) in &env.toolchain_vars {
            let Some((_, id, name, kind, bx)) = rules::ENV_TOOLCHAINS.iter().find(|t| t.0.eq_ignore_ascii_case(var)) else {
                continue;
            };
            let sane = self.idx.dir(path).is_some_and(|e| e.depth >= 1)
                && !self.homes.iter().any(|h| h.eq_ignore_ascii_case(path))
                && !self.is_program_root(path)
                && !self.is_container(path);
            if sane {
                let i = self.fixed(id, name, *bx);
                self.claim_inside(i, path, *kind);
            }
        }
        for h in self.homes.clone() {
            let local = join_path(&join_path(&h, "AppData"), "Local");
            let roaming = join_path(&join_path(&h, "AppData"), "Roaming");
            for (dot, editor) in rules::EDITOR_DIRS {
                let ext = join_path(&join_path(&h, dot), "extensions");
                if self.idx.dir(&ext).is_some() {
                    let i = self.fixed(&format!("ext-{}", norm(editor)), &format!("{editor} 扩展"), "env-ext");
                    self.claim_inside(i, &ext, Kind::Regen);
                }
            }
            let native = join_path(&join_path(&join_path(&h, ".local"), "share"), "claude");
            if self.idx.dir(&native).is_some() {
                let i = self.fixed("claude-code", "Claude Code", "env-cli");
                self.claim_inside(i, &native, Kind::Regen);
            }
            let runtimes = join_path(&join_path(&h, ".cache"), "codex-runtimes");
            if self.idx.dir(&runtimes).is_some() {
                let i = self.fixed("codex", "Codex CLI", "env-cli");
                self.claim_inside(i, &runtimes, Kind::Regen);
            }
            self.npm_globals(&join_path(&join_path(&roaming, "npm"), "node_modules"));
            self.node_versions(&join_path(&local, "nvm"));
            for conda in [join_path(&local, "conda"), join_path(&h, ".conda"), join_path(&h, "anaconda3"), join_path(&h, "miniconda3")] {
                for e in self.idx.children(&join_path(&conda, "envs")) {
                    if e.is_dir && e.size >= MIN_APP {
                        let name = format!("Conda 环境 · {}", base(&e.path));
                        let i = self.dev_item(format!("app:dev:{}", lower(&e.path)), &name, "env-python", "Conda");
                        self.claim_inside(i, &e.path, Kind::Regen);
                    }
                }
            }
        }
        for (var, path) in &env.toolchain_vars {
            if var.eq_ignore_ascii_case("NVM_HOME") {
                self.node_versions(path);
            }
        }
        // 程序目录里的 Python 容器（Programs\Python\Python311）：每个版本一项
        for root in self.program_roots_list() {
            for e in self.idx.children(&join_path(&root, "Python")) {
                if let Some(name) = rules::runtime_display(&base(&e.path)).filter(|_| e.is_dir && e.size >= MIN_APP) {
                    let i = self.dev_item(format!("app:dev:{}", lower(&e.path)), &name, "env-python", "Python");
                    self.claim_inside(i, &e.path, Kind::Prog);
                }
            }
        }
        // 放在哪都认的开发工具目录（D:\dev\LLVM、D:\dev\vcpkg、D:\Java\jdk-21）；外层先认，msys64 里的 mingw64 就不会被单拆
        let mut shallow: Vec<&DigestEntry> = self.idx.by_path.values().copied().filter(|e| e.is_dir && (1..=3).contains(&e.depth)).collect();
        shallow.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
        for e in shallow {
            if self.is_claimed(&e.path) || self.is_program_root(&e.path) {
                continue;
            }
            let dir = base(&e.path);
            let lname = dir.to_ascii_lowercase();
            let has_kid = |k: &str| self.idx.children(&e.path).iter().any(|c| base(&c.path).eq_ignore_ascii_case(k));
            if let Some((_, id, name, kind, bx, marker)) = rules::DEV_FOLDERS.iter().find(|t| t.0 == lname) {
                if marker.is_none_or(|m| has_kid(m)) {
                    let i = self.fixed(id, name, *bx);
                    self.claim(i, &e.path, *kind);
                }
            } else if let Some(name) = rules::jdk_display(&dir).filter(|_| has_kid("bin")) {
                let i = self.dev_item(format!("app:dev:{}", lower(&e.path)), &name, "env-lang", "Java");
                self.claim(i, &e.path, Kind::Prog);
            }
        }
    }

    /// nvm 目录里每个 v20.18.1 是一个 Node.js 版本
    fn node_versions(&mut self, nvm: &str) {
        for e in self.idx.children(nvm) {
            if let Some(name) = rules::runtime_display(&base(&e.path)).filter(|n| e.is_dir && n.starts_with("Node.js")) {
                let i = self.dev_item(format!("app:dev:{}", lower(&e.path)), &name, "env-node", "nvm");
                self.claim_inside(i, &e.path, Kind::Regen);
            }
        }
    }

    /// npm 全局包：每个包一项；Claude Code、Codex 这类命令行工具单独认
    fn npm_globals(&mut self, node_modules: &str) {
        let mut pkgs: Vec<(String, String)> = Vec::new();
        for c in self.idx.children(node_modules) {
            if !c.is_dir {
                continue;
            }
            let name = base(&c.path);
            if name.starts_with('@') {
                for g in self.idx.children(&c.path) {
                    if g.is_dir {
                        pkgs.push((format!("{name}/{}", base(&g.path)), g.path.clone()));
                    }
                }
            } else {
                pkgs.push((name, c.path.clone()));
            }
        }
        for (pkg, path) in pkgs {
            let i = match rules::NPM_TOOLS.iter().find(|(p, ..)| p.eq_ignore_ascii_case(&pkg)) {
                Some((_, id, label, bx)) => self.fixed(id, label, *bx),
                None => self.dev_item(format!("app:dev:npm:{}", pkg.to_ascii_lowercase()), &pkg, "env-node", "npm 全局包"),
            };
            self.claim_inside(i, &path, Kind::Regen);
        }
    }

    // 5. 数据目录 -------------------------------------------------------------

    fn data_dirs(&mut self, env: &Env) {
        self.squirrel_apps();
        let mut containers: Vec<(String, &'static str)> = Vec::new();
        for h in self.homes.clone() {
            if WINDOWS {
                let appdata = join_path(&h, "AppData");
                containers.push((join_path(&appdata, "Roaming"), "roaming"));
                containers.push((join_path(&appdata, "Local"), "local"));
                containers.push((join_path(&appdata, "LocalLow"), "locallow"));
                containers.push((join_path(&h, "Documents"), "docs"));
            } else {
                let lib = join_path(&h, "Library");
                containers.push((join_path(&lib, "Application Support"), "roaming"));
                containers.push((join_path(&lib, "Caches"), "caches"));
                containers.push((join_path(&lib, "Containers"), "roaming"));
            }
            containers.push((h, "home"));
        }
        for r in self.roots.clone() {
            if WINDOWS {
                containers.push((join_path(&r, "ProgramData"), "programdata"));
            }
            containers.push((r, "root"));
        }
        for (path, place) in containers {
            self.scan_container(&path, place, env);
        }
    }

    /// Squirrel 装的软件（Local\T2GP Launcher\app-1.15.0.6071\T2GP Launcher.exe）常常没登记安装位置：
    /// 按目录结构认成软件，Roaming 里同名的数据才有主
    fn squirrel_apps(&mut self) {
        if !WINDOWS {
            return;
        }
        let locals: Vec<String> = self.homes.iter().map(|h| join_path(&join_path(h, "AppData"), "Local")).collect();
        for local in locals {
            for c in self.idx.children(&local) {
                let squirrel = c.is_dir
                    && self.idx.children(&c.path).iter().any(|k| k.is_dir && k.exe_depth == 0 && is_squirrel_version(&base(&k.path)));
                if squirrel && !self.is_claimed(&c.path) {
                    self.unregistered(&c.path, None);
                }
            }
        }
    }

    fn scan_container(&mut self, container: &str, place: &'static str, env: &Env) {
        for c in self.idx.children(container) {
            if self.is_claimed(&c.path) || self.is_program_root(&c.path) {
                continue;
            }
            let name = base(&c.path);
            let lname = name.to_ascii_lowercase();
            if let Some(tc) = rules::TOOLCHAINS.iter().find(|t| t.place == place && t.folder == lname) {
                let i = self.fixed(tc.id, tc.name, tc.purpose);
                self.claim(i, &c.path, tc.kind);
                continue;
            }
            if skip_in_container(place, &lname) || (place == "root" && (name.starts_with('$') || is_system_dir(&c.path, env))) {
                continue;
            }
            let default_kind = if place == "caches" { Kind::Cache } else { Kind::Data };
            // 用户目录、文档、盘根下放的多是用户自己的东西：code、projects 这类常用词不认，
            // 只认名字完全对得上的，认出来的位置里也不去找缓存
            let personal = matches!(place, "home" | "docs" | "root");
            if personal && rules::is_personal_folder_name(&name) {
                continue;
            }
            // 厂商目录（Microsoft、Tencent）装着好几个软件的数据，先往下一层认，不能整个归给某一个
            let vendor = c.is_dir && rules::is_vendor_folder(&name);
            if !vendor {
                if let Some((i, kind, score)) = self.match_app_scored(&name) {
                    if !personal || score >= STRONG_MATCH {
                        // 别名认出的（SteamLibrary）是软件自己的目录，里面的缓存照常找
                        if self.claim(i, &c.path, kind.unwrap_or(default_kind)) && personal && score < ALIAS_MATCH {
                            self.no_cache.insert(lower(&c.path));
                        }
                        continue;
                    }
                }
            }
            if !c.is_dir {
                continue;
            }
            let publisher_apps = self.apps_by_publisher(&name);
            if vendor || !publisher_apps.is_empty() {
                // 已知的大厂商目录（Tencent、Microsoft）装着多个软件，认不出的子目录不能整个归给一家
                let whole_owner = if vendor { &[][..] } else { &publisher_apps[..] };
                self.scan_vendor_dir(&c.path, whole_owner, default_kind);
            }
        }
    }

    /// 厂商目录：子目录按名字认；一个都认不出、而这家厂商只有一个软件时整个归它
    fn scan_vendor_dir(&mut self, path: &str, publisher_apps: &[usize], kind: Kind) {
        let mut any = false;
        for g in self.idx.children(path) {
            if self.is_claimed(&g.path) {
                continue;
            }
            if let Some((i, k)) = self.match_app(&base(&g.path)) {
                any |= self.claim(i, &g.path, k.unwrap_or(kind));
            }
        }
        if !any && publisher_apps.len() == 1 {
            self.claim(publisher_apps[0], path, kind);
        }
    }

    fn apps_by_publisher(&self, name: &str) -> Vec<usize> {
        let n = norm(name);
        if rules::is_generic_key(&n) {
            return Vec::new();
        }
        (0..self.apps.len())
            .filter(|&i| {
                let p = norm(&self.apps[i].publisher);
                !p.is_empty() && rules::publisher_is(&p, &n)
            })
            .collect()
    }

    fn match_app(&self, folder: &str) -> Option<(usize, Option<Kind>)> {
        self.match_app_scored(folder).map(|(i, k, _)| (i, k))
    }

    /// 按目录名找软件：别名优先，其次名字匹配；两个软件打平时不归。返回匹配强度（别名、全名相同为强）
    fn match_app_scored(&self, folder: &str) -> Option<(usize, Option<Kind>, usize)> {
        let forms = name_forms(folder);
        for f in &forms {
            for (alias, target, kind) in rules::ALIASES {
                if !rules::alias_matches(f, alias) {
                    continue;
                }
                let mut hits: Vec<usize> = (0..self.apps.len()).filter(|&i| self.apps[i].keys.iter().any(|k| k == target)).collect();
                if hits.len() > 1 {
                    hits.retain(|&i| norm(&self.apps[i].name) == *target);
                }
                if hits.len() == 1 {
                    return Some((hits[0], *kind, ALIAS_MATCH));
                }
            }
        }
        let mut best = 0;
        let mut tied: Vec<usize> = Vec::new();
        for (i, a) in self.apps.iter().enumerate() {
            let score = forms.iter().flat_map(|f| a.keys.iter().map(move |k| rules::match_score(f, k))).max().unwrap_or(0);
            if score == 0 || score < best {
                continue;
            }
            if score > best {
                best = score;
                tied.clear();
            }
            tied.push(i);
        }
        // 打平时（老版 QQ 和 QQNT 都有 QQ.exe）名字本身就是这个目录名的那个胜出，还分不出就不归
        if tied.len() > 1 {
            tied.retain(|&i| forms.contains(&norm(&self.apps[i].name)));
        }
        match tied[..] {
            [i] => Some((i, None, best)),
            _ => None,
        }
    }

    /// 离某位置最近的已归属位置（小写路径）和它的主人
    fn owner_part_of(&self, path: &str) -> Option<(String, usize)> {
        let mut cur = Some(path);
        while let Some(p) = cur {
            let l = lower(p);
            if let Some(&i) = self.claimed.get(&l) {
                return Some((l, i));
            }
            let up = parent_of(p);
            cur = if up == Some(p) { None } else { up };
        }
        None
    }

    // 6. 缓存 -----------------------------------------------------------------

    fn caches(&mut self) {
        // 每个名字像缓存的目录，找离它最近的已归属上级
        let mut found: Vec<(usize, String, u64)> = Vec::new();
        for e in self.idx.by_path.values() {
            if !e.is_dir || e.size < MIN_CACHE || !rules::is_junk_cache(&base(&e.path)) || self.pinned.contains(&lower(&e.path)) {
                continue;
            }
            let Some(parent) = parent_of(&e.path) else { continue };
            let Some((owner_path, i)) = self.owner_part_of(parent) else { continue };
            let owner_kind = self.apps[i].parts.iter().find(|p| lower(&p.path) == owner_path).map(|p| p.kind);
            if self.no_cache.contains(&owner_path)
                || matches!(owner_kind, Some(Kind::Cache | Kind::Residue))
                || !cache_allowed(&self.apps[i], &e.path)
            {
                continue;
            }
            found.push((i, e.path.clone(), e.size));
        }
        // 外层的先收，里层的不再重复
        found.sort_by_key(|(_, p, _)| p.len());
        let mut taken: HashSet<String> = HashSet::new();
        for (i, path, size) in found {
            let inside_taken = {
                let mut cur = parent_of(&path);
                let mut hit = false;
                while let Some(p) = cur {
                    if taken.contains(&lower(p)) {
                        hit = true;
                        break;
                    }
                    let up = parent_of(p);
                    cur = if up == Some(p) { None } else { up };
                }
                hit
            };
            if inside_taken || self.apps[i].parts.iter().any(|p| p.path.eq_ignore_ascii_case(&path)) {
                continue;
            }
            taken.insert(lower(&path));
            self.apps[i].parts.push(Part::new(path, Kind::Cache, size));
        }
    }

    // 7. 卸载残留 -------------------------------------------------------------

    fn mark_uninstalled(&mut self) {
        if !WINDOWS {
            return;
        }
        let roots = self.program_roots_list();
        for a in self.apps.iter_mut() {
            if a.registered || a.fixed_purpose.is_some() {
                continue;
            }
            let known = a.keys.iter().any(|k| {
                self.installed_names.iter().any(|n| n == k || (rules::compact(n).len() >= 4 && rules::compact(n) == rules::compact(k)))
            });
            if known {
                continue;
            }
            let no_exe = {
                let mut progs = a.parts.iter().filter(|p| p.kind == Kind::Prog).peekable();
                progs.peek().is_some()
                    && progs.all(|p| {
                        roots.iter().any(|r| is_strict_ancestor(r, &p.path))
                            && self.idx.get(&p.path).map_or(true, |e| e.exe_depth == NO_EXE)
                    })
            };
            if no_exe {
                a.uninstalled = true;
                a.icon = None;
                // 用户 / AI 指定的成分不改
                for p in a.parts.iter_mut().filter(|p| p.kind == Kind::Prog && p.origin.is_none()) {
                    p.kind = Kind::Residue;
                }
            }
        }
    }

    // 收尾 --------------------------------------------------------------------

    fn finish(mut self, digests: &[DriveDigest]) -> Inventory {
        // 嵌套的位置从离它最近的外层扣掉
        let mut owner: HashMap<String, (usize, usize)> = HashMap::new();
        for (ai, a) in self.apps.iter().enumerate() {
            for (pi, p) in a.parts.iter().enumerate() {
                owner.insert(lower(&p.path), (ai, pi));
            }
        }
        let mut deduct: Vec<(usize, usize, u64)> = Vec::new();
        let mut claimed_below: HashMap<String, u64> = HashMap::new();
        for a in &self.apps {
            for p in &a.parts {
                let mut cur = parent_of(&p.path);
                let mut nearest_found = false;
                while let Some(up) = cur {
                    let key = lower(up);
                    if !nearest_found {
                        if let Some(&(ai, pi)) = owner.get(&key) {
                            deduct.push((ai, pi, p.size));
                            nearest_found = true;
                        }
                    }
                    if !nearest_found {
                        *claimed_below.entry(key).or_default() += p.size;
                    }
                    let next = parent_of(up);
                    cur = if next == Some(up) { None } else { next };
                }
            }
        }
        for (ai, pi, size) in deduct {
            let p = &mut self.apps[ai].parts[pi];
            p.size = p.size.saturating_sub(size);
        }
        // 能不能删：和删除命令用同一套系统保护；里面还装着别的位置的也不能整个删（会把它们一起删掉）
        let all_parts: Vec<String> = owner.keys().cloned().collect();
        let temp_parents: Vec<String> = self
            .homes
            .iter()
            .map(|h| lower(&join_path(&join_path(h, "AppData"), "Local")))
            .chain(self.system_root.iter().map(|s| lower(s)))
            .collect();
        for a in self.apps.iter_mut() {
            for p in a.parts.iter_mut() {
                let l = lower(&p.path);
                let holds_others = all_parts.iter().any(|o| is_strict_ancestor(&l, o));
                p.deletable = !crate::commands::cleanup::is_protected(&p.path) && !holds_others;
                p.contents_only = self.temp_dirs.contains(&l)
                    || (base(&p.path).eq_ignore_ascii_case("temp") && parent_of(&p.path).is_some_and(|up| temp_parents.contains(&lower(up))));
            }
        }

        let tree = digests
            .iter()
            .map(|d| {
                let kids = self.folder_kids(&d.root, &claimed_below, &owner);
                FolderNode {
                    key: format!("dir:{}", d.root),
                    path: d.root.clone(),
                    name: d.root.clone(),
                    is_dir: true,
                    depth: 0,
                    rest: d.total.saturating_sub(claimed_below.get(&lower(&d.root)).copied().unwrap_or(0)),
                    hint: None,
                    kids,
                }
            })
            .collect();

        let apps = self
            .apps
            .into_iter()
            .filter_map(|mut a| {
                a.parts.retain(|p| p.size > 0);
                a.parts.sort_by(|x, y| y.size.cmp(&x.size));
                let total: u64 = a.parts.iter().map(|p| p.size).sum();
                if total < MIN_APP {
                    return None;
                }
                let hint = a
                    .fixed_purpose
                    .or(a.hint_box)
                    .or_else(|| rules::purpose_hint(&a.keys, &a.publisher))
                    .map(str::to_string);
                Some(AppItem {
                    key: a.key,
                    name: a.name,
                    publisher: a.publisher,
                    icon: a.icon,
                    uninstalled: a.uninstalled,
                    hint,
                    category: a.category.to_string(),
                    parts: a.parts,
                })
            })
            .collect();

        Inventory {
            drives: digests.iter().map(|d| DriveInfo { root: d.root.clone(), scanned_at: d.scanned_at, total: d.total }).collect(),
            apps,
            tree,
        }
    }

    fn folder_kids(&self, path: &str, claimed_below: &HashMap<String, u64>, owner: &HashMap<String, (usize, usize)>) -> Vec<FolderNode> {
        let mut kids: Vec<FolderNode> = self
            .idx
            .children(path)
            .into_iter()
            .filter(|e| !owner.contains_key(&lower(&e.path)))
            .filter_map(|e| {
                let rest = e.size.saturating_sub(claimed_below.get(&lower(&e.path)).copied().unwrap_or(0));
                (rest >= MIN_NODE).then(|| FolderNode {
                    key: format!("dir:{}", e.path),
                    path: e.path.clone(),
                    name: base(&e.path),
                    is_dir: e.is_dir,
                    depth: e.depth,
                    rest,
                    hint: if e.is_dir && rules::is_personal_area(&e.path) {
                        let names: Vec<String> = self.idx.children(&e.path).iter().map(|k| base(&k.path)).collect();
                        rules::personal_hint(&base(&e.path), &names).map(str::to_string)
                    } else {
                        None
                    },
                    kids: if e.is_dir { self.folder_kids(&e.path, claimed_below, owner) } else { Vec::new() },
                })
            })
            .collect();
        kids.sort_by(|a, b| b.rest.cmp(&a.rest));
        kids
    }
}

/// 缓存清理的范围：系统目录里只认临时文件，系统组件、安装程序缓存不动；
/// 应用商店应用（含拆出来单列的）在 Packages 里的 LocalCache 是应用重定向写入的真实数据，只认 INetCache、TempState
fn cache_allowed(app: &Draft, path: &str) -> bool {
    let name = base(path).to_ascii_lowercase();
    match app.key.as_str() {
        "app:rule:windows" => name == "temp",
        "app:rule:store" => matches!(name.as_str(), "inetcache" | "tempstate"),
        k if k.starts_with("app:store:") && lower(path).contains("\\packages\\") => matches!(name.as_str(), "inetcache" | "tempstate"),
        "app:rule:windows-components" | "app:rule:installer-cache" | "app:rule:swap" | "app:rule:temp"
        | "app:rule:crashdumps" | "app:rule:shadercache" => false,
        _ => true,
    }
}

/// 目录名的几种写法：原名；反向域名（`com.ccswitch.desktop`、`io.github.clash-verge-rev`）去掉前缀
fn name_forms(folder: &str) -> Vec<String> {
    let n = norm(folder);
    let mut forms = vec![n.clone()];
    let tokens: Vec<&str> = n.split(' ').collect();
    let prefixes: &[&[&str]] = &[&["io", "github"], &["com"], &["org"], &["dev"], &["net"], &["io"], &["cn"]];
    for pre in prefixes {
        if tokens.len() > pre.len() && tokens[..pre.len()] == **pre {
            let rest = &tokens[pre.len()..];
            for take in (1..=rest.len()).rev() {
                let f = rest[..take].join(" ");
                if !forms.contains(&f) {
                    forms.push(f);
                }
            }
            if rest.len() > 1 {
                let f = rest[1..].join(" ");
                if !forms.contains(&f) {
                    forms.push(f);
                }
            }
            break;
        }
    }
    // 名字后面跟着版本号的（Anki2）也按去掉数字的名字认
    let trimmed = n.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
    if trimmed.chars().count() >= 3 && trimmed != n && !forms.iter().any(|f| f == trimmed) {
        forms.push(trimmed.to_string());
    }
    forms
}

/// Squirrel 的版本目录：app-1.15.0.6071
fn is_squirrel_version(dir: &str) -> bool {
    dir.strip_prefix("app-").is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit()))
}

fn exe_stem(path: &str) -> String {
    let name = base(path);
    match name.rsplit_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name,
    }
}

fn is_helper_exe(stem: &str) -> bool {
    let s = rules::compact(stem);
    ["unins", "uninst", "update", "crash", "helper", "setup", "elevat", "notif", "squirrel", "report", "service",
        "install", "repair", "daemon", "agent", "bugsplat", "cef", "webview", "render", "redist", "dxsetup", "launcherold"]
        .iter()
        .any(|w| s.contains(w))
}

/// 用户目录、盘根下不往软件名下归的位置（留在待整理，由用户自己整理）
fn skip_in_container(place: &str, lname: &str) -> bool {
    match place {
        "home" => matches!(
            lname,
            "appdata" | "application data" | "local settings" | "documents" | "my documents" | "desktop" | "downloads"
                | "pictures" | "videos" | "music" | "movies" | "public" | "favorites" | "contacts" | "links" | "searches"
                | "saved games" | "3d objects" | "onedrive" | "cookies" | "nethood" | "printhood" | "recent" | "sendto"
                | "templates" | "「开始」菜单" | "start menu" | "library" | "applications" | "source" | "repos"
        ),
        "root" => matches!(
            lname,
            "users" | "windows" | "programdata" | "program files" | "program files (x86)" | "recovery" | "perflogs"
                | "system volume information" | "documents and settings" | "msocache" | "library" | "system" | "applications"
                | "volumes" | "private" | "opt" | "usr" | "bin" | "sbin" | "etc" | "var" | "tmp"
        ),
        _ => false,
    }
}

/// 盘根下属于系统的目录
fn is_system_dir(path: &str, env: &Env) -> bool {
    env.system_root.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(path))
}

mod overrides;
mod packages;

#[cfg(test)]
mod tests;
