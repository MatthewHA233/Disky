# Disky - 磁盘空间分析工具

## 技术栈

- **前端**: React + TypeScript + Vite
- **后端**: Rust (Tauri 2)，Windows + macOS 跨平台
- **数据库**: SQLite (rusqlite) — 存储历史记录、AI 分析、笔记等
- **扫描数据持久化**: bincode 二进制序列化 → `.bin` 文件

## 跨平台代码约定

平台差异全部收口在固定位置，新代码不要再散落写死 `\` 或 Windows 专有调用：

- **Rust 路径工具**: `src-tauri/src/scanner/mod.rs` — `SEP` / `normalize_path` / `join_path` / `is_strict_ancestor` / `path_file_name`，处理 Windows `\` 与 Unix `/`（含根目录 `/` 特例）
- **Rust 平台分支**: `#[cfg(windows)]` / `#[cfg(not(windows))]` 成对实现 —— 扫描枚举（`walk.rs` 的 `enumerate_dir`）、保护目录（`cleanup.rs` 的 `is_protected`）、右键系统集成（`context_menu.rs`：Explorer↔Finder、CMD/PowerShell↔Terminal、属性↔显示简介、PowerShell 选目录↔osascript choose folder）
- **整盘监听（档案馆实时更新）**: `src-tauri/src/archive/fswatch.rs` —— Windows 自己调 `ReadDirectoryChangesW`（notify 6 缓冲只有 16KB，溢出时静默丢事件），外接盘（USB/SD）不盯，免得挡住「安全弹出」；其他平台用 notify（FSEvents）。变化落到摘要的逻辑在 `archive/digest/live.rs`，调度在 `commands/archive_live.rs`
- **macOS 扫描跳过**: `walk.rs` 的 `should_skip_dir` 跳过 `/System/Volumes`（固件链接重复计数）、`/Volumes`（其他卷单独扫）、`/dev`
- **AI 提示词**: `agent_workspace.rs` 用单一模板 + `PLATFORM_VARS` 词表渲染（`__OS__`、`__TRASH__` 等占位符），平台空间大户知识库表在 `HOG_TABLE`；改共享文案只改模板，改平台差异只改词表
- **前端路径工具**: `src/lib/path.ts` — `SEP` / `IS_WINDOWS` / `TRASH_NAME`（回收站/废纸篓）/ `normalizePath` / `joinPath` / `basename` / `pathChain` 等，组件一律用它，不手写分隔符

## 开发命令（Windows）

本机无 Visual Studio / Windows SDK，C 盘空间极度紧张（~2GB），故意绕过 C++ 编译工具链，使用 xwin 交叉编译。Windows 上 `npm run tauri dev` 不可用。

开发模式分两步启动：

```bash
# 1. 前端开发服务器（热更新）
npm run dev

# 2. Rust 后端编译（另开终端）
cd src-tauri && cargo xwin build
# 编译完成后手动运行 target/x86_64-pc-windows-msvc/debug/disky.exe
```

类型检查：

```bash
npx tsc --noEmit                          # 前端
# 后端：禁止用 cargo xwin check（~5.5GB 无用产物）
# 直接 cargo xwin build 即可，编译本身就会检查
```

### Rust 改动后的重启流程

改了 `src-tauri/src/**/*.rs` 后按顺序处理。运行中的 `disky.exe` 会占用 debug 产物，
必须先 kill 再编译：

```powershell
# 1. 关闭运行中的应用（否则编译报 "文件被占用"）
Get-Process disky -ErrorAction SilentlyContinue | Stop-Process -Force

# 2. 编译后端 debug 产物（绝对路径，避免 Bash 工具 cwd 漂移）
cd D:\my_pro\py_ob\Disky\src-tauri && cargo xwin build

# 3. 重启 exe（前端 Vite dev server 由用户保持运行，不要重启）
D:\my_pro\py_ob\Disky\src-tauri\target\x86_64-pc-windows-msvc\debug\disky.exe
```

### WebView2 DevTools MCP 调试（看真实 DOM / 控制台 / 截图）

