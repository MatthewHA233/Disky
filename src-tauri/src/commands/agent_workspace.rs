//! Claude Code 受管工作目录 — 对应 open-design 把 SKILL.md/DESIGN.md 写进
//! `.od/projects/<id>/` 再在那里 spawn CLI 的机制：
//! cwd 指向本目录后，Claude Code 自动加载 `CLAUDE.md`（角色指令）和
//! `.claude/skills/`（清理分析技能）；动态上下文（深层扫描摘要、用户数据）
//! 每轮刷新为 `context/*.md`，由模型按需 Read，避免撑爆 prompt。

use std::path::PathBuf;

use tauri::Manager;

use crate::commands::ai::ensure_ai_db;
use crate::scanner::TreeNode;

const CLAUDE_MD: &str = r#"# Disky AI 助手

你内嵌在 Disky —— 一个 Windows 磁盘空间分析工具中，通过它的聊天面板与用户交流。

## 你的角色
帮助用户理解磁盘占用、定位空间大户、判断目录/文件的作用与可清理性，给出安全的清理建议。

## 铁律
- **绝不**删除、移动、修改用户磁盘上的任何文件。删除由 Disky 应用自身的回收站流程执行，你只分析和建议。
- 磁盘上你只有只读工具（Read / Glob / Grep）；唯一可写的位置是工作目录里的 `memory/`。
- 用用户消息的语言回答（通常是中文），简洁、可操作。
- 给清理建议时按 disk-cleanup 技能中的评级标准给出 0.5-5.0 优先级。
- 回复中引用磁盘位置时一律写完整绝对路径（如 `D:\my_pro\foo`，不要省略盘符）——
  Disky 会把它渲染成可点击链接，用户点击即可在左侧目录树中跳转查看。
- Markdown 表格中单元格内容保持简短，长路径放在表格外的列表里。

## 上下文：渐进式披露（由浅入深，按需深入）
1. prompt 里的概览 — 根目录一层，回答粗粒度问题通常够用
2. `context/scan-summary.md` — 多层目录大小明细（每轮刷新）
3. `context/scan-tree.tsv` — 接近全量的扫描树（字节数<TAB>路径，每轮刷新）。
   **这是你的"左侧目录树"**：像用户在 Disky 里逐层点开目录一样，用 Grep 在这里检索任意子树，
   例如 `Grep pattern="\\\\node_modules" path="context/scan-tree.tsv"` 找出所有 node_modules 及其大小。
   文件可能上万行，不要整个 Read，用 Grep 定位。
4. 真实磁盘 — Read/Glob 实际查看文件内容（package.json、Cargo.toml、exe 名）做最终判断，不要凭目录名臆测
5. `context/user-data.md` — 用户的标签、笔记、清理历史、历史 AI 评级（每轮刷新）

## 你的记忆（重要）
`memory/notes.md` 是你自己的渐进式备忘录，跨会话保留，你可以用 Write/Edit 维护它：
- 回答涉及磁盘具体内容时，先 Read 它恢复已有认知
- 学到新结论就趁热更新：目录用途的调查结论、用户的偏好与决定（"X 不要动"、"Y 已确认是缓存"）、待办的清理建议
- 保持精炼（200 行以内）：合并同类、删过时条目，像整理笔记一样滚动重写，而不是无限追加

## 清理执行协议
你不能直接删除文件，但当用户**明确同意**清理某些路径后，把清理计划写入
**`memory/cleanup-plan.json`**（JSON 数组，path 为待清理项的绝对路径）：

```json
[{"path": "D:\\dev\\yarn-global\\cache", "reason": "yarn 包缓存，可重新下载"}]
```

写入后本轮结束时 Disky 会立即弹出确认窗口，用户最终确认后经回收站删除，并自动记入
清理历史——下一轮对话你能在 `context/user-data.md` 的"最近清理历史"里看到执行结果。
注意：只在用户明确同意后写入；一次写入一批；不要列系统保护目录。

## 安全意识
对系统目录（Windows、Program Files、AppData 中应用核心数据）保持保守；
对缓存、临时文件、旧安装包、构建产物可以积极建议清理。
"#;

const DISK_CLEANUP_SKILL: &str = r#"---
name: disk-cleanup
description: 磁盘清理评级方法论与 Windows 常见空间大户知识库。当用户询问哪些目录可以清理、某个目录是什么、如何释放空间、对路径做清理评级时使用。
---

# 磁盘清理分析

