//! 让 AI 自己查「没认出归属的文件夹属于哪个软件」：内置一套只读工具给 AI 调用，
//! AI 查清楚了用 record 记下结论，存成 AI 指定的归属（把握够的才会被识别采用，用户可以改）。
//!
//! API 方式走工具调用；Claude Code 方式用它自带的只读工具（Read / Glob / Grep）看文件，最后回 JSON。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use super::ai_tools::{run_tool_loop, ToolCall, ToolSpec};
use crate::archive::digest::DriveDigest;
use crate::archive::inventory::{self, AppItem, Env};
use crate::archive::registry::{self, InstalledApp};
use crate::archive::store::{self, ClaimRow, CLAIM_KINDS};
use crate::archive::{digest, exe_info};
use crate::scanner::{is_strict_ancestor, normalize_path, path_file_name, SEP};

/// 一次最多查几个文件夹、AI 最多来回几轮
const MAX_PATHS: usize = 30;
const MAX_ROUNDS: usize = 40;
const LIST_LIMIT: usize = 60;
const TEXT_LIMIT: usize = 2000;
const TEXT_EXTS: &[&str] = &["json", "ini", "xml", "cfg", "conf", "toml", "yaml", "yml", "plist", "properties", "log", "manifest"];

const SYSTEM: &str = "你是磁盘整理工具 Disky 里的助手。用户电脑上有一些文件夹，Disky 还没认出它们属于哪个软件。\
你的任务是用工具查清楚，把能确定的结论用 record 记下来。\n\
- 先用 find_apps 找候选软件；只能记到 find_apps 返回过的软件 key 上。\n\
- 用 list_dir 看文件夹里有什么，file_info 看 exe 的厂商和产品名，read_text 看配置文件和日志，installed_apps 查系统登记的安装信息。\n\
- 一个文件夹里混着几个软件的东西，就往里看，分别记子文件夹；只能记交给你的文件夹或它们里面的位置。\n\
- 用户自己的文档、照片、项目代码这类不属于任何软件的，不要记。\n\
- 认得出是某个软件、但它不在 find_apps 的结果里（没装或没认出），不要记，在最后的总结里说一句。\n\
- kind：prog 程序本体；data 软件数据（设置、存档、聊天记录）；cache 删了会自动重建的缓存；regen 能重新下载的（游戏本体、模型、依赖包）；residue 卸载残留、装完没用的安装包。拿不准是不是缓存就记 data。\n\
- confidence 取 0 到 1；只是猜的给 0.5 以下（不会被采用）。reason 用一句话写依据。\n\
- 不要编造路径，只记查过的位置。查完用一两句话总结。";

pub struct Toolbox {
    /// 小写路径 → (大小, 里面直接放着的 exe)
    dirs: HashMap<String, (u64, Vec<String>)>,
    /// 小写父路径 → 子目录（路径, 大小）
    kids: HashMap<String, Vec<(String, u64)>>,
    /// 小写路径 → 扫描里的原样路径
    originals: HashMap<String, String>,
    apps: Vec<AppItem>,
    installed: Vec<InstalledApp>,
    /// 交给 AI 查的文件夹（小写）：只能看、只能记它们自己或里面的位置。
    /// None：侧栏对话里用，用户在和它聊哪儿就能看哪儿（网络路径、凭据文件照样不给看）
    scope: Option<Vec<String>>,
    /// 已认出软件的位置（小写）：可以看，拿来和要查的文件夹对照
    known: Vec<String>,
    pub records: Vec<ClaimRow>,
}

/// 名字像凭据、令牌的文件不给读
const SENSITIVE: &[&str] = &[
    "credential", "secret", "token", "password", "passwd", "cookie", "auth", "session", "wallet", "netrc", ".env", "hosts.yml",
    "sitemanager", "recentservers", "winscp", "rclone", "npmrc", "pypirc", "id_rsa", "id_ed25519", "private",
];

/// 存密钥、登录信息的目录：整个不给看
const SECRET_DIRS: &[&str] = &[".ssh", ".aws", ".azure", ".kube", ".gnupg", ".docker", ".m2", "filezilla", "winscp", "rclone", "credentials", "protect", "vault"];

