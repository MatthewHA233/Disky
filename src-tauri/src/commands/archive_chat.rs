//! 侧栏 AI 助手里的「整理助手」：每轮开场把档案馆现状交代给 AI，让它主动提问、问清楚再动手。
//!
//! API 方式：带工具的对话（查文件夹、记归属、放进箱子、建箱子、记住用户的话）。
//! Claude Code 方式：现状写进工作目录的 context/archive.md，它要做的事写进 memory/archive-actions.json，
//! 这一轮结束时由这里按同一套规矩执行。

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use super::agent_cli::{ChatSegment, CliStreamEvent};
use super::ai::{ensure_ai_db, AiSettings};
use super::ai_tools::{run_tool_loop, ToolCall, ToolSpec};
use super::archive_agent::Toolbox;
use super::archive_brief::{brief, load_snapshot};
use crate::archive::store;

/// 整理助手的角色和规矩（API 方式放进 system，Claude Code 方式写进 CLAUDE.md）
pub const ROLE: &str = "你是 Disky 档案馆的整理助手，在侧栏和用户对话。Disky 把电脑上的东西分成三个门类：软件（装在电脑上的程序，按使用目的分箱）、\
个人文件（不属于任何软件的文件夹，按内容分箱）、插件与开发环境（运行时、命令行工具、编辑器扩展）。箱子是用户自己的分类，一个东西只进一个箱子；\
软件下面再分成分：prog 程序本体、data 软件数据、cache 缓存、regen 能重新下载的、residue 卸载残留。你的任务是陪用户把整理做完。\n\n\
每轮开场你会拿到「档案馆现状」，先看它再说话。做事的规矩：\n\
- 主动问：从还没整理的东西里挑最值得问的一件（通常是最大的、最拿不准的），一次只问一个问题，给 2 到 4 个选项。用户答了再动手，再问下一个。\n\
- 选项写在回复最后的 options 代码块里，一行一个，Disky 会显示成按钮，用户点了就是他的回答：\n\
```options\n放进「游戏」\n这是我自己的文件\n先跳过\n```\n\
- 拿不准的先查清楚再问：list_dir 看文件夹、file_info 看 exe 的厂商和产品名、read_text 看配置和日志、find_apps 找已认出的软件、installed_apps 查系统登记的安装信息。\
问的时候把查到的依据用一句话告诉用户。\n\
- 用户确认过的才动手：record 记某个位置属于哪个软件的哪种成分；place_item 把软件或文件夹放进箱子；create_box 建用户要的新箱子。\n\
- 用户表达的偏好和决定（比如「D:\\my files 是杂物，别整个归」「游戏都放游戏箱」），马上用 remember 记下来；以后每次开场都会交代给你，务必遵守。\n\
- 现在是模板模式的话，提醒用户之前是按名字猜的，可能有放错的地方，可以一起复查。\n\
- 绝不删除、移动用户的文件；清理由用户自己在 Disky 里点。\n\
- 用中文，简洁，路径写完整。";

const CHAT_ROUNDS: usize = 24;
const HISTORY: i64 = 20;
const BOX_COLORS: &[&str] = &["#CA5010", "#4F6BED", "#00B7C3", "#498205", "#B4009E", "#986F0B", "#E74856", "#0099BC"];
/// 一个门类最多几个箱子（与前端 MAX_TILES 一致，多了界面上摆不下）
const MAX_BOXES: usize = 16;

/// 只有侧栏对话里有的工具
fn chat_specs() -> Vec<ToolSpec> {
    let mut specs = Toolbox::specs();
    specs.push(ToolSpec {
        name: "place_item",
        description: "把软件或文件夹放进箱子（用户确认过才用）。key 是现状里列出的 key（软件 app:…，文件夹 dir:路径）。",
        schema: json!({
            "type": "object",
            "properties": { "key": { "type": "string" }, "box_id": { "type": "string" } },
            "required": ["key", "box_id"]
        }),
    });
    specs.push(ToolSpec {
        name: "create_box",
        description: "按用户的意思建一个新箱子，返回箱子 id。category 是 software、personal 或 devenv。",
        schema: json!({
            "type": "object",
            "properties": { "name": { "type": "string" }, "category": { "type": "string", "enum": ["software", "personal", "devenv"] } },
            "required": ["name", "category"]
        }),
    });
    specs.push(ToolSpec {
        name: "remember",
        description: "记下用户的偏好或决定，一句话。以后每次开场都会交代给你。",
        schema: json!({ "type": "object", "properties": { "note": { "type": "string" } }, "required": ["note"] }),
    });
    specs
}