## 评级标准（priority，0.5-5.0，0.5 步进）
- 0.5-1.0 系统/程序核心，勿删（Windows、Program Files 主体、驱动）
- 1.5-2.0 可能重要，谨慎（用户文档、项目源码、配置、MSI 缓存）
- 2.5-3.0 中等（可重新生成但重建成本高：模型权重、SDK、引擎缓存）
- 3.5-4.0 推荐清理（包管理器缓存、日志、临时文件、不活跃项目的构建产物）
- 4.5-5.0 强烈推荐（回收站、旧备份、重复安装包、崩溃转储）

## Windows 常见空间大户
| 路径模式 | 作用 | 参考评级 |
|---|---|---|
| `**/node_modules` | npm 依赖，可由 lockfile 重建 | 不活跃项目 4.0 |
| `**/target`（同级有 Cargo.toml） | Rust 构建产物 | 4.0 |
| `%LOCALAPPDATA%\pip\cache`、`npm-cache`、`Yarn` | 包管理器缓存 | 4.0 |
| `%LOCALAPPDATA%\Temp` | 临时文件 | 4.5 |
| `C:\Windows\Installer` | MSI 缓存，删除会影响卸载/修复 | 1.5 |
| `C:\Windows\WinSxS` | 系统组件库 | 0.5（只能 DISM 清理） |
| `hiberfil.sys` / `pagefile.sys` | 休眠/页面文件 | 1.0（需在系统设置关闭） |
| `%LOCALAPPDATA%\Docker\wsl`、`ext4.vhdx` | Docker/WSL 虚拟磁盘 | 2.5（docker prune + compact 瘦身） |
| `~/.gradle/caches`、`~/.m2/repository` | JVM 依赖缓存 | 3.5 |
| `%LOCALAPPDATA%\NVIDIA\DXCache`、`GLCache` | 着色器缓存 | 4.0 |
| 浏览器 `User Data\Default\Cache` | 浏览器缓存 | 4.0 |
| `**/.git`（异常大） | git 历史，gc 可瘦身 | 2.0 |
| `*.iso`、`*.zip` 旧安装包 | 看修改时间 | 3.5-4.5 |
| Unity `Library/`、UE `DerivedDataCache/` | 引擎缓存，可重建（耗时） | 3.0 |
| `C:\ProgramData\Package Cache` | 安装器缓存（VS 等） | 2.0 |

## 调查方法
1. 先看 `context/scan-summary.md` 锁定大块头
2. 不认识的目录：Read 其中的 README / package.json / *.toml / exe 文件名判断归属
3. 用 Glob 看文件类型构成与修改时间判断活跃度（旧 = 更可清理）
4. 同名目录多处出现 → 可能是多版本残留
5. 结合 `context/user-data.md`：用户标过"重要"的路径要保守；清理历史反映用户偏好
"#;

fn fmt_size(bytes: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.1} MB", b / MB)
    }
}

/// 多层目录摘要：第 1 层取前 15、第 2 层前 8、第 3 层前 5，小目录截断，
/// 控制在几百行内 —— 比 prompt 里的概览深，又远小于完整树。
pub fn build_deep_scan_summary(tree: &TreeNode) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "# 扫描摘要\n\n根: {}\n总大小: {}（文件 {} 个 / 目录 {} 个）\n",
        tree.path,
        fmt_size(tree.logical_size),
        tree.files,
        tree.subdirs
    ));
    walk_node(tree, 0, &mut lines);
    lines.join("\n")
}

fn walk_node(node: &TreeNode, depth: usize, lines: &mut Vec<String>) {
    const TOP_PER_DEPTH: [usize; 3] = [15, 8, 5];
    const MIN_SIZE_PER_DEPTH: [u64; 3] = [10_485_760, 104_857_600, 524_288_000]; // 10MB/100MB/500MB
    if depth >= TOP_PER_DEPTH.len() || lines.len() > 400 {
        return;
    }
    let mut children: Vec<&TreeNode> = node.children.iter().collect();
    children.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));
    for child in children.into_iter().take(TOP_PER_DEPTH[depth]) {
        if child.logical_size < MIN_SIZE_PER_DEPTH[depth] {
            break;
        }
        let indent = "  ".repeat(depth);
        let kind = if child.is_dir { "" } else { " [文件]" };
        lines.push(format!(
            "{}- {}{} — {}",
            indent,
            child.name,
            kind,
            fmt_size(child.logical_size)
        ));
        if child.is_dir {
            walk_node(child, depth + 1, lines);
        }
    }
}