fn lower(p: &str) -> String {
    p.to_ascii_lowercase()
}

fn parent(p: &str) -> Option<&str> {
    p.rsplit_once(SEP).map(|(h, _)| h).filter(|h| !h.is_empty())
}

fn human(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 1024.0 * MB {
        format!("{:.1} GB", b / 1024.0 / MB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{:.0} KB", (b / 1024.0).max(1.0))
    }
}

/// 年月日（UTC）
fn ymd(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // Howard Hinnant 的 civil_from_days
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input[key].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| format!("缺少参数 {key}"))
}

fn ext_of(path: &str) -> String {
    path_file_name(path).rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

impl Toolbox {
    pub fn new(digests: &[DriveDigest], apps: Vec<AppItem>, installed: Vec<InstalledApp>, paths: &[String]) -> Self {
        Self::with_scope(digests, apps, installed, Some(paths.iter().map(|p| lower(&normalize_path(p))).collect()))
    }

    /// 侧栏对话用：不限定在几个文件夹里
    pub fn anywhere(digests: &[DriveDigest], apps: Vec<AppItem>, installed: Vec<InstalledApp>) -> Self {
        Self::with_scope(digests, apps, installed, None)
    }

    fn with_scope(digests: &[DriveDigest], apps: Vec<AppItem>, installed: Vec<InstalledApp>, scope: Option<Vec<String>>) -> Self {
        let mut dirs = HashMap::new();
        let mut kids: HashMap<String, Vec<(String, u64)>> = HashMap::new();
        let mut originals = HashMap::new();
        for e in digests.iter().flat_map(|d| d.entries.iter()).filter(|e| e.is_dir) {
            dirs.insert(lower(&e.path), (e.size, e.exes.clone()));
            originals.insert(lower(&e.path), e.path.clone());
            if let Some(p) = parent(&e.path) {
                kids.entry(lower(p)).or_default().push((e.path.clone(), e.size));
            }
        }
        kids.values_mut().for_each(|v| v.sort_by(|a, b| b.1.cmp(&a.1)));
        let known = apps.iter().flat_map(|a| a.parts.iter().map(|p| lower(&p.path))).collect();
        Toolbox { dirs, kids, originals, apps, installed, scope, known, records: Vec::new() }
    }

    pub fn apps(&self) -> &[AppItem] {
        &self.apps
    }

    /// 摘要里这个目录的原样路径（大小写、分隔符以扫描为准）；太小、太深的没有
    pub fn canonical_dir(&self, path: &str) -> Option<String> {
        self.originals.get(&lower(&normalize_path(path))).cloned()
    }

    /// 读的工具只能看交给 AI 的文件夹和已认出软件的位置；网络路径、设备路径、带 .. 的一律不认
    fn readable(&self, raw: &str) -> Result<String, String> {
        let p = normalize_path(raw);
        if p.starts_with("\\\\") || p.starts_with("//") || p.split(SEP).any(|s| s == "..") {
            return Err(format!("{raw} 不能看"));
        }
        let l = lower(&p);
        if l.split(SEP).any(|s| SECRET_DIRS.contains(&s)) {
            return Err(format!("{p} 是存密钥、登录信息的地方，不看"));
        }
        let inside = |s: &String| *s == l || is_strict_ancestor(s, &l);
        let Some(scope) = &self.scope else { return Ok(p) };
        if scope.iter().any(inside) || self.known.iter().any(inside) {
            Ok(p)
        } else {
            Err(format!("{p} 不在能看的范围里：只能看交给你查的文件夹和已认出软件的位置"))
        }
    }

    pub fn specs() -> Vec<ToolSpec> {
        let path = json!({ "type": "object", "properties": { "path": { "type": "string", "description": "完整路径" } }, "required": ["path"] });
        let query = json!({ "type": "object", "properties": { "query": { "type": "string", "description": "名字、发行商或路径里的一段" } }, "required": ["query"] });
        vec![
            ToolSpec { name: "list_dir", description: "列出一个目录里的子目录和文件：名字、大小、修改日期，子目录里直接放着哪些 exe。", schema: path.clone() },
            ToolSpec { name: "file_info", description: "看一个文件的大小、修改日期；exe / dll 还会给出版本信息里的厂商、产品名、文件说明。", schema: path.clone() },
            ToolSpec { name: "read_text", description: "读一个配置文件或日志开头的一段（json、ini、xml、cfg、toml、yaml、log 等）。", schema: path },
            ToolSpec { name: "find_apps", description: "在 Disky 已经认出的软件里按名字、发行商或位置找，返回软件 key、名字、发行商和它已有的位置。", schema: query.clone() },
            ToolSpec { name: "installed_apps", description: "查系统登记的已安装软件（卸载信息里的名字、发行商、安装位置）。", schema: query },
            ToolSpec {
                name: "record",
                description: "记下一条结论：某个位置属于哪个软件的哪种成分。位置正好是另一个软件条目自己的文件夹时，那个条目整个并进 app_key 指的软件。",
                schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "app_key": { "type": "string", "description": "find_apps 返回的软件 key" },
                        "kind": { "type": "string", "enum": CLAIM_KINDS },
                        "confidence": { "type": "number" },
                        "reason": { "type": "string" }
                    },
                    "required": ["path", "app_key", "kind", "confidence", "reason"]
                }),
            },
        ]
    }

    /// 给用户看的进度
    pub fn progress(&self, c: &ToolCall) -> String {
        let path = c.input["path"].as_str().unwrap_or_default();
        let query = c.input["query"].as_str().unwrap_or_default();
        match c.name.as_str() {
            "list_dir" => format!("看看 {path} 里面有什么"),
            "file_info" => format!("读 {} 的版本信息", path_file_name(path)),
            "read_text" => format!("读 {}", path_file_name(path)),
            "find_apps" => format!("找名字像「{query}」的软件"),
            "installed_apps" => format!("查系统登记的「{query}」"),
            "record" => format!("记下 {path}"),
            other => format!("调用 {other}"),
        }
    }

    pub fn call(&mut self, c: &ToolCall) -> String {
        let out = match c.name.as_str() {
            "list_dir" => str_arg(&c.input, "path").and_then(|p| self.readable(p)).map(|p| self.list_dir(&p)),
            "file_info" => str_arg(&c.input, "path").and_then(|p| self.readable(p)).map(|p| file_info(&p)),
            "read_text" => str_arg(&c.input, "path").and_then(|p| self.readable(p)).map(|p| read_text(&p)),
            "find_apps" => str_arg(&c.input, "query").map(|q| self.find_apps(q)),
            "installed_apps" => str_arg(&c.input, "query").map(|q| self.installed_apps(q)),
            "record" => self.record(&c.input),
            other => Err(format!("没有叫 {other} 的工具")),
        };
        out.unwrap_or_else(|e| format!("出错了：{e}"))
    }

    fn list_dir(&self, path: &str) -> String {
        let rd = match std::fs::read_dir(path) {
            Ok(rd) => rd,
            Err(e) => return format!("打不开 {path}：{e}"),
        };
        let mut rows: Vec<(bool, u64, String)> = Vec::new();
        for ent in rd.flatten().take(2000) {
            let Ok(meta) = ent.metadata() else { continue };
            let full = ent.path().to_string_lossy().to_string();
            let name = ent.file_name().to_string_lossy().to_string();
            let date = meta.modified().map(ymd).unwrap_or_default();
            let line = if meta.is_dir() {
                let (size, exes) = self.dirs.get(&lower(&full)).cloned().unwrap_or_default();
                let exes = if exes.is_empty() { String::new() } else { format!("  里面的 exe：{}", exes.iter().take(4).cloned().collect::<Vec<_>>().join("、")) };
                (true, size, format!("目录  {name}  {}  {date}{exes}", if size > 0 { human(size) } else { "小于 1 MB".into() }))
            } else {
                (false, meta.len(), format!("文件  {name}  {}  {date}", human(meta.len())))
            };
            rows.push(line);
        }
        let (dirs, files) = (rows.iter().filter(|r| r.0).count(), rows.iter().filter(|r| !r.0).count());
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let shown: Vec<String> = rows.into_iter().take(LIST_LIMIT).map(|r| r.2).collect();
        let more = if dirs + files > LIST_LIMIT { format!("（只列最大的 {LIST_LIMIT} 个）") } else { String::new() };
        format!("{path}：{dirs} 个子目录，{files} 个文件{more}\n{}", shown.join("\n"))
    }

    fn find_apps(&self, query: &str) -> String {
        let q = query.to_lowercase();
        let hits: Vec<String> = self
            .apps
            .iter()
            .filter(|a| {
                a.name.to_lowercase().contains(&q)
                    || a.publisher.to_lowercase().contains(&q)
                    || a.parts.iter().any(|p| path_file_name(&p.path).to_lowercase().contains(&q))
            })
            .take(15)
            .map(|a| {
                let mut parts: Vec<&inventory::Part> = a.parts.iter().collect();
                parts.sort_by(|x, y| y.size.cmp(&x.size));
                let at: Vec<&str> = parts.iter().take(3).map(|p| p.path.as_str()).collect();
                let publisher = if a.publisher.is_empty() { String::new() } else { format!("  发行商：{}", a.publisher) };
                format!("key={}  {}{publisher}  位置：{}", a.key, a.name, at.join("；"))
            })
            .collect();
        if hits.is_empty() {
            format!("已认出的软件里没有名字、发行商或位置带「{query}」的")
        } else {
            hits.join("\n")
        }
    }

    fn installed_apps(&self, query: &str) -> String {
        let q = query.to_lowercase();
        let hits: Vec<String> = self
            .installed
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&q) || a.publisher.to_lowercase().contains(&q))
            .take(15)
            .map(|a| {
                format!(
                    "{}  发行商：{}  安装位置：{}  图标：{}",
                    a.name,
                    a.publisher,
                    a.location.as_deref().unwrap_or("（没登记）"),
                    a.icon.as_deref().unwrap_or("（没登记）")
                )
            })
            .collect();
        if hits.is_empty() {
            format!("系统登记的软件里没有「{query}」")
        } else {
            hits.join("\n")
        }
    }

    fn in_scope(&self, path: &str) -> bool {
        let l = lower(path);
        self.scope.as_ref().is_none_or(|scope| scope.iter().any(|s| *s == l || is_strict_ancestor(s, &l)))
    }

    fn record(&mut self, input: &Value) -> Result<String, String> {
        let path = normalize_path(str_arg(input, "path")?);
        let key = str_arg(input, "app_key")?;
        let kind = str_arg(input, "kind")?;
        if path.split(SEP).any(|s| s == ".." || s.is_empty()) || !self.in_scope(&path) {
            return Err(format!("{path} 不在交给你查的文件夹里"));
        }
        // 只有扫描摘要里有的位置才会被识别采用（太小、太深的记了也没用）
        if !self.dirs.contains_key(&lower(&path)) {
            return Err(format!("{path} 太小或者不存在，记不了；记它的上一级"));
        }
        let app = self.apps.iter().find(|a| a.key == key).ok_or_else(|| format!("没有 key 为 {key} 的软件，先用 find_apps 找"))?;
        if !CLAIM_KINDS.contains(&kind) {
            return Err(format!("kind 只能是 {}", CLAIM_KINDS.join("、")));
        }
        let row = ClaimRow {
            path: path.clone(),
            app_key: Some(app.key.clone()),
            kind: kind.to_string(),
            source: "ai".into(),
            confidence: input["confidence"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0),
            reason: input["reason"].as_str().unwrap_or_default().chars().take(60).collect(),
        };
        let msg = format!("记下了：{path} → {}（{kind}）", app.name);
        self.records.retain(|r| lower(&r.path) != lower(&path));
        self.records.push(row);
        Ok(msg)
    }

    /// 交给 AI 的任务说明：要查的文件夹、各自里面最大的几项
    pub fn brief(&self, paths: &[String]) -> String {
        let lines: Vec<String> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let size = self.dirs.get(&lower(p)).map_or(0, |d| d.0);
                let inside: Vec<String> = self.kids.get(&lower(p)).map_or_else(Vec::new, |k| k.iter().take(6).map(|(c, _)| path_file_name(c)).collect());
                let inside = if inside.is_empty() { String::new() } else { format!("；里面最大的：{}", inside.join("、")) };
                format!("{}. {p}（{}{inside}）", i + 1, human(size))
            })
            .collect();
        format!("需要查的文件夹（{} 个）：\n{}", paths.len(), lines.join("\n"))
    }

    /// Claude Code 方式用不了 find_apps，把已认出的软件整个列给它
    fn catalog(&self) -> String {
        let lines: Vec<String> = self
            .apps
            .iter()
            .map(|a| format!("{} | {} | {} | {}", a.key, a.name, a.publisher, a.parts.first().map_or("", |p| p.path.as_str())))
            .collect();
        format!("已认出的软件（key | 名字 | 发行商 | 一个位置）：\n{}", lines.join("\n"))
    }

    /// Claude Code 的回复里取出 JSON 数组（从每个「[」试着解析，前后夹着说明文字也行），逐条按 record 的规矩检查
    fn take_json_records(&mut self, text: &str) -> Result<usize, String> {
        let rows = text
            .match_indices('[')
            .find_map(|(i, _)| serde_json::Deserializer::from_str(&text[i..]).into_iter::<Vec<Value>>().next()?.ok())
            .ok_or("Claude Code 的回复里没有能解析的结果")?;
        Ok(rows.iter().filter(|r| self.record(r).is_ok()).count())
    }
}