/// 执行整理助手的工具调用；写库的立刻写（对话中途出错也不丢）
pub struct ChatTools {
    tb: Toolbox,
    conn: Connection,
    /// 箱子 id（每次建箱子后补上）
    boxes: Vec<(String, String)>,
    /// 改过档案馆，前端要重新读
    pub changed: bool,
}

impl ChatTools {
    pub fn new(tb: Toolbox, conn: Connection) -> Result<Self, String> {
        let boxes = store::boxes(&conn)?.into_iter().map(|b| (b.id, b.category)).collect();
        Ok(ChatTools { tb, conn, boxes, changed: false })
    }

    pub fn progress(&self, c: &ToolCall) -> String {
        match c.name.as_str() {
            "place_item" => format!("放进箱子：{}", c.input["key"].as_str().unwrap_or_default()),
            "create_box" => format!("新建箱子：{}", c.input["name"].as_str().unwrap_or_default()),
            "remember" => "记下你说的".into(),
            _ => self.tb.progress(c),
        }
    }

    pub fn call(&mut self, c: &ToolCall) -> String {
        let out = match c.name.as_str() {
            "place_item" => self.place(&c.input),
            "create_box" => self.create_box(&c.input),
            "remember" => self.remember(&c.input),
            "record" => self.record(c),
            _ => Ok(self.tb.call(c)),
        };
        out.unwrap_or_else(|e| format!("出错了：{e}"))
    }

    fn arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
        input[key].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| format!("缺少参数 {key}"))
    }

    fn record(&mut self, c: &ToolCall) -> Result<String, String> {
        let out = self.tb.call(c);
        if let Some(r) = self.tb.records.last().filter(|_| out.starts_with("记下了")) {
            // 按 AI 的判断存（带它自己的把握）：不会盖掉用户定的，也不会绕过「缓存要核对过才能删」
            if store::save_claim(&self.conn, r)? {
                self.changed = true;
            } else {
                return Ok(format!("{out}（用户之前自己定过这个位置，没有改）"));
            }
        }
        Ok(out)
    }

    fn place(&mut self, input: &Value) -> Result<String, String> {
        let key = Self::arg(input, "key")?;
        let box_id = Self::arg(input, "box_id")?;
        if !self.boxes.iter().any(|(id, _)| id == box_id) {
            return Err(format!("没有 id 为 {box_id} 的箱子"));
        }
        // 文件夹的 key 按扫描里的原样拼（大小写、分隔符不同界面上对不上）
        let key = match key.strip_prefix("dir:") {
            Some(path) => self.tb.canonical_dir(path).map(|p| format!("dir:{p}")),
            None => self.tb.apps().iter().any(|a| a.key == key).then(|| key.to_string()),
        }
        .ok_or_else(|| format!("没有 key 为 {key} 的软件或文件夹，用现状里列出的 key"))?;
        store::place(&self.conn, std::slice::from_ref(&key), Some(box_id))?;
        self.changed = true;
        Ok(format!("放好了：{key} → {box_id}"))
    }

    fn create_box(&mut self, input: &Value) -> Result<String, String> {
        let name = Self::arg(input, "name")?;
        let category = Self::arg(input, "category")?;
        let used = self.boxes.iter().filter(|(_, c)| c == category).count();
        if used >= MAX_BOXES {
            return Err(format!("这个门类已经有 {used} 个箱子，放不下了；先和用户商量合并或删掉一些"));
        }
        let row = store::create_box(&self.conn, name, BOX_COLORS[used % BOX_COLORS.len()], category)?;
        self.boxes.push((row.id.clone(), row.category.clone()));
        self.changed = true;
        Ok(format!("建好了箱子「{}」，id={}", row.name, row.id))
    }

    fn remember(&mut self, input: &Value) -> Result<String, String> {
        store::add_note(&self.conn, Self::arg(input, "note")?)?;
        self.changed = true;
        Ok("记下了，以后每次开场都会交代给你".into())
    }
}

