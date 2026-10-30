//! 档案馆命令：跨盘的软件清单、箱子、装箱、图标、AI 归类。
//! `classify_footprints` 仍给目录视图的矩形图按性质着色用。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::scan::ScanState;
use crate::archive::inventory::{self, AppItem, Claimed, DriveInfo, Env, FolderNode, Override};
use crate::archive::loose::{self, LooseList};
use crate::archive::store::{self, AiPlacement, BoxRow, CacheCheck, ClaimRow, UserPlacement};
use crate::archive::growth::{self, Growth};
use crate::archive::{digest, icons, registry};
use crate::scanner::classify::{classify, Footprint, Kind};
use crate::scanner::normalize_path;

#[tauri::command]
pub fn classify_footprints(state: State<'_, ScanState>) -> Result<Vec<Footprint>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let tree = guard.as_ref().ok_or("No scan data. Run a scan first.")?;
    Ok(classify(tree))
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// 各盘摘要所在目录
pub(crate) fn digest_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("archive"))
}

pub(crate) fn db(app: &AppHandle) -> Result<Connection, String> {
    let dir = data_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let conn = Connection::open(dir.join("disky.db")).map_err(|e| e.to_string())?;
    store::open(&conn)?;
    Ok(conn)
}

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Serialize)]
pub struct ArchiveModel {
    drives: Vec<DriveInfo>,
    apps: Vec<AppItem>,
    tree: Vec<FolderNode>,
    boxes: Vec<BoxRow>,
    user: Vec<UserPlacement>,
    ai: Vec<AiPlacement>,
    checks: Vec<CacheCheck>,
    /// 和上次扫描比的大小变化
    growth: Growth,
}

/// 指定的归属交给识别：AI 指定的要有足够把握才采用
pub(crate) fn overrides(rows: Vec<ClaimRow>) -> Vec<Override> {
    rows.into_iter()
        .filter(|r| r.source == "user" || r.confidence >= AI_CLAIM_MIN)
        .filter_map(|r| {
            Some(Override { kind: Kind::parse(&r.kind)?, path: r.path, app_key: r.app_key, origin: r.source, reason: r.reason })
        })
        .collect()
}

/// 与前端 AI_MIN_CONFIDENCE 一致
const AI_CLAIM_MIN: f64 = 0.6;

/// 上次给前端的清单里哪些位置归到了软件名下：列文件夹里零散的东西时扣掉
static CLAIMED: Mutex<Option<Claimed>> = Mutex::new(None);

