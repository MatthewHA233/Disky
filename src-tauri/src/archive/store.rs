//! 箱子与装箱结果，存在 disky.db。
//!
//! - `archive_boxes`：箱子（内置的 + 用户自己建的），各属一个门类（软件 / 个人文件 / 插件与开发环境）
//! - `archive_user`：用户亲手放的位置；box_id 为空表示用户把它放回了待整理
//! - `archive_ai`：AI 的归类判断，带把握程度。用户的决定永远优先
//! - `archive_check`：AI 对「像是缓存 / 残留」的位置的核对结果，核对过可放心删的才算可清理
//! - `archive_claim`：用户（或 AI 查过之后）指定的「这个位置属于哪个软件的哪种成分」，重新识别时压过内置规则；
//!   app_key 为空表示不属于任何软件。AI 的不会盖掉用户定过的
//! - `archive_notes`：整理助手记下的用户回答和偏好，每次开场都交代给 AI
//! - `archive_meta`：内置箱子版本、整理模式（ai / template，没走完引导时为空）

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::rules::{BOX_SEEDS, SOFTWARE};

/// 内置箱子的版本：加了新门类的内置箱子时加一，老用户只补一次缺的，删掉的内置箱子不会被加回来
const SEED_VERSION: i64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxRow {
    pub id: String,
    pub name: String,
    pub color: String,
    pub builtin: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserPlacement {
    pub key: String,
    /// None = 放回了待整理
    pub box_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlacement {
    pub key: String,
    pub box_id: String,
    pub confidence: f64,
    #[serde(default)]
    pub reason: String,
}

/// AI 核对一个「像是缓存 / 残留」的位置的结论
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheCheck {
    pub path: String,
    /// safe：删了不丢东西；keep：别删；unsure：看不出来
    pub verdict: String,
    pub reason: String,
}

/// 指定的归属：某个位置属于哪个软件的哪种成分
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaimRow {
    pub path: String,
    /// None：不属于任何软件
    pub app_key: Option<String>,
    /// prog / data / cache / regen / residue
    pub kind: String,
    /// user / ai
    pub source: String,
    pub confidence: f64,
    pub reason: String,
}

pub const CLAIM_KINDS: &[&str] = &["prog", "data", "cache", "regen", "residue"];

pub fn open(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS archive_boxes (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            color TEXT NOT NULL,
            sort INTEGER NOT NULL DEFAULT 0,
            builtin INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS archive_user (
            key TEXT PRIMARY KEY,
            box_id TEXT,
            updated_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS archive_ai (
            key TEXT PRIMARY KEY,
            box_id TEXT NOT NULL,
            confidence REAL NOT NULL,
            reason TEXT NOT NULL DEFAULT '',
            updated_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS archive_check (
            path TEXT PRIMARY KEY COLLATE NOCASE,
            verdict TEXT NOT NULL,
            reason TEXT NOT NULL DEFAULT '',
            checked_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS archive_claim (
            path TEXT PRIMARY KEY COLLATE NOCASE,
            app_key TEXT,
            kind TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1,
            reason TEXT NOT NULL DEFAULT '',
            updated_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS archive_notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            note TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS archive_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| e.to_string())?;
    // 旧库迁移：箱子加门类列（已存在则忽略）
    let _ = conn.execute(&format!("ALTER TABLE archive_boxes ADD COLUMN category TEXT NOT NULL DEFAULT '{SOFTWARE}'"), []);
    seed(conn)
}

/// 首次建库放入全部内置箱子；之后内置箱子有新增时只补一次缺的
fn seed(conn: &Connection) -> Result<(), String> {
    let done: i64 = conn
        .query_row("SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM archive_meta WHERE key = 'seed_version'), 0)", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if done >= SEED_VERSION {
        return Ok(());
    }
    for (i, (id, name, color, category)) in BOX_SEEDS.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO archive_boxes (id, name, color, sort, builtin, category) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![id, name, color, i as i64, category],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "INSERT INTO archive_meta (key, value) VALUES ('seed_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SEED_VERSION.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn boxes(conn: &Connection) -> Result<Vec<BoxRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, color, builtin, category FROM archive_boxes ORDER BY sort, rowid")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(BoxRow {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                builtin: r.get::<_, i64>(3)? != 0,
                category: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn user_placements(conn: &Connection) -> Result<Vec<UserPlacement>, String> {
    let mut stmt = conn.prepare("SELECT key, box_id FROM archive_user").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(UserPlacement { key: r.get(0)?, box_id: r.get(1)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn ai_placements(conn: &Connection) -> Result<Vec<AiPlacement>, String> {
    let mut stmt = conn
        .prepare("SELECT key, box_id, confidence, reason FROM archive_ai")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(AiPlacement { key: r.get(0)?, box_id: r.get(1)?, confidence: r.get(2)?, reason: r.get(3)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn checks(conn: &Connection) -> Result<Vec<CacheCheck>, String> {
    let mut stmt = conn.prepare("SELECT path, verdict, reason FROM archive_check").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(CacheCheck { path: r.get(0)?, verdict: r.get(1)?, reason: r.get(2)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn save_checks(conn: &Connection, items: &[CacheCheck]) -> Result<(), String> {
    for c in items {
        if !matches!(c.verdict.as_str(), "safe" | "keep" | "unsure") {
            continue;
        }
        conn.execute(
            "INSERT INTO archive_check (path, verdict, reason) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET verdict = excluded.verdict, reason = excluded.reason,
               checked_at = datetime('now','localtime')",
            params![c.path, c.verdict, c.reason],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn claims(conn: &Connection) -> Result<Vec<ClaimRow>, String> {
    let mut stmt = conn
        .prepare("SELECT path, app_key, kind, source, confidence, reason FROM archive_claim")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ClaimRow {
                path: r.get(0)?,
                app_key: r.get(1)?,
                kind: r.get(2)?,
                source: r.get(3)?,
                confidence: r.get(4)?,
                reason: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// 记下一条指定的归属；AI 的不盖掉用户定过的。返回是否写进去了
pub fn save_claim(conn: &Connection, c: &ClaimRow) -> Result<bool, String> {
    if !CLAIM_KINDS.contains(&c.kind.as_str()) {
        return Err(format!("不认识的成分：{}", c.kind));
    }
    if !matches!(c.source.as_str(), "user" | "ai") {
        return Err(format!("不认识的来源：{}", c.source));
    }
    if c.path.trim().is_empty() {
        return Err("位置不能为空".into());
    }
    let changed = conn
        .execute(
            "INSERT INTO archive_claim (path, app_key, kind, source, confidence, reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET app_key = excluded.app_key, kind = excluded.kind, source = excluded.source,
               confidence = excluded.confidence, reason = excluded.reason, updated_at = datetime('now','localtime')
             WHERE excluded.source = 'user' OR archive_claim.source = 'ai'",
            params![c.path, c.app_key, c.kind, c.source, c.confidence.clamp(0.0, 1.0), c.reason],
        )
        .map_err(|e| e.to_string())?;
    Ok(changed > 0)
}

/// 撤掉指定的归属，回到内置规则的判断
pub fn delete_claim(conn: &Connection, path: &str) -> Result<(), String> {
    conn.execute("DELETE FROM archive_claim WHERE path = ?1", params![path]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn place(conn: &Connection, keys: &[String], box_id: Option<&str>) -> Result<(), String> {
    if let Some(id) = box_id {
        let exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM archive_boxes WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err("这个箱子已经不存在了".into());
        }
    }
    for key in keys {
        conn.execute(
            "INSERT INTO archive_user (key, box_id) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET box_id = excluded.box_id, updated_at = datetime('now','localtime')",
            params![key, box_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn save_ai(conn: &Connection, items: &[AiPlacement]) -> Result<(), String> {
    for p in items {
        conn.execute(
            "INSERT INTO archive_ai (key, box_id, confidence, reason) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET box_id = excluded.box_id, confidence = excluded.confidence,
               reason = excluded.reason, updated_at = datetime('now','localtime')",
            params![p.key, p.box_id, p.confidence.clamp(0.0, 1.0), p.reason],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn create_box(conn: &Connection, name: &str, color: &str, category: &str) -> Result<BoxRow, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("箱子名字不能为空".into());
    }
    if !matches!(category, "software" | "personal" | "devenv") {
        return Err("不认识的门类".into());
    }
    let id = format!("u{}", uuid::Uuid::new_v4().simple());
    let sort: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM archive_boxes", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO archive_boxes (id, name, color, sort, builtin, category) VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        params![id, name, color, sort, category],
    )
    .map_err(|e| e.to_string())?;
    Ok(BoxRow { id, name: name.to_string(), color: color.to_string(), builtin: false, category: category.to_string() })
}

pub fn rename_box(conn: &Connection, id: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("箱子名字不能为空".into());
    }
    conn.execute("UPDATE archive_boxes SET name = ?1 WHERE id = ?2", params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NoteRow {
    pub id: i64,
    pub note: String,
    pub created_at: String,
}

pub fn notes(conn: &Connection) -> Result<Vec<NoteRow>, String> {
    let mut stmt = conn.prepare("SELECT id, note, created_at FROM archive_notes ORDER BY id").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(NoteRow { id: r.get(0)?, note: r.get(1)?, created_at: r.get(2)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// 最多记几条：每次开场都要交代给 AI，太多了既占地方，也让读到的文件里夹带的话有机会混进来
pub const MAX_NOTES: i64 = 60;

/// 记一条用户的回答 / 偏好；一样的不重复记，太长的截掉
pub fn add_note(conn: &Connection, note: &str) -> Result<(), String> {
    let note: String = note.trim().chars().take(300).collect();
    if note.is_empty() {
        return Err("记下的内容不能为空".into());
    }
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM archive_notes", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if count >= MAX_NOTES {
        return Err(format!("已经记了 {count} 条，记满了；请用户在「AI 记住的」里删掉一些"));
    }
    conn.execute(
        "INSERT INTO archive_notes (note) SELECT ?1 WHERE NOT EXISTS (SELECT 1 FROM archive_notes WHERE note = ?1)",
        params![note],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_note(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM archive_notes WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// 整理模式：ai / template；还没走完引导时为 None
pub fn mode(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM archive_meta WHERE key = 'mode'", [], |r| r.get(0))
        .map(Some)
        .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e.to_string()) })
}

pub fn set_mode(conn: &Connection, mode: &str) -> Result<(), String> {
    if !matches!(mode, "ai" | "template") {
        return Err(format!("不认识的整理模式：{mode}"));
    }
    conn.execute(
        "INSERT INTO archive_meta (key, value) VALUES ('mode', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![mode],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 重新开始整理：清掉装箱结果、AI 的判断、指定的归属、缓存核对、记下的回答和整理模式，箱子恢复成内置的。
/// 扫描摘要不在库里，不受影响
pub fn reset(conn: &Connection) -> Result<(), String> {
    // 一起成功或一起不做：清到一半出错不会留下没有箱子的库
    conn.execute_batch(
        "BEGIN;
         DELETE FROM archive_user; DELETE FROM archive_ai; DELETE FROM archive_claim; DELETE FROM archive_check;
         DELETE FROM archive_notes; DELETE FROM archive_boxes; DELETE FROM archive_meta;",
    )
    .map_err(|e| e.to_string())?;
    match seed(conn) {
        Ok(()) => conn.execute_batch("COMMIT;").map_err(|e| e.to_string()),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

/// 删掉箱子：用户放进去的回到待整理（而不是落回 AI 的猜测）
pub fn delete_box(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("UPDATE archive_user SET box_id = NULL WHERE box_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM archive_ai WHERE box_id = ?1", params![id]).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM archive_boxes WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        open(&conn).unwrap();
        conn
    }

    #[test]
    fn seeds_builtin_boxes_once() {
        let conn = db();
        open(&conn).unwrap();
        let b = boxes(&conn).unwrap();
        assert_eq!(b.len(), BOX_SEEDS.len());
        assert_eq!(b[0].id, "game");
        assert!(b.iter().all(|x| x.builtin));
        assert!(b.iter().any(|x| x.category == "personal") && b.iter().any(|x| x.category == "devenv"));
        // 删掉的内置箱子不会在下次打开时被加回来
        delete_box(&conn, "p-game").unwrap();
        open(&conn).unwrap();
        assert!(boxes(&conn).unwrap().iter().all(|x| x.id != "p-game"));
    }

    #[test]
    fn old_database_gets_new_categories_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE archive_boxes (id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL,
               sort INTEGER NOT NULL DEFAULT 0, builtin INTEGER NOT NULL DEFAULT 0);
             INSERT INTO archive_boxes (id, name, color, builtin) VALUES ('game', '我的游戏', '#000', 1);",
        )
        .unwrap();
        open(&conn).unwrap();
        let b = boxes(&conn).unwrap();
        let game = b.iter().find(|x| x.id == "game").unwrap();
        assert_eq!((game.name.as_str(), game.category.as_str()), ("我的游戏", "software"), "旧箱子保留改过的名字");
        assert!(b.iter().any(|x| x.id == "env-cli"));
    }

    #[test]
    fn user_choice_and_box_lifecycle() {
        let conn = db();
        let mine = create_box(&conn, " 项目 ", "#123456", "personal").unwrap();
        assert_eq!((mine.name.as_str(), mine.category.as_str()), ("项目", "personal"));
        assert!(create_box(&conn, "x", "#000", "nope").is_err());
        place(&conn, &["app:a".into(), "dir:D:\\x".into()], Some(&mine.id)).unwrap();
        save_ai(&conn, &[AiPlacement { key: "app:b".into(), box_id: mine.id.clone(), confidence: 1.7, reason: String::new() }]).unwrap();
        assert!(place(&conn, &["app:c".into()], Some("nope")).is_err());
        assert_eq!(ai_placements(&conn).unwrap()[0].confidence, 1.0);

        delete_box(&conn, &mine.id).unwrap();
        let users = user_placements(&conn).unwrap();
        assert_eq!(users.len(), 2);
        assert!(users.iter().all(|u| u.box_id.is_none()), "删掉箱子后用户放的回到待整理");
        assert!(ai_placements(&conn).unwrap().is_empty());
    }

    #[test]
    fn cache_checks_upsert_and_reject_unknown_verdicts() {
        let conn = db();
        let c = |path: &str, verdict: &str| CacheCheck { path: path.into(), verdict: verdict.into(), reason: "r".into() };
        save_checks(&conn, &[c("C:\\a\\Cache", "safe"), c("C:\\b", "maybe")]).unwrap();
        save_checks(&conn, &[c("c:\\A\\cache", "keep")]).unwrap();
        let all = checks(&conn).unwrap();
        assert_eq!(all.len(), 1, "路径不分大小写，未知结论丢掉");
        assert_eq!(all[0].verdict, "keep");
    }

    #[test]
    fn claims_user_wins_over_ai() {
        let conn = db();
        let c = |path: &str, app: Option<&str>, kind: &str, source: &str| ClaimRow {
            path: path.into(),
            app_key: app.map(str::to_string),
            kind: kind.into(),
            source: source.into(),
            confidence: 0.8,
            reason: String::new(),
        };
        assert!(save_claim(&conn, &c("C:\\a", Some("app:x"), "data", "ai")).unwrap());
        assert!(save_claim(&conn, &c("c:\\A", Some("app:y"), "cache", "user")).unwrap());
        assert!(!save_claim(&conn, &c("C:\\a", Some("app:z"), "data", "ai")).unwrap(), "被用户的挡住了");
        let all = claims(&conn).unwrap();
        assert_eq!(all.len(), 1, "路径不分大小写");
        assert_eq!((all[0].app_key.as_deref(), all[0].source.as_str()), (Some("app:y"), "user"), "AI 不盖掉用户定的");
        save_claim(&conn, &c("C:\\b", None, "data", "user")).unwrap();
        assert!(claims(&conn).unwrap().iter().any(|r| r.path == "C:\\b" && r.app_key.is_none()));
        assert!(save_claim(&conn, &c("C:\\c", Some("app:x"), "junk", "user")).is_err());
        assert!(save_claim(&conn, &c("C:\\c", Some("app:x"), "data", "robot")).is_err());
        delete_claim(&conn, "c:\\a").unwrap();
        assert_eq!(claims(&conn).unwrap().len(), 1);
    }

    #[test]
    fn notes_mode_and_reset() {
        let conn = db();
        assert_eq!(mode(&conn).unwrap(), None, "没走完引导");
        set_mode(&conn, "template").unwrap();
        assert!(set_mode(&conn, "magic").is_err());
        add_note(&conn, " D:\\my files 是杂物，别整个归类 ").unwrap();
        add_note(&conn, "D:\\my files 是杂物，别整个归类").unwrap();
        assert!(add_note(&conn, "  ").is_err());
        assert_eq!(notes(&conn).unwrap().len(), 1, "一样的不重复记");
        let mine = create_box(&conn, "学习", "#123456", "personal").unwrap();
        place(&conn, &["app:a".into()], Some("game")).unwrap();
        rename_box(&conn, "game", "我的游戏").unwrap();
        reset(&conn).unwrap();
        assert_eq!(mode(&conn).unwrap(), None);
        assert!(notes(&conn).unwrap().is_empty() && user_placements(&conn).unwrap().is_empty());
        let b = boxes(&conn).unwrap();
        assert_eq!(b.len(), BOX_SEEDS.len(), "箱子恢复成内置的");
        assert!(b.iter().all(|x| x.id != mine.id) && b.iter().any(|x| x.id == "game" && x.name == "游戏"));
    }
}
