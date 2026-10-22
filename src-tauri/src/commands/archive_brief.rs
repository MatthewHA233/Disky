//! 每轮对话开场交代给整理助手的「档案馆现状」：门类和箱子、各盘、还没整理的软件和文件夹、
//! 待核对的缓存、用户说过的话。只是数据，做事的规矩在 archive_chat 的角色说明里。

use std::collections::HashMap;

use tauri::AppHandle;

use crate::archive::digest::{self, DriveDigest};
use crate::archive::inventory::{self, Env, FolderNode, Inventory};
use crate::archive::registry::{self, InstalledApp};
use crate::archive::store::{self, AiPlacement, BoxRow, NoteRow, UserPlacement};
use crate::scanner::classify::Kind;

/// AI 放进箱子要有多大把握才算数（与前端 AI_MIN_CONFIDENCE 一致）
const AI_MIN: f64 = 0.6;
const LIST_APPS: usize = 40;
const LIST_FOLDERS: usize = 30;
const FOLDER_MIN: u64 = 200 * 1024 * 1024;

/// 开场要用到的全部数据（读库、读摘要、重新识别一遍）
pub struct Snapshot {
    pub digests: Vec<DriveDigest>,
    pub installed: Vec<InstalledApp>,
    pub inv: Inventory,
    pub boxes: Vec<BoxRow>,
    pub user: Vec<UserPlacement>,
    pub ai: Vec<AiPlacement>,
    pub notes: Vec<NoteRow>,
    pub mode: Option<String>,
    /// 还没核对的「像是缓存 / 残留」的位置：(处数, 大小)
    pub pending: (usize, u64),
}

/// 读出现状（会读盘，放在 spawn_blocking 里调）
pub fn load_snapshot(app: &AppHandle) -> Result<Snapshot, String> {
    let digests = digest::load_all(&super::archive::digest_dir(app)?);
    let installed = registry::installed_apps();
    let conn = super::archive::db(app)?;
    let inv = inventory::build(&digests, &installed, &Env::current(), &super::archive::overrides(store::claims(&conn)?));
    let checked: std::collections::HashSet<String> =
        store::checks(&conn)?.into_iter().filter(|c| c.verdict != "unsure").map(|c| c.path.to_ascii_lowercase()).collect();
    let pending = inv
        .apps
        .iter()
        .flat_map(|a| a.parts.iter())
        .filter(|p| matches!(p.kind, Kind::Cache | Kind::Residue) && p.deletable && p.origin.as_deref() != Some("user"))
        .filter(|p| !checked.contains(&p.path.to_ascii_lowercase()))
        .fold((0, 0), |(n, s), p| (n + 1, s + p.size));
    Ok(Snapshot {
        digests,
        installed,
        inv,
        boxes: store::boxes(&conn)?,
        user: store::user_placements(&conn)?,
        ai: store::ai_placements(&conn)?,
        notes: store::notes(&conn)?,
        mode: store::mode(&conn)?,
        pending,
    })
}

fn human(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / 1024.0 / 1024.0)
    }
}

/// 某一项现在在哪个箱子：用户放的 > 有把握的 AI 判断 > 内置规则
fn placed<'a>(key: &str, user: &HashMap<&str, Option<&'a str>>, ai: &HashMap<&str, &'a AiPlacement>, hint: Option<&'a str>) -> Option<&'a str> {
    if let Some(u) = user.get(key) {
        return *u;
    }
    ai.get(key).filter(|a| a.confidence >= AI_MIN).map(|a| a.box_id.as_str()).or(hint)
}

/// 装着很多软件的系统目录本身不算「没认出的文件夹」，往里一层看（与前端 attributeCandidates 一致）
fn is_container(n: &FolderNode, parent: Option<&FolderNode>) -> bool {
    n.depth == 0
        || parent.is_some_and(|p| p.name.eq_ignore_ascii_case("users"))
        || matches!(
            n.name.to_ascii_lowercase().as_str(),
            "users" | "appdata" | "local" | "locallow" | "roaming" | "programdata" | "program files" | "program files (x86)" | "programs"
                | "library" | "application support"
        )
}

fn loose_folders<'a>(roots: &'a [FolderNode], user: &HashMap<&str, Option<&str>>, ai: &HashMap<&str, &AiPlacement>) -> Vec<&'a FolderNode> {
    fn walk<'a>(n: &'a FolderNode, parent: Option<&FolderNode>, keep: &dyn Fn(&FolderNode) -> bool, out: &mut Vec<&'a FolderNode>) {
        if !n.is_dir || n.rest < FOLDER_MIN || !keep(n) {
            return;
        }
        if is_container(n, parent) {
            n.kids.iter().for_each(|k| walk(k, Some(n), keep, out));
        } else {
            out.push(n);
        }
    }
    let keep = |n: &FolderNode| placed(&n.key, user, ai, None).is_none();
    let mut out = Vec::new();
    roots.iter().for_each(|r| walk(r, None, &keep, &mut out));
    out.sort_by(|a, b| b.rest.cmp(&a.rest));
    out.truncate(LIST_FOLDERS);
    out
}