/// 读现状、开场交代、工具，读盘的部分放到后台线程
pub async fn prepare(app: &AppHandle) -> Result<(String, ChatTools), String> {
    let handle = app.clone();
    tokio::task::spawn_blocking(move || -> Result<(String, ChatTools), String> {
        let snap = load_snapshot(&handle)?;
        let text = brief(&snap);
        let tb = Toolbox::anywhere(&snap.digests, snap.inv.apps, snap.installed);
        Ok((text, ChatTools::new(tb, super::archive::db(&handle)?)?))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 接口要求 user / assistant 交替、从 user 开始：同一方接连的几条并成一条（上一轮失败留下的），开头的回复丢掉
fn alternate(rows: Vec<(String, String)>) -> Vec<Value> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (role, content) in rows.into_iter().filter(|(_, c)| !c.trim().is_empty()) {
        match out.last_mut() {
            Some((r, c)) if *r == role => {
                c.push_str("\n\n");
                c.push_str(&content);
            }
            _ => out.push((role, content)),
        }
    }
    let start = out.iter().position(|(r, _)| r == "user").unwrap_or(out.len());
    out.drain(start..).map(|(role, content)| json!({ "role": role, "content": content })).collect()
}

fn emit(app: &AppHandle, ev: CliStreamEvent) {
    let _ = app.emit("chat-stream", ev);
}

/// API 方式的一轮对话：带工具，工具调用以卡片显示在侧栏，结束后存消息
pub async fn run_api_chat(app: &AppHandle, settings: &AiSettings, message: &str, session_id: i64, disk_context: &str) -> Result<(), String> {
    let fail = |e: String| {
        emit(app, CliStreamEvent { done: true, error: Some(e.clone()), ..Default::default() });
        e
    };
    if settings.api_key.is_empty() {
        return Err(fail("还没有配置 AI：请在 AI 设置里填写 API 密钥".into()));
    }
    let history: Vec<Value> = {
        let conn = ensure_ai_db(app)?;
        conn.execute("INSERT INTO chat_messages (role, content, session_id) VALUES ('user', ?1, ?2)", params![message, session_id])
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM (SELECT role, content, id FROM chat_messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2) ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id, HISTORY], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        alternate(rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?)
    };
    emit(app, CliStreamEvent { kind: Some("status".into()), status: Some("正在看档案馆现状".into()), ..Default::default() });
    let (text, mut tools) = prepare(app).await.map_err(fail)?;
    let system = format!("{ROLE}\n\n{text}\n\n# 当前打开的扫描\n{disk_context}");
    let mut segments: Vec<ChatSegment> = Vec::new();
    let mut call = |c: &ToolCall| {
        let input = c.input.to_string();
        emit(app, CliStreamEvent {
            kind: Some("tool_use".into()),
            tool_id: Some(c.id.clone()),
            tool_name: Some(tools.progress(c)),
            tool_input: Some(input.clone()),
            ..Default::default()
        });
        let out = tools.call(c);
        let is_error = out.starts_with("出错了");
        emit(app, CliStreamEvent {
            kind: Some("tool_result".into()),
            tool_id: Some(c.id.clone()),
            tool_result: Some(out.chars().take(2000).collect()),
            tool_error: Some(is_error),
            ..Default::default()
        });
        segments.push(ChatSegment::Tool { id: c.id.clone(), name: tools.progress(c), input, result: Some(out.chars().take(2000).collect()), is_error });
        out
    };
    let outcome = run_tool_loop(settings, &system, history, &chat_specs(), CHAT_ROUNDS, &mut call).await;
    if tools.changed {
        let _ = app.emit("archive-changed", ());
    }
    let reply = outcome.map_err(fail)?;
    // 只调了工具没说话的也存一句，免得下一轮的历史里接连两条用户消息
    let reply = if reply.trim().is_empty() { "（查完了，这一轮没有要说的。）".to_string() } else { reply };
    emit(app, CliStreamEvent { delta: reply.clone(), kind: Some("text".into()), ..Default::default() });
    let meta = (!segments.is_empty()).then(|| serde_json::to_string(&segments).ok()).flatten();
    ensure_ai_db(app)?
        .execute("INSERT INTO chat_messages (role, content, meta, session_id) VALUES ('assistant', ?1, ?2, ?3)", params![reply, meta, session_id])
        .map_err(|e| e.to_string())?;
    emit(app, CliStreamEvent { done: true, ..Default::default() });
    Ok(())
}

/// Claude Code 这一轮写下的动作（memory/archive-actions.json：[{"tool":"place_item","key":…,"box_id":…}, …]），
/// 按同一套规矩执行；返回每条的结果，给下一轮看
pub fn apply_cli_actions(tools: &mut ChatTools, text: &str) -> Vec<String> {
    let Ok(actions) = serde_json::from_str::<Vec<Value>>(text) else { return vec!["archive-actions.json 不是 JSON 数组，没有执行".into()] };
    actions
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = a["tool"].as_str().unwrap_or_default();
            let allowed = matches!(name, "record" | "place_item" | "create_box" | "remember");
            let out = if allowed { tools.call(&ToolCall { id: format!("cli_{i}"), name: name.into(), input: a.clone() }) } else { format!("不认识的动作 {name}") };
            format!("{name}：{out}")
        })
        .collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::archive::digest::{DigestEntry, DriveDigest};

    fn tools() -> ChatTools {
        let conn = Connection::open_in_memory().unwrap();
        store::open(&conn).unwrap();
        let digests = vec![DriveDigest {
            version: 3,
            root: "D:".into(),
            scanned_at: 1,
            total: 1 << 40,
            modified_at: 1,
            entries: vec![DigestEntry { path: "D:\\stuff".into(), size: 1 << 30, depth: 1, is_dir: true, exe_depth: 255, exes: Vec::new() }],
        }];
        ChatTools::new(Toolbox::anywhere(&digests, Vec::new(), Vec::new()), conn).unwrap()
    }

    #[test]
    fn chat_tools_validate_and_write() {
        let mut t = tools();
        let call = |t: &mut ChatTools, name: &str, input: Value| t.call(&ToolCall { id: "1".into(), name: name.into(), input });
        assert!(call(&mut t, "place_item", json!({ "key": "dir:D:\\stuff", "box_id": "p-docs" })).starts_with("放好了"));
        assert!(call(&mut t, "place_item", json!({ "key": "dir:D:\\nope", "box_id": "p-docs" })).contains("没有 key"));
        assert!(call(&mut t, "place_item", json!({ "key": "dir:D:\\stuff", "box_id": "nope" })).contains("没有 id"));
        let made = call(&mut t, "create_box", json!({ "name": "学习资料", "category": "personal" }));
        let id = made.rsplit("id=").next().unwrap().to_string();
        assert!(call(&mut t, "place_item", json!({ "key": "dir:D:\\stuff", "box_id": id })).starts_with("放好了"), "刚建的箱子能用");
        assert!(call(&mut t, "remember", json!({ "note": "stuff 是学习资料" })).starts_with("记下了"));
        assert_eq!(store::notes(&t.conn).unwrap().len(), 1);
        assert!(t.changed);
        let out = apply_cli_actions(&mut t, r#"[{"tool":"remember","note":"再记一条"},{"tool":"delete_everything"}]"#);
        assert!(out[0].contains("记下了") && out[1].contains("不认识"));
        assert!(apply_cli_actions(&mut t, "oops")[0].contains("不是 JSON"));
        // 大小写、分隔符不同的 key 按扫描里的原样存
        assert!(call(&mut t, "place_item", json!({ "key": "dir:d:/STUFF/", "box_id": "p-docs" })).contains("dir:D:\\stuff"));
    }

    #[test]
    fn history_alternates_starting_from_user() {
        let r = |role: &str, c: &str| (role.to_string(), c.to_string());
        let msgs = alternate(vec![r("assistant", "上一轮的回复"), r("user", "第一句"), r("user", "失败后又问"), r("assistant", ""), r("assistant", "好的"), r("user", "继续")]);
        let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "user"]);
        assert_eq!(msgs[0]["content"], "第一句\n\n失败后又问");
    }
}