#[tauri::command]
pub async fn get_archive(app: AppHandle) -> Result<ArchiveModel, String> {
    tokio::task::spawn_blocking(move || {
        let digests = digest::load_all(&digest_dir(&app)?);
        let installed = registry::installed_apps();
        let conn = db(&app)?;
        let inv = inventory::build(&digests, &installed, &Env::current(), &overrides(store::claims(&conn)?));
        let growth = growth::compute(&inv, &digests, &digest::load_prev_all(&digest_dir(&app)?));
        *CLAIMED.lock().unwrap_or_else(|e| e.into_inner()) = Some(inv.claimed);
        let live = super::archive_live::watched(&app);
        Ok(ArchiveModel {
            growth,
            drives: inv.drives.into_iter().map(|d| DriveInfo { live: live.contains(&d.root), ..d }).collect(),
            apps: inv.apps,
            tree: inv.tree,
            boxes: store::boxes(&conn)?,
            user: store::user_placements(&conn)?,
            ai: store::ai_placements(&conn)?,
            checks: store::checks(&conn)?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 文件夹树里一个位置下没单独列出的东西，现场去磁盘上列这一层。
/// `skip`：树上单独列着或放进了箱子的子位置；`known`：树上有、小到不单独显示的子位置和前端算的大小
#[tauri::command]
pub async fn archive_loose(app: AppHandle, path: String, skip: Vec<String>, known: Vec<(String, u64)>) -> Result<LooseList, String> {
    tokio::task::spawn_blocking(move || {
        let path = normalize_path(&path);
        let digests = digest::load_all(&digest_dir(&app)?);
        if !digests.iter().any(|d| d.root.eq_ignore_ascii_case(&path) || d.covers(&path)) {
            return Err("这个位置不在扫过的盘上".into());
        }
        let claimed = CLAIMED.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default();
        let skip = skip.iter().map(|p| p.to_ascii_lowercase()).collect();
        let known = known.into_iter().map(|(p, s)| (p.to_ascii_lowercase(), s)).collect();
        Ok(loose::list(&path, &digests, &claimed, &skip, &known))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 把若干项放进箱子；`box_id` 为空表示放回待整理
#[tauri::command]
pub fn archive_place(app: AppHandle, keys: Vec<String>, box_id: Option<String>) -> Result<(), String> {
    store::place(&db(&app)?, &keys, box_id.as_deref())
}

/// 用户指定某个位置属于哪个软件的哪种成分；`app_key` 为空表示不属于任何软件
#[tauri::command]
pub fn archive_assign(app: AppHandle, path: String, app_key: Option<String>, kind: String) -> Result<(), String> {
    let row = ClaimRow { path, app_key, kind, source: "user".into(), confidence: 1.0, reason: String::new() };
    store::save_claim(&db(&app)?, &row).map(|_| ())
}

/// 撤掉指定的归属，回到内置规则的判断
#[tauri::command]
pub fn archive_unassign(app: AppHandle, path: String) -> Result<(), String> {
    store::delete_claim(&db(&app)?, &path)
}

/// 整理助手记下的用户回答和偏好
#[tauri::command]
pub fn archive_notes(app: AppHandle) -> Result<Vec<store::NoteRow>, String> {
    store::notes(&db(&app)?)
}

#[tauri::command]
pub fn archive_delete_note(app: AppHandle, id: i64) -> Result<(), String> {
    store::delete_note(&db(&app)?, id)
}

/// 整理模式：ai / template；还没走完新手引导时为 null
#[tauri::command]
pub fn archive_mode(app: AppHandle) -> Result<Option<String>, String> {
    store::mode(&db(&app)?)
}

#[tauri::command]
pub fn archive_set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    store::set_mode(&db(&app)?, &mode)
}

/// 重新开始整理：先把整个库备份一份（disky-备份-时间.db），再清掉整理结果。各盘扫描摘要保留
#[tauri::command]
pub async fn archive_reset(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let conn = db(&app)?;
        // 文件名带上毫秒：同一秒里点两次也不会撞名（VACUUM INTO 不覆盖已有文件）
        let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let backup = data_dir(&app)?.join(format!("disky-备份-{millis}.db"));
        conn.execute("VACUUM INTO ?1", rusqlite::params![backup.to_string_lossy()]).map_err(|e| format!("备份失败，没有清空：{e}"))?;
        store::reset(&conn)?;
        Ok(backup.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn archive_create_box(app: AppHandle, name: String, color: String, category: String) -> Result<BoxRow, String> {
    store::create_box(&db(&app)?, &name, &color, &category)
}

#[tauri::command]
pub fn archive_rename_box(app: AppHandle, id: String, name: String) -> Result<(), String> {
    store::rename_box(&db(&app)?, &id, &name)
}

#[tauri::command]
pub fn archive_delete_box(app: AppHandle, id: String) -> Result<(), String> {
    store::delete_box(&db(&app)?, &id)
}

/// 取一批文件的图标（与输入一一对应，取不到为 null）
#[tauri::command]
pub async fn archive_icons(app: AppHandle, sources: Vec<String>) -> Result<Vec<Option<String>>, String> {
    let dir = data_dir(&app)?.join("icons");
    tokio::task::spawn_blocking(move || sources.iter().map(|s| icons::icon_data_url(&dir, s)).collect())
        .await
        .map_err(|e| e.to_string())
}

/// 删除了一些位置后同步各盘摘要（(路径, 大小)）
#[tauri::command]
pub async fn archive_forget(app: AppHandle, items: Vec<(String, u64)>) -> Result<(), String> {
    let dir = digest_dir(&app)?;
    let live = super::archive_live::watched(&app);
    tokio::task::spawn_blocking(move || digest::forget_paths(&dir, &items, &live))
        .await
        .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// AI 按使用目的归类
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ClassifyItem {
    key: String,
    name: String,
    /// 发布者、位置等线索
    detail: String,
}

#[derive(Debug, Deserialize)]
struct RawPlacement {
    id: serde_json::Value,
    #[serde(rename = "box")]
    box_id: String,
    #[serde(default = "half")]
    confidence: f64,
    #[serde(default)]
    reason: String,
}

fn half() -> f64 {
    0.5
}

const AI_BATCH: usize = 60;

fn classify_prompt(boxes: &[BoxRow], items: &[ClassifyItem]) -> String {
    let box_lines: Vec<String> = boxes.iter().map(|b| format!("- {}：{}", b.id, b.name)).collect();
    let item_lines: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, it)| format!("{}. {}（{}）", i + 1, it.name, it.detail))
        .collect();
    format!(
        "你在帮用户整理电脑里的东西：把下面每一项按「使用目的」放进一个箱子。\n\n\
        箱子（id：名字）：\n{}\n\n\
        要求：\n\
        - 根据名字、发布者和所在位置判断它是干什么用的。\n\
        - confidence 取 0 到 1，表示你有多大把握；只是猜测就给 0.5 以下。\n\
        - 完全看不出用途的，box 填 \"unknown\"。\n\
        - 文件夹里混着多种用途的东西（比如个人资料、下载、项目混放），box 填 \"unknown\"；它的子文件夹会单独列出来判断。\n\
        - reason 用不超过 15 个字说明依据。\n\n\
        待整理：\n{}\n\n\
        只返回 JSON 数组，不要 markdown，不要其他文字：\n\
        [{{\"id\":1,\"box\":\"game\",\"confidence\":0.9,\"reason\":\"Steam 游戏平台\"}}]",
        box_lines.join("\n"),
        item_lines.join("\n")
    )
}

/// 从模型回复里取出 JSON 数组，按编号对回原来的项；不认识的箱子、编号丢掉
fn parse_placements(text: &str, items: &[ClassifyItem], boxes: &[BoxRow]) -> Result<Vec<AiPlacement>, String> {
    let start = text.find('[').ok_or("AI 回复里没有找到结果")?;
    let end = text.rfind(']').filter(|&e| e > start).ok_or("AI 回复里没有找到结果")?;
    let raw: Vec<RawPlacement> =
        serde_json::from_str(&text[start..=end]).map_err(|e| format!("AI 回复格式不对: {e}"))?;
    let known: HashMap<&str, ()> = boxes.iter().map(|b| (b.id.as_str(), ())).collect();
    Ok(raw
        .into_iter()
        .filter_map(|r| {
            let n = match &r.id {
                serde_json::Value::Number(n) => n.as_u64()? as usize,
                serde_json::Value::String(s) => s.trim().trim_start_matches('#').parse().ok()?,
                _ => return None,
            };
            let item = items.get(n.checked_sub(1)?)?;
            known.contains_key(r.box_id.as_str()).then(|| AiPlacement {
                key: item.key.clone(),
                box_id: r.box_id,
                confidence: r.confidence.clamp(0.0, 1.0),
                reason: r.reason.chars().take(40).collect(),
            })
        })
        .collect())
}

/// 让 AI 把一批东西归进某个门类的箱子
#[tauri::command]
pub async fn archive_ai_classify(app: AppHandle, items: Vec<ClassifyItem>, category: String) -> Result<Vec<AiPlacement>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let settings = super::ai::load_ai_settings(app.clone())?;
    let boxes: Vec<BoxRow> = store::boxes(&db(&app)?)?.into_iter().filter(|b| b.category == category).collect();
    if boxes.is_empty() {
        return Err("这个门类还没有箱子".into());
    }
    let mut all = Vec::new();
    for batch in items.chunks(AI_BATCH) {
        let text = super::ai::complete_once(&settings, &classify_prompt(&boxes, batch), 4096).await?;
        all.extend(parse_placements(&text, batch, &boxes)?);
    }
    store::save_ai(&db(&app)?, &all)?;
    Ok(all)
}

// ---------------------------------------------------------------------------
// AI 核对「像是缓存 / 残留」的位置
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CheckItem {
    path: String,
    /// 所属软件
    owner: String,
    /// cache / residue
    kind: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RawCheck {
    id: serde_json::Value,
    verdict: String,
    #[serde(default)]
    reason: String,
}

const CHECK_BATCH: usize = 80;

fn check_prompt(items: &[CheckItem], kids: &[Vec<String>]) -> String {
    let lines: Vec<String> = items
        .iter()
        .zip(kids)
        .enumerate()
        .map(|(i, (it, k))| {
            let what = if it.kind == "residue" { "像是卸载后的残留" } else { "像是缓存" };
            let inside = if k.is_empty() { String::new() } else { format!("；里面有：{}", k.join("、")) };
            format!("{}. {}（属于 {}，{}，{} MB{}）", i + 1, it.path, it.owner, what, it.size / (1024 * 1024), inside)
        })
        .collect();
    format!(
        "磁盘清理工具只按文件夹名字，找出了下面这些「像是缓存 / 卸载残留」的位置。名字像缓存不代表真的能删，请逐个核对：\n\
        - safe：确实是缓存、临时文件、日志、崩溃转储，或者确定是已卸载软件的残留；删掉后软件会自己重建，不会丢用户的东西。\n\
        - keep：里面可能有用户数据、设置、聊天记录、存档、下载的模型 / 插件 / 离线内容，或者删了软件会坏、要重新下载很久。\n\
        - unsure：看不出来。\n\
        reason 用不超过 20 个字说明依据。\n\n\
        待核对：\n{}\n\n\
        只返回 JSON 数组，不要 markdown，不要其他文字：\n\
        [{{\"id\":1,\"verdict\":\"safe\",\"reason\":\"Chromium 内核的网页缓存\"}}]",
        lines.join("\n")
    )
}

fn parse_checks(text: &str, items: &[CheckItem]) -> Result<Vec<CacheCheck>, String> {
    let start = text.find('[').ok_or("AI 回复里没有找到结果")?;
    let end = text.rfind(']').filter(|&e| e > start).ok_or("AI 回复里没有找到结果")?;
    let raw: Vec<RawCheck> = serde_json::from_str(&text[start..=end]).map_err(|e| format!("AI 回复格式不对: {e}"))?;
    Ok(raw
        .into_iter()
        .filter_map(|r| {
            let n = match &r.id {
                serde_json::Value::Number(n) => n.as_u64()? as usize,
                serde_json::Value::String(s) => s.trim().trim_start_matches('#').parse().ok()?,
                _ => return None,
            };
            let item = items.get(n.checked_sub(1)?)?;
            let verdict = r.verdict.trim().to_ascii_lowercase();
            matches!(verdict.as_str(), "safe" | "keep" | "unsure").then(|| CacheCheck {
                path: item.path.clone(),
                verdict,
                reason: r.reason.chars().take(40).collect(),
            })
        })
        .collect())
}

/// 让 AI 核对一批「像是缓存 / 残留」的位置，结论存下来，只有 safe 的才算可清理
#[tauri::command]
pub async fn archive_ai_check(app: AppHandle, items: Vec<CheckItem>) -> Result<Vec<CacheCheck>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let settings = super::ai::load_ai_settings(app.clone())?;
    // 摘要里记着的子目录名给 AI 当线索
    let digests = {
        let dir = digest_dir(&app)?;
        tokio::task::spawn_blocking(move || digest::load_all(&dir)).await.map_err(|e| e.to_string())?
    };
    let kids: Vec<Vec<String>> = items
        .iter()
        .map(|it| {
            let prefix = format!("{}{}", it.path.to_ascii_lowercase(), crate::scanner::SEP);
            let mut names: Vec<(u64, String)> = digests
                .iter()
                .flat_map(|d| d.entries.iter())
                .filter(|e| {
                    let l = e.path.to_ascii_lowercase();
                    l.starts_with(&prefix) && !l[prefix.len()..].contains(crate::scanner::SEP)
                })
                .map(|e| (e.size, crate::scanner::path_file_name(&e.path)))
                .collect();
            names.sort_by(|a, b| b.0.cmp(&a.0));
            names.into_iter().take(6).map(|(_, n)| n).collect()
        })
        .collect();
    let mut all = Vec::new();
    for (batch, kids) in items.chunks(CHECK_BATCH).zip(kids.chunks(CHECK_BATCH)) {
        let text = super::ai::complete_once(&settings, &check_prompt(batch, kids), 4096).await?;
        all.extend(parse_checks(&text, batch)?);
    }
    store::save_checks(&db(&app)?, &all)?;
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes() -> Vec<BoxRow> {
        ["game", "dev"]
            .iter()
            .map(|id| BoxRow { id: id.to_string(), name: id.to_string(), color: String::new(), builtin: true, category: "software".into() })
            .collect()
    }

    #[test]
    fn parses_cache_checks() {
        let items: Vec<CheckItem> = ["C:\\a\\Cache", "C:\\b\\LocalCache"]
            .iter()
            .map(|p| CheckItem { path: p.to_string(), owner: "x".into(), kind: "cache".into(), size: 1 })
            .collect();
        let out = parse_checks("结果如下：[{\"id\":1,\"verdict\":\"SAFE\",\"reason\":\"网页缓存\"},{\"id\":2,\"verdict\":\"maybe\"},{\"id\":5,\"verdict\":\"keep\"}] 以上", &items).unwrap();
        assert_eq!(out, vec![CacheCheck { path: "C:\\a\\Cache".into(), verdict: "safe".into(), reason: "网页缓存".into() }]);
    }

    fn items() -> Vec<ClassifyItem> {
        ["a", "b", "c"].iter().map(|k| ClassifyItem { key: format!("app:{k}"), name: k.to_string(), detail: String::new() }).collect()
    }

    #[test]
    fn parses_reply_with_noise_and_bad_rows() {
        let text = "好的：\n```json\n[{\"id\":1,\"box\":\"game\",\"confidence\":0.9,\"reason\":\"x\"},\
            {\"id\":\"#2\",\"box\":\"nope\"},{\"id\":3,\"box\":\"dev\"},{\"id\":9,\"box\":\"dev\"}]\n```";
        let out = parse_placements(text, &items(), &boxes()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key, "app:a");
        assert_eq!(out[1].key, "app:c");
        assert_eq!(out[1].confidence, 0.5);
    }

    #[test]
    fn rejects_reply_without_array() {
        assert!(parse_placements("抱歉，我不能", &items(), &boxes()).is_err());
    }
}