调试 Tauri 内嵌 WebView2 页面时，用远程调试端口启动 exe，chrome-devtools MCP 即可连上，
直接查看真实 DOM、页面文本、console、截图，不必靠猜：

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"
D:\my_pro\py_ob\Disky\src-tauri\target\x86_64-pc-windows-msvc\debug\disky.exe
```

连接前先确认端口可访问：`http://127.0.0.1:9222/json/version`。
MCP 配置（stdio）：`npx -y chrome-devtools-mcp@latest --browser-url=http://127.0.0.1:9222 --no-usage-statistics`。
连上后用 `list_pages` / `select_page` / `take_snapshot` / `list_console_messages` 调试当前会话。

构建 Windows 安装包（MSI + NSIS EXE）：

```bash
npx tauri build --runner cargo-xwin
# 产物位置：
# src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/
# src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/
```

### Windows 环境注意事项

- Rust 编译必须用 `cargo xwin`，不能直接 `cargo build`
- D 盘空间紧张，注意 `target/` 缓存膨胀
- **禁止** `cargo xwin check` — 会产生 ~5.5GB 无用 debug 产物
- **禁止自主使用 `cargo xwin build --release`** — release 编译产物极大，会撑爆 D 盘。除非用户明确要求，否则一律使用 `cargo xwin build`（debug 模式）
- `target/debug` 是 host 默认目标产物（无用），实际产物在 `target/x86_64-pc-windows-msvc/debug/`

## 开发命令（macOS）

**mac 上就是正常的原生工具链，没有 Windows 那一套 xwin 限制。** `cargo build` / `tauri dev` 都直接可用。

```bash
# 一条命令起开发环境（tauri.conf.json 配了 beforeDevCommand，会自动起 vite + 增量编译 Rust + 弹出窗口）
npx tauri dev

# 或者只编译后端验证（debug，产物 src-tauri/target/debug/disky）
cd src-tauri && cargo build

# 前端类型检查同 Windows
npx tsc --noEmit
```

### macOS Rust 改动后的重启流程

```bash
# kill 正在跑的 dev 进程
pkill -f "tauri dev|target/debug/disky|vite" 2>/dev/null

# 重新启动（后台跑，Claude 可自己起）
npx tauri dev
```

### macOS 打包（APP + DMG）

```bash
npx tauri build
# 产物位置：
# src-tauri/target/release/bundle/macos/Disky.app
# src-tauri/target/release/bundle/dmg/
```

非用户明确要求不要自主跑 release 打包（编译耗时长）；日常验证用 `cargo build`（debug）。

### macOS 环境注意事项

- WebView 是系统 WKWebView（不是 WebView2），**chrome-devtools MCP 连不上**；debug 构建里 Tauri 自带 devtools，窗口里右键 → Inspect Element 即可
- 扫描整盘 `/` 需要给应用（或终端启动的 dev 进程所属终端）授予**完全磁盘访问权限**（系统设置 → 隐私与安全性），否则 `~/Library/Mail` 等目录读不到、尺寸偏小
- 磁盘列表只显示 `/`（启动卷）和 `/Volumes/*`（外接卷）；APFS 的 Preboot/VM/Update 等系统卷已过滤
- 删除走 `trash` crate（废纸篓），与 Windows 回收站同一套代码

## 项目结构

```
src/                          # 前端 React
├── components/               # UI 组件 (TreeMap, DirectoryTree, HistoryDialog 等)
├── hooks/                    # 自定义 hooks (useScanSession, useAiAnalysis 等)
├── lib/                      # 工具函数 (invoke.ts, format.ts, path.ts 跨平台路径)
└── types/                    # TypeScript 类型定义

src-tauri/src/                # Rust 后端
├── commands/                 # Tauri 命令 (scan, history, ai, notes, cleanup)
├── scanner/                  # 磁盘扫描核心 (walk.rs) + 跨平台路径工具 (mod.rs)
└── lib.rs                    # 应用入口 + 命令注册
```

## 核心功能

- 实时磁盘扫描 → TreeMap 矩形分块图 + DirectoryTree 目录树（双向导航联动）
- 扫描历史保存/加载（bincode 序列化，秒级保存，免重扫加载）
- 两次扫描对比（目录级差异分析）
- AI 分析（接入 Claude/OpenAI API，清理推荐评级）
- 文件/目录删除（回收站/废纸篓 or 永久）