fn file_info(path: &str) -> String {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return format!("打不开 {path}：{e}"),
    };
    let mut out = format!("{path}：{}，修改于 {}", human(meta.len()), meta.modified().map(ymd).unwrap_or_default());
    if matches!(ext_of(path).as_str(), "exe" | "dll") {
        match exe_info::read(path) {
            Some(i) => out.push_str(&format!("\n厂商：{}\n产品：{}\n说明：{}\n版本：{}", i.company, i.product, i.description, i.version)),
            None => out.push_str("\n没有版本信息"),
        }
    }
    out
}

fn read_text(path: &str) -> String {
    if !TEXT_EXTS.contains(&ext_of(path).as_str()) {
        return format!("只能读配置文件和日志（{}）", TEXT_EXTS.join("、"));
    }
    let name = path_file_name(path).to_lowercase();
    if SENSITIVE.iter().any(|w| name.contains(w)) {
        return format!("{name} 像是存放凭据的文件，不读");
    }
    use std::io::Read;
    let mut buf = vec![0u8; TEXT_LIMIT * 4];
    let n = match std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)) {
        Ok(n) => n,
        Err(e) => return format!("打不开 {path}：{e}"),
    };
    let text: String = String::from_utf8_lossy(&buf[..n]).chars().take(TEXT_LIMIT).collect();
    format!("{path} 开头：\n{text}")
}