/// 接近全量的扫描树 dump（TSV：字节数<TAB>路径），AI 用 Grep 在其中导航子树，
/// 等价于用户在左侧 DirectoryTree 里逐层浏览。目录 ≥10MB、文件 ≥50MB，上限 2 万行。
pub fn build_scan_tree_dump(tree: &TreeNode) -> String {
    const MIN_DIR: u64 = 10_485_760; // 10MB
    const MIN_FILE: u64 = 52_428_800; // 50MB
    const MAX_LINES: usize = 20_000;
    let mut lines = vec![format!("{}\t{}", tree.logical_size, tree.path)];
    let mut stack: Vec<&TreeNode> = tree.children.iter().collect();
    while let Some(node) = stack.pop() {
        if lines.len() >= MAX_LINES {
            lines.push("# （已截断：超过 2 万行）".into());
            break;
        }
        let min = if node.is_dir { MIN_DIR } else { MIN_FILE };
        if node.logical_size < min {
            continue;
        }
        lines.push(format!("{}\t{}", node.logical_size, node.path));
        if node.is_dir {
            stack.extend(node.children.iter());
        }
    }
    lines.join("\n")
}

/// 用户数据摘要：标签、笔记、清理历史、历史 AI 评级（全部来自 disky.db）
pub fn build_user_data_summary(app: &tauri::AppHandle) -> String {
    let mut out = String::from("# 用户数据\n");
    let Ok(conn) = ensure_ai_db(app) else {
        return out;
    };

    out.push_str("\n## 标签（用户手动标注的路径）\n");
    if let Ok(mut stmt) = conn.prepare(
        "SELECT t.name, ft.path FROM file_tags ft JOIN tags t ON t.id = ft.tag_id
         ORDER BY t.name, ft.path LIMIT 100",
    ) {
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .ok();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                out.push_str(&format!("- [{}] {}\n", row.0, row.1));
            }
        }
    }

    out.push_str("\n## 笔记\n");
    if let Ok(mut stmt) = conn.prepare(
        "SELECT path, content FROM notes ORDER BY updated_at DESC LIMIT 30",
    ) {
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .ok();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                out.push_str(&format!("- {}: {}\n", row.0, row.1.replace('\n', " ")));
            }
        }
    }

    out.push_str("\n## 最近清理历史\n");
    if let Ok(mut stmt) = conn.prepare(
        "SELECT created_at, path, action_type, bytes_freed, description
         FROM cleanup_logs ORDER BY id DESC LIMIT 30",
    ) {
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .ok();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                out.push_str(&format!(
                    "- {} {} {}（释放 {}）{}\n",
                    row.0,
                    row.2,
                    row.1,
                    fmt_size(row.3.max(0) as u64),
                    if row.4.is_empty() { String::new() } else { format!("— {}", row.4) }
                ));
            }
        }
    }

    out.push_str("\n## 历史 AI 评级\n");
    if let Ok(mut stmt) = conn.prepare(
        "SELECT path, priority, description FROM ai_analysis ORDER BY created_at DESC LIMIT 50",
    ) {
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .ok();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                out.push_str(&format!("- [{}] {} — {}\n", row.1, row.0, row.2));
            }
        }
    }

    out
}

/// 写出/刷新受管工作目录，返回其路径（作为 claude 子进程的 cwd）。
/// `memory/` 是 AI 自己的跨会话备忘区，只创建不覆盖。
pub fn prepare_workspace(
    app: &tauri::AppHandle,
    scan_summary: &str,
    scan_tree: &str,
    user_data: &str,
) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("agent-workspace");
    let skills_dir = dir.join(".claude").join("skills").join("disk-cleanup");
    let context_dir = dir.join("context");
    let memory_dir = dir.join("memory");
    std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&context_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&memory_dir).map_err(|e| e.to_string())?;
    // 上一轮残留的清理计划（如进程中断未消费）作废，避免误弹确认窗
    let _ = std::fs::remove_file(memory_dir.join("cleanup-plan.json"));
    let _ = std::fs::remove_file(memory_dir.join("cleanup-plan-draft.json"));
    let _ = std::fs::remove_file(dir.join("requests").join("cleanup-plan.json"));

    std::fs::write(dir.join("CLAUDE.md"), CLAUDE_MD).map_err(|e| e.to_string())?;
    std::fs::write(skills_dir.join("SKILL.md"), DISK_CLEANUP_SKILL).map_err(|e| e.to_string())?;
    std::fs::write(context_dir.join("scan-summary.md"), scan_summary).map_err(|e| e.to_string())?;
    std::fs::write(context_dir.join("scan-tree.tsv"), scan_tree).map_err(|e| e.to_string())?;
    std::fs::write(context_dir.join("user-data.md"), user_data).map_err(|e| e.to_string())?;
    let notes = memory_dir.join("notes.md");
    if !notes.exists() {
        std::fs::write(&notes, "# AI 备忘录\n\n（尚无记录）\n").map_err(|e| e.to_string())?;
    }
    Ok(dir)
}