/// 档案馆现状，给 AI 看的 Markdown
pub fn brief(s: &Snapshot) -> String {
    let user: HashMap<&str, Option<&str>> = s.user.iter().map(|u| (u.key.as_str(), u.box_id.as_deref())).collect();
    let ai: HashMap<&str, &AiPlacement> = s.ai.iter().map(|a| (a.key.as_str(), a)).collect();
    let mut out = String::from("# 档案馆现状（Disky 每轮自动生成，以这里为准）\n\n");
    out.push_str(match s.mode.as_deref() {
        Some("template") => "整理模式：模板模式。之前只按内置规则看名字认软件、猜文件夹，大概率有放错、认错的地方，值得逐个复查。\n",
        Some("ai") => "整理模式：AI 整理。\n",
        _ => "整理模式：还没走完新手引导。\n",
    });

    out.push_str("\n## 箱子（id：名字）\n");
    for (cat, label) in [("software", "软件"), ("personal", "个人文件"), ("devenv", "插件与开发环境")] {
        let names: Vec<String> = s.boxes.iter().filter(|b| b.category == cat).map(|b| format!("{}：{}", b.id, b.name)).collect();
        out.push_str(&format!("- {label}：{}\n", if names.is_empty() { "（没有箱子）".into() } else { names.join("、") }));
    }

    out.push_str("\n## 各盘\n");
    if s.inv.drives.is_empty() {
        out.push_str("- 还没有扫描过任何盘\n");
    }
    for d in &s.inv.drives {
        out.push_str(&format!("- {}：{}\n", d.root, human(d.total)));
    }

    for (cat, title) in [("software", "还没进箱子的软件"), ("devenv", "还没进箱子的插件与开发环境")] {
        let mut inbox: Vec<(&inventory::AppItem, u64)> = s
            .inv
            .apps
            .iter()
            .filter(|a| a.category == cat && placed(&a.key, &user, &ai, a.hint.as_deref()).is_none())
            .map(|a| (a, a.parts.iter().map(|p| p.size).sum()))
            .collect();
        inbox.sort_by(|a, b| b.1.cmp(&a.1));
        out.push_str(&format!("\n## {title}（共 {} 个，列最大的 {LIST_APPS} 个）\n", inbox.len()));
        for (a, size) in inbox.iter().take(LIST_APPS) {
            let publisher = if a.publisher.is_empty() { String::new() } else { format!("，{}", a.publisher) };
            let gone = if a.uninstalled { "，像是已卸载" } else { "" };
            out.push_str(&format!("- key={} {}（{}{publisher}{gone}）\n", a.key, a.name, human(*size)));
        }
    }

    let loose = loose_folders(&s.inv.tree, &user, &ai);
    out.push_str(&format!("\n## 没认出归属、也没进箱子的文件夹（列最大的 {LIST_FOLDERS} 个）\n"));
    if loose.is_empty() {
        out.push_str("- 没有了\n");
    }
    for n in loose {
        let hint = n.hint.as_deref().map(|h| format!("，名字像是 {h}")).unwrap_or_default();
        out.push_str(&format!("- key=dir:{} （{}{hint}）\n", n.path, human(n.rest)));
    }

    let (n, size) = s.pending;
    out.push_str(&format!("\n## 待核对的缓存\n- {n} 处名字像缓存 / 残留的位置还没核对，共 {}；用户在档案馆点「让 AI 核对」就会核对\n", human(size)));

    // 这些是对话里由 AI 记下的，用户能在「AI 记住的」里看到、删掉；和用户当面说的冲突时以用户为准
    out.push_str("\n## 之前对话里记下的用户要求（遵守；和用户这次说的冲突时以这次为准）\n");
    if s.notes.is_empty() {
        out.push_str("- 还没有\n");
    }
    for note in &s.notes {
        out.push_str(&format!("- {}\n", note.note));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::inventory::{AppItem, DriveInfo, Part};

    fn app(key: &str, name: &str, hint: Option<&str>, size: u64) -> AppItem {
        AppItem {
            key: key.into(),
            name: name.into(),
            publisher: String::new(),
            icon: None,
            uninstalled: false,
            hint: hint.map(str::to_string),
            category: "software".into(),
            parts: vec![Part { path: format!("C:\\{name}"), kind: Kind::Prog, size, deletable: true, contents_only: false, origin: None, reason: None }],
        }
    }

    fn folder(path: &str, depth: u8, rest: u64, kids: Vec<FolderNode>) -> FolderNode {
        FolderNode { key: format!("dir:{path}"), path: path.into(), name: path.rsplit('\\').next().unwrap().into(), is_dir: true, depth, rest, hint: None, kids }
    }

    #[test]
    fn brief_lists_what_is_left() {
        const GB: u64 = 1 << 30;
        let snap = Snapshot {
            digests: Vec::new(),
            installed: Vec::new(),
            inv: Inventory {
                drives: vec![DriveInfo { root: "D:".into(), scanned_at: 1, total: 100 * GB }],
                apps: vec![app("app:steam", "Steam", Some("game"), 5 * GB), app("app:odd", "Odd", None, 2 * GB), app("app:ai", "AiPlaced", None, GB)],
                tree: vec![folder("D:", 0, 50 * GB, vec![folder("D:\\stuff", 1, 30 * GB, Vec::new()), folder("D:\\placed", 1, 10 * GB, Vec::new())])],
            },
            boxes: vec![BoxRow { id: "game".into(), name: "游戏".into(), color: String::new(), builtin: true, category: "software".into() }],
            user: vec![UserPlacement { key: "dir:D:\\placed".into(), box_id: Some("game".into()) }],
            ai: vec![AiPlacement { key: "app:ai".into(), box_id: "game".into(), confidence: 0.9, reason: String::new() }],
            notes: vec![NoteRow { id: 1, note: "游戏都放游戏箱".into(), created_at: String::new() }],
            mode: Some("template".into()),
            pending: (3, GB),
        };
        let b = brief(&snap);
        assert!(b.contains("模板模式") && b.contains("game：游戏"));
        assert!(b.contains("还没进箱子的软件（共 1 个") && b.contains("key=app:odd Odd"), "规则和 AI 放好的不列：{b}");
        assert!(b.contains("key=dir:D:\\stuff") && !b.contains("D:\\placed"));
        assert!(b.contains("3 处") && b.contains("- 游戏都放游戏箱"));
    }
}
