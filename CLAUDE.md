# Disky - 磁盘空间分析工具

## 技术栈

- **前端**: React + TypeScript + Vite
- **后端**: Rust (Tauri 2)
- **数据库**: SQLite (rusqlite) — 存储历史记录、AI 分析、笔记等
- **扫描数据持久化**: bincode 二进制序列化 → `.bin` 文件

## 开发命令

本机无 Visual Studio / Windows SDK，C 盘空间极度紧张（~2GB），故意绕过 C++ 编译工具链，使用 xwin 交叉编译。`npm run tauri dev` 不可用。

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

## 环境注意事项

- Rust 编译必须用 `cargo xwin`，不能直接 `cargo build`
- D 盘空间紧张，注意 `target/` 缓存膨胀
- **禁止** `cargo xwin check` — 会产生 ~5.5GB 无用 debug 产物
- **禁止自主使用 `cargo xwin build --release`** — release 编译产物极大，会撑爆 D 盘。除非用户明确要求，否则一律使用 `cargo xwin build`（debug 模式）
- `target/debug` 是 host 默认目标产物（无用），实际产物在 `target/x86_64-pc-windows-msvc/debug/`

## 项目结构

```
src/                          # 前端 React
├── components/               # UI 组件 (TreeMap, DirectoryTree, HistoryDialog 等)
├── hooks/                    # 自定义 hooks (useScanSession, useAiAnalysis 等)
├── lib/                      # 工具函数 (invoke.ts, format.ts)
└── types/                    # TypeScript 类型定义

src-tauri/src/                # Rust 后端
├── commands/                 # Tauri 命令 (scan, history, ai, notes, cleanup)
├── scanner/                  # 磁盘扫描核心 (walk.rs)
└── lib.rs                    # 应用入口 + 命令注册
```

## 核心功能

- 实时磁盘扫描 → TreeMap 矩形分块图 + DirectoryTree 目录树（双向导航联动）
- 扫描历史保存/加载（bincode 序列化，秒级保存，免重扫加载）
- 两次扫描对比（目录级差异分析）
- AI 分析（接入 Claude/OpenAI API，清理推荐评级）
- 文件/目录删除（回收站 or 永久）
