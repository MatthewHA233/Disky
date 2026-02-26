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
cd src-tauri && cargo xwin check          # 后端
```

## 环境注意事项

- Rust 编译必须用 `cargo xwin`，不能直接 `cargo build`
- D 盘空间紧张，注意 `target/` 缓存膨胀
- `target/debug` 是 host 默认目标产物（无用），实际产物在 `target/x86_64-pc-windows-msvc/`

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