#[derive(Serialize)]
pub struct AttributeResult {
    records: Vec<ClaimRow>,
    summary: String,
}

/// 让 AI 查一批没认出归属的文件夹，结论存成 AI 指定的归属
#[tauri::command]
pub async fn archive_ai_attribute(app: AppHandle, paths: Vec<String>) -> Result<AttributeResult, String> {
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).take(MAX_PATHS).collect();
    if paths.is_empty() {
        return Ok(AttributeResult { records: Vec::new(), summary: String::new() });
    }
    let settings = super::ai::load_ai_settings(app.clone())?;
    let handle = app.clone();
    let (digests, installed, apps) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let digests = digest::load_all(&super::archive::digest_dir(&handle)?);
        let installed = registry::installed_apps();
        let claims = store::claims(&super::archive::db(&handle)?)?;
        let inv = inventory::build(&digests, &installed, &Env::current(), &super::archive::overrides(claims));
        Ok((digests, installed, inv.apps))
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut tb = Toolbox::new(&digests, apps, installed, &paths);
    drop(digests);
    let brief = tb.brief(&paths);
    let outcome = if settings.provider == "claude-code" {
        let _ = app.emit("archive-agent", json!({ "text": "Claude Code 正在查这些文件夹" }));
        let prompt = format!(
            "{SYSTEM}\n\n你没有 find_apps / record 工具：用你自带的 Read、Glob、Grep 去看文件夹，软件 key 从下面的列表里选。\
            查完只输出 JSON 数组，不要其他文字：[{{\"path\":\"...\",\"app_key\":\"...\",\"kind\":\"data\",\"confidence\":0.9,\"reason\":\"...\"}}]\n\n{}\n\n{brief}",
            tb.catalog()
        );
        // 只给只读工具，再把要查的文件夹加进它能看的范围（默认只能看工作目录）
        let mut extra: Vec<String> = ["--allowedTools", "Read,Glob,Grep", "--disallowedTools", "Bash,Write,Edit,MultiEdit,NotebookEdit,WebFetch,WebSearch", "--add-dir"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        extra.extend(paths.iter().cloned());
        super::agent_cli::prompt_once_cli_with(&settings, &prompt, None, &extra, std::time::Duration::from_secs(600))
            .await
            .and_then(|text| tb.take_json_records(&text).map(|n| format!("Claude Code 认出了 {n} 处")))
    } else {
        let mut call = |c: &ToolCall| {
            let _ = app.emit("archive-agent", json!({ "text": tb.progress(c) }));
            tb.call(c)
        };
        run_tool_loop(&settings, SYSTEM, vec![json!({ "role": "user", "content": brief })], &Toolbox::specs(), MAX_ROUNDS, &mut call).await
    };
    // 中途出错也把已经记下的存好；被用户定过的挡住的不算
    let conn = super::archive::db(&app)?;
    let mut saved = Vec::new();
    for r in tb.records {
        if store::save_claim(&conn, &r)? {
            saved.push(r);
        }
    }
    let summary = match outcome {
        Ok(s) => s,
        Err(e) if !saved.is_empty() => format!("中途出错了（{e}），已经查出的 {} 处保留了", saved.len()),
        Err(e) => return Err(e),
    };
    Ok(AttributeResult { records: saved, summary: summary.chars().take(400).collect() })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::archive::digest::DigestEntry;
    use crate::scanner::classify::Kind;

    fn dir(path: &str, size: u64, exes: &[&str]) -> DigestEntry {
        DigestEntry { path: path.into(), size, depth: path.matches('\\').count() as u8, is_dir: true, exe_depth: 0, exes: exes.iter().map(|s| s.to_string()).collect() }
    }

    fn toolbox() -> Toolbox {
        let digests = vec![DriveDigest {
            version: 3,
            root: "D:".into(),
            scanned_at: 1,
            total: 1 << 40,
            modified_at: 1,
            entries: vec![dir("D:\\ghidra_data", 3 << 30, &[]), dir("D:\\ghidra_data\\proj", 2 << 30, &[]), dir("D:\\Tools\\Ghidra", 1 << 29, &["ghidraRun.bat"])],
        }];
        let app = AppItem {
            key: "app:d:\\tools\\ghidra".into(),
            name: "Ghidra".into(),
            publisher: "NSA".into(),
            icon: None,
            uninstalled: false,
            hint: None,
            category: "software".into(),
            parts: vec![inventory::Part {
                path: "D:\\Tools\\Ghidra".into(),
                kind: Kind::Prog,
                size: 1 << 29,
                deletable: true,
                contents_only: false,
                origin: None,
                reason: None,
            }],
        };
        Toolbox::new(&digests, vec![app], Vec::new(), &["D:\\ghidra_data".into()])
    }

    fn call(tb: &mut Toolbox, name: &str, input: Value) -> String {
        tb.call(&ToolCall { id: "1".into(), name: name.into(), input })
    }

    #[test]
    fn record_checks_scope_app_and_kind() {
        let mut tb = toolbox();
        assert!(call(&mut tb, "find_apps", json!({ "query": "ghidra" })).contains("key=app:d:\\tools\\ghidra"));
        let ok = json!({ "path": "D:\\ghidra_data\\proj", "app_key": "app:d:\\tools\\ghidra", "kind": "data", "confidence": 0.9, "reason": "Ghidra 的工程目录" });
        assert!(call(&mut tb, "record", ok.clone()).starts_with("记下了"));
        assert!(call(&mut tb, "record", json!({ "path": "D:\\Tools\\Ghidra", "app_key": "app:d:\\tools\\ghidra", "kind": "prog" })).contains("不在交给你查"));
        assert!(call(&mut tb, "record", json!({ "path": "D:\\ghidra_data\\..\\Tools", "app_key": "app:d:\\tools\\ghidra", "kind": "data" })).contains("不在交给你查"));
        assert!(call(&mut tb, "record", json!({ "path": "D:\\ghidra_data\\tiny", "app_key": "app:d:\\tools\\ghidra", "kind": "data" })).contains("太小"));
        assert!(call(&mut tb, "record", json!({ "path": "D:\\ghidra_data", "app_key": "app:nope", "kind": "data" })).contains("没有 key"));
        assert!(call(&mut tb, "record", json!({ "path": "D:\\ghidra_data", "app_key": "app:d:\\tools\\ghidra", "kind": "junk" })).contains("kind 只能是"));
        assert!(call(&mut tb, "list_dir", json!({})).contains("缺少参数 path"));
        assert_eq!(tb.records.len(), 1);
        assert_eq!((tb.records[0].source.as_str(), tb.records[0].confidence), ("ai", 0.9));
    }

    #[test]
    fn brief_and_cli_reply() {
        let mut tb = toolbox();
        assert!(tb.brief(&["D:\\ghidra_data".into()]).contains("D:\\ghidra_data（3.0 GB；里面最大的：proj）"));
        let reply = "查完了：\n[{\"path\":\"D:\\\\ghidra_data\",\"app_key\":\"app:d:\\\\tools\\\\ghidra\",\"kind\":\"data\",\"confidence\":0.8,\"reason\":\"工程\"},{\"path\":\"C:\\\\x\",\"app_key\":\"a\",\"kind\":\"data\"}]";
        assert_eq!(tb.take_json_records(reply), Ok(1));
        assert!(tb.take_json_records("[说明] 没有结果").is_err());
    }

    #[test]
    fn read_tools_stay_in_scope() {
        let tb = toolbox();
        assert_eq!(tb.readable("D:/ghidra_data/proj/").as_deref(), Ok("D:\\ghidra_data\\proj"));
        assert!(tb.readable("D:\\Tools\\Ghidra\\ghidraRun.bat").is_ok(), "已认出软件的位置可以看");
        assert!(tb.readable("D:\\ghidra_data\\..\\Windows").is_err());
        assert!(tb.readable("\\\\evil.example\\share").is_err());
        assert!(tb.readable("C:\\Users\\me\\.claude").is_err());
        let anywhere = Toolbox::anywhere(&[], Vec::new(), Vec::new());
        assert!(anywhere.readable("C:\\Users\\me\\AppData\\Roaming\\Claude").is_ok(), "对话里不限文件夹");
        assert!(anywhere.readable("C:\\Users\\me\\.ssh\\config").is_err());
        assert!(anywhere.readable("C:\\Users\\me\\AppData\\Roaming\\FileZilla").is_err());
        assert!(read_text("D:\\ghidra_data\\auth.json").contains("不读"));
    }

    #[test]
    fn lists_real_directory_and_formats_dates() {
        let root = std::env::temp_dir().join(format!("disky-agent-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("config.json"), "{\"app\":\"demo\"}").unwrap();
        let tb = toolbox();
        let out = tb.list_dir(&root.to_string_lossy());
        assert!(out.contains("1 个子目录，1 个文件") && out.contains("文件  config.json"), "{out}");
        assert!(read_text(&root.join("config.json").to_string_lossy()).contains("demo"));
        assert!(read_text("C:\\x\\notes.txt").starts_with("只能读"));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(ymd(UNIX_EPOCH), "1970-01-01");
        assert_eq!(ymd(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)), "2023-11-14");
    }
}
