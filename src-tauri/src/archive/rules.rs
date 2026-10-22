//! 档案馆的内置规则。规则只管认得很确定的东西，认不出的留给 AI 和用户。
//!
//! - 名字归一化与匹配：数据目录名 ↔ 软件名
//! - 别名：名字对不上但确定属于某软件的目录（`xwechat_files` → 微信）
//! - 开发工具链：没有安装记录、只在用户目录留下一堆缓存的工具（nvm、.rustup、pip）
//! - 用途提示：常见软件属于哪种使用目的
//! - 系统目录：Program Files 里属于 Windows 自己的部分
//! - 三大门类：软件 / 个人文件 / 插件与开发环境，各有自己的内置箱子

use crate::scanner::classify::Kind;

/// 门类：软件按使用目的分箱；个人文件按内容分箱；插件与开发环境按所属生态分箱
pub const SOFTWARE: &str = "software";
pub const PERSONAL: &str = "personal";
pub const DEVENV: &str = "devenv";

/// 内置箱子：(id, 名字, 颜色, 门类)
pub const BOX_SEEDS: &[(&str, &str, &str, &str)] = &[
    ("game", "游戏", "#D83B01", SOFTWARE),
    ("dev", "开发", "#0078D4", SOFTWARE),
    ("create", "创作", "#8764B8", SOFTWARE),
    ("chat", "聊天", "#107C10", SOFTWARE),
    ("office", "办公与笔记", "#C19C00", SOFTWARE),
    ("tool", "工具", "#038387", SOFTWARE),
    ("media", "影音", "#E3008C", SOFTWARE),
    ("system", "系统与驱动", "#7A7574", SOFTWARE),
    ("p-code", "项目代码", "#0078D4", PERSONAL),
    ("p-media", "照片视频", "#E3008C", PERSONAL),
    ("p-docs", "文档资料", "#C19C00", PERSONAL),
    ("p-game", "游戏资源", "#D83B01", PERSONAL),
    ("p-down", "安装包与下载", "#038387", PERSONAL),
    ("p-chat", "聊天记录与备份", "#107C10", PERSONAL),
    ("env-cli", "命令行工具", "#8764B8", DEVENV),
    ("env-node", "Node.js", "#498205", DEVENV),
    ("env-python", "Python", "#0063B1", DEVENV),
    ("env-lang", "其他语言与构建工具", "#CA5010", DEVENV),
    ("env-ext", "编辑器扩展", "#00B7C3", DEVENV),
];

/// 箱子 id 属于哪个门类（内置箱子按前缀；用户建的箱子由库里记着）
pub fn category_of_box(id: &str) -> &'static str {
    if id.starts_with("env-") {
        DEVENV
    } else if id.starts_with("p-") {
        PERSONAL
    } else {
        SOFTWARE
    }
}

/// 名字归一化：小写、去掉开头的点，非字母数字一律当分隔，连续分隔合成一个空格。
/// 中文等非 ASCII 字母按字母保留。
pub fn norm(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut gap = false;
    for ch in lower.trim_start_matches('.').chars() {
        if ch.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(ch);
        } else {
            gap = true;
        }
    }
    out
}

/// 去掉空格后的紧凑形式（`clash verge` ↔ `clashverge`）
pub fn compact(s: &str) -> String {
    s.chars().filter(|c| *c != ' ').collect()
}

/// 目录名 `folder`（已归一化）与软件的一个名字 `key`（已归一化）的匹配程度，0 为不匹配。
/// 完全相同 > 去空格后相同 > 一方是另一方开头的整词（`cursor` ↔ `cursor updater`）。
pub fn match_score(folder: &str, key: &str) -> usize {
    if folder.is_empty() || key.is_empty() {
        return 0;
    }
    if folder == key {
        return 1000 + key.len();
    }
    let (cf, ck) = (compact(folder), compact(key));
    if cf.len() >= 4 && cf == ck {
        return 900 + key.len();
    }
    let word_prefix = |long: &str, short: &str| long.len() > short.len() && long.starts_with(short) && long.as_bytes()[short.len()] == b' ';
    // 目录名带后缀（cursor updater）可以；软件名比目录名多出来的只能是泛泛的后缀
    // （telegram desktop、epic games launcher），否则 Claude 会被当成 Claude Usage Monitor 的
    let generic_tail = |long: &str, short: &str| word_prefix(long, short) && is_generic_suffix(&long[short.len() + 1..]);
    if key.chars().count() >= 3 && (word_prefix(folder, key) || generic_tail(key, folder)) {
        return key.len().min(folder.len());
    }
    0
}

/// 软件名常见的泛泛后缀
fn is_generic_suffix(tail: &str) -> bool {
    matches!(
        tail,
        "desktop" | "launcher" | "client" | "app" | "for windows" | "pc" | "客户端" | "电脑版" | "corporation" | "inc"
            | "studio" | "preview" | "beta" | "x64" | "64 bit" | "player"
    )
}

/// 目录名是否命中别名：只认目录名等于别名或以别名整词开头（`dingtalk 108` 命中 `dingtalk`），
/// 反过来不算——`tencent` 不能因为别名里有 `tencent files` 就整个归给 QQ
pub fn alias_matches(folder: &str, alias: &str) -> bool {
    folder == alias
        || (folder.len() > alias.len() && folder.starts_with(alias) && folder.as_bytes()[alias.len()] == b' ')
        || (compact(folder).len() >= 4 && compact(folder) == compact(alias))
}

/// 太泛、不能拿来认软件的名字
pub fn is_generic_key(key: &str) -> bool {
    key.chars().count() < 2
        || matches!(
            key,
            "app" | "apps" | "bin" | "data" | "common" | "client" | "desktop" | "launcher" | "main" | "program"
                | "programs" | "setup" | "update" | "updater" | "tools" | "tool" | "windows" | "microsoft"
                | "x64" | "x86" | "user data" | "local" | "roaming" | "temp" | "cache" | "files" | "plugins"
                | "resources" | "runtime" | "service" | "studio" | "helper" | "default" | "config" | "lib"
        )
}

/// 别名：目录名（归一化）→ 软件的某个名字（归一化）；可附带性质（游戏库是「可重新下载」）
pub const ALIASES: &[(&str, &str, Option<Kind>)] = &[
    ("xwechat files", "weixin", None),
    ("wechat files", "weixin", None),
    ("xwechat", "weixin", None),
    ("wechat", "weixin", None),
    ("tencent files", "qq", None),
    ("qqnt", "qq", None),
    ("larkshell", "feishu", None),
    ("lark", "feishu", None),
    ("dingtalk", "dingding", None),
    ("steamlibrary", "steam", Some(Kind::Regen)),
    ("steamapps", "steam", Some(Kind::Regen)),
    ("baidunetdiskdownload", "baidunetdisk", None),
    ("baiduyunguanjia", "baidunetdisk", None),
    ("baiduyunkernel", "baidunetdisk", None),
    ("qqmusiccache", "qqmusic", Some(Kind::Cache)),
    ("kingsoft", "wps", None),
    ("kso", "wps", None),
    ("wpsdrive", "wps", None),
    ("vscode", "code", None),
    ("vscode insiders", "code", None),
    ("blizzard entertainment", "battle net", None),
    ("electronic arts", "ea", None),
    ("ubisoft game launcher", "ubisoft connect", None),
    ("epicgameslauncher", "epic games launcher", None),
    ("hoyoplay", "mihoyo launcher", None),
    ("mihoyo", "mihoyo launcher", None),
    ("lghub", "logitech g hub", None),
    ("g hub", "logitech g hub", None),
];

/// 没有安装记录、按位置认出来的东西：(所在位置, 目录名, id, 显示名, 性质, 箱子)
/// 位置：home = 用户目录，local / roaming / locallow = AppData 下，root = 盘根。
/// 箱子决定门类：env-* 是插件与开发环境，其余是软件
pub struct Toolchain {
    pub place: &'static str,
    pub folder: &'static str,
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub purpose: &'static str,
}

const fn tc(place: &'static str, folder: &'static str, id: &'static str, name: &'static str, kind: Kind, purpose: &'static str) -> Toolchain {
    Toolchain { place, folder, id, name, kind, purpose }
}

pub const TOOLCHAINS: &[Toolchain] = &[
    tc("local", "nvm", "nvm", "nvm（Node.js 版本管理）", Kind::Regen, "env-node"),
    tc("roaming", "npm", "npm-global", "npm 全局包", Kind::Regen, "env-node"),
    tc("local", "npm-cache", "npm-cache", "npm 缓存", Kind::Cache, "env-node"),
    tc("roaming", "npm-cache", "npm-cache", "npm 缓存", Kind::Cache, "env-node"),
    tc("home", ".npm", "npm-cache", "npm 缓存", Kind::Cache, "env-node"),
    tc("local", "pnpm", "pnpm", "pnpm", Kind::Regen, "env-node"),
    tc("local", "pnpm-cache", "pnpm-cache", "pnpm 缓存", Kind::Cache, "env-node"),
    tc("home", ".pnpm-store", "pnpm-cache", "pnpm 缓存", Kind::Cache, "env-node"),
    tc("local", "yarn", "yarn-cache", "Yarn 缓存", Kind::Cache, "env-node"),
    tc("home", ".yarn", "yarn-cache", "Yarn 缓存", Kind::Cache, "env-node"),
    tc("local", "node-gyp", "node-gyp", "node-gyp 缓存", Kind::Cache, "env-node"),
    tc("local", "ms-playwright", "playwright", "Playwright 浏览器", Kind::Regen, "env-node"),
    tc("local", "electron", "electron", "Electron 下载缓存", Kind::Cache, "env-node"),
    tc("local", "electron-builder", "electron", "Electron 下载缓存", Kind::Cache, "env-node"),
    tc("home", ".bun", "bun", "Bun", Kind::Regen, "env-node"),
    tc("local", "pip", "pip-cache", "pip 缓存", Kind::Cache, "env-python"),
    tc("local", "uv", "uv", "uv", Kind::Cache, "env-python"),
    tc("local", "pypa", "pypa", "Python 工具缓存", Kind::Cache, "env-python"),
    tc("local", "pyinstaller", "pyinstaller", "PyInstaller 缓存", Kind::Cache, "env-python"),
    tc("local", "conda", "conda", "Conda", Kind::Regen, "env-python"),
    tc("home", ".conda", "conda", "Conda", Kind::Regen, "env-python"),
    tc("home", ".anaconda", "conda", "Conda", Kind::Regen, "env-python"),
    tc("home", "anaconda3", "conda", "Conda", Kind::Regen, "env-python"),
    tc("home", "miniconda3", "conda", "Conda", Kind::Regen, "env-python"),
    tc("home", ".rustup", "rust", "Rust 工具链", Kind::Regen, "env-lang"),
    tc("home", ".cargo", "rust", "Rust 工具链", Kind::Regen, "env-lang"),
    tc("home", "go", "go", "Go", Kind::Regen, "env-lang"),
    tc("local", "go-build", "go", "Go", Kind::Cache, "env-lang"),
    tc("home", ".gradle", "java", "Java（Gradle / Maven）", Kind::Regen, "env-lang"),
    tc("home", ".m2", "java", "Java（Gradle / Maven）", Kind::Regen, "env-lang"),
    tc("home", ".nuget", "dotnet", ".NET（NuGet）", Kind::Regen, "env-lang"),
    tc("local", "nuget", "dotnet", ".NET（NuGet）", Kind::Cache, "env-lang"),
    tc("home", ".dotnet", "dotnet", ".NET（NuGet）", Kind::Regen, "env-lang"),
    tc("home", ".android", "android", "Android SDK", Kind::Regen, "env-lang"),
    tc("local", "android", "android", "Android SDK", Kind::Regen, "env-lang"),
    tc("home", ".claude", "claude-code", "Claude Code", Kind::Data, "env-cli"),
    tc("home", ".codex", "codex", "Codex CLI", Kind::Data, "env-cli"),
    tc("home", ".gemini", "gemini-cli", "Gemini CLI", Kind::Data, "env-cli"),
    tc("local", "temp", "temp", "临时文件", Kind::Cache, "system"),
    tc("local", "crashdumps", "crashdumps", "崩溃转储", Kind::Cache, "system"),
    tc("local", "d3dscache", "shadercache", "显卡着色器缓存", Kind::Cache, "system"),
    tc("locallow", "intel", "shadercache", "显卡着色器缓存", Kind::Cache, "system"),
    tc("local", "packages", "store", "应用商店应用", Kind::Data, "system"),
    tc("root", "windowsapps", "store", "应用商店应用", Kind::Prog, "system"),
    tc("root", "wpsystem", "store", "应用商店应用", Kind::Data, "system"),
];

/// 用环境变量改过位置的工具链（CARGO_HOME=D:\dev\.cargo）：(变量, id, 显示名, 性质, 箱子)
pub const ENV_TOOLCHAINS: &[(&str, &str, &str, Kind, &str)] = &[
    ("CARGO_HOME", "rust", "Rust 工具链", Kind::Regen, "env-lang"),
    ("RUSTUP_HOME", "rust", "Rust 工具链", Kind::Regen, "env-lang"),
    ("GOPATH", "go", "Go", Kind::Regen, "env-lang"),
    ("GOROOT", "go", "Go", Kind::Prog, "env-lang"),
    ("NVM_HOME", "nvm", "nvm（Node.js 版本管理）", Kind::Regen, "env-node"),
    ("PNPM_HOME", "pnpm", "pnpm", Kind::Regen, "env-node"),
    ("BUN_INSTALL", "bun", "Bun", Kind::Regen, "env-node"),
    ("npm_config_cache", "npm-cache", "npm 缓存", Kind::Cache, "env-node"),
    ("GRADLE_USER_HOME", "java", "Java（Gradle / Maven）", Kind::Regen, "env-lang"),
    ("JAVA_HOME", "java", "Java（Gradle / Maven）", Kind::Prog, "env-lang"),
    ("ANDROID_HOME", "android", "Android SDK", Kind::Regen, "env-lang"),
    ("ANDROID_SDK_ROOT", "android", "Android SDK", Kind::Regen, "env-lang"),
    ("UV_CACHE_DIR", "uv", "uv", Kind::Cache, "env-python"),
    ("PIP_CACHE_DIR", "pip-cache", "pip 缓存", Kind::Cache, "env-python"),
    ("PYTHONUSERBASE", "python-user", "Python 用户包", Kind::Regen, "env-python"),
    ("CODEX_HOME", "codex", "Codex CLI", Kind::Data, "env-cli"),
    ("CLAUDE_CONFIG_DIR", "claude-code", "Claude Code", Kind::Data, "env-cli"),
];

/// 编辑器的扩展目录：(用户目录下的点目录, 编辑器名)
pub const EDITOR_DIRS: &[(&str, &str)] = &[
    (".vscode", "VS Code"),
    (".vscode-insiders", "VS Code Insiders"),
    (".cursor", "Cursor"),
    (".kiro", "Kiro"),
    (".antigravity", "Antigravity"),
    (".windsurf", "Windsurf"),
    (".trae", "Trae"),
    (".trae-cn", "Trae"),
    (".qoder", "Qoder"),
    (".qoder-cn", "Qoder"),
    (".vscode-oss", "VSCodium"),
];

/// 放在哪都认的开发工具目录名（D:\dev\LLVM、D:\dev\vcpkg）：(目录名, id, 显示名, 性质, 箱子, 名字太泛时里面必须有的子目录)
pub const DEV_FOLDERS: &[(&str, &str, &str, Kind, &str, Option<&str>)] = &[
    ("llvm", "llvm", "LLVM", Kind::Prog, "env-lang", None),
    ("vcpkg", "vcpkg", "vcpkg", Kind::Regen, "env-lang", None),
    ("msys64", "msys2", "MSYS2", Kind::Prog, "env-lang", None),
    ("flutter", "flutter", "Flutter", Kind::Prog, "env-lang", None),
    ("strawberry", "strawberry-perl", "Strawberry Perl", Kind::Prog, "env-lang", Some("perl")),
    ("tdm-gcc-64", "mingw", "MinGW 编译器", Kind::Prog, "env-lang", None),
    ("tdm-gcc-32", "mingw", "MinGW 编译器", Kind::Prog, "env-lang", None),
    ("mingw64", "mingw", "MinGW 编译器", Kind::Prog, "env-lang", Some("bin")),
    ("mingw32", "mingw", "MinGW 编译器", Kind::Prog, "env-lang", Some("bin")),
    ("rust-gnu", "rust", "Rust 工具链", Kind::Prog, "env-lang", None),
    ("yarn-cache", "yarn-cache", "Yarn 缓存", Kind::Cache, "env-node", None),
    ("yarn-global", "yarn-global", "Yarn 全局包", Kind::Regen, "env-node", None),
    ("cargo-xwin-cache", "cargo-xwin", "cargo-xwin 缓存", Kind::Cache, "env-lang", None),
    (".rustup", "rust", "Rust 工具链", Kind::Regen, "env-lang", None),
    (".cargo", "rust", "Rust 工具链", Kind::Regen, "env-lang", None),
];

/// JDK 目录的好认的名字：jdk-21.0.11+10 → JDK 21.0.11，jdk1.8.0_301 → JDK 1.8.0
pub fn jdk_display(dir: &str) -> Option<String> {
    let l = dir.to_ascii_lowercase();
    let ver = l.strip_prefix("jdk")?.trim_start_matches('-');
    let ver: String = ver.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let ver = ver.trim_end_matches('.');
    (!ver.is_empty() && ver.starts_with(|c: char| c.is_ascii_digit())).then(|| format!("JDK {ver}"))
}

/// npm 全局包里有名有姓的命令行工具：(包名, id, 显示名, 箱子)。id 和用户目录里认出的同一个工具一致
pub const NPM_TOOLS: &[(&str, &str, &str, &str)] = &[
    ("@anthropic-ai/claude-code", "claude-code", "Claude Code", "env-cli"),
    ("@openai/codex", "codex", "Codex CLI", "env-cli"),
    ("@google/gemini-cli", "gemini-cli", "Gemini CLI", "env-cli"),
    ("@qwen-code/qwen-code", "qwen-code", "Qwen Code", "env-cli"),
    ("pnpm", "pnpm", "pnpm", "env-node"),
];

/// 个人文件夹只在用户自己的地方猜（AppData、ProgramData、程序目录、Windows 里同名的文件夹是软件的）
pub fn is_personal_area(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    !["\\appdata\\", "\\programdata", "\\program files", "\\windows\\", "/library/"].iter().any(|m| l.contains(m))
}

/// 装在系统里、但其实是开发环境 / 命令行工具的软件（按名字认）
const DEVENV_HINTS: &[(&str, &[&str])] = &[
    ("env-python", &["python", "python launcher", "anaconda", "anaconda3", "miniconda", "miniconda3", "pycharm community"]),
    ("env-node", &["node js", "nodejs", "nvm for windows", "nvm", "bun", "deno", "yarn", "pnpm"]),
    ("env-lang", &[
        "go", "go programming language", "golang", "rust", "rustup", "java", "jdk", "openjdk", "eclipse temurin", "microsoft net sdk",
        "net sdk", "android sdk", "llvm", "mingw", "tdm gcc", "cmake", "msys2", "strawberry perl", "vcpkg", "flutter",
        "dart sdk", "erlang", "julia", "windows software development kit", "windows sdk",
    ]),
    ("env-cli", &["git", "github cli", "claude code", "codex", "codex cli", "gemini cli", "uv", "ffmpeg", "openssl", "powershell", "oh my posh", "starship"]),
];

/// 软件里哪些其实属于插件与开发环境，给出对应箱子
pub fn devenv_box<'a>(keys: impl IntoIterator<Item = &'a String>) -> Option<&'static str> {
    let keys: Vec<&String> = keys.into_iter().collect();
    DEVENV_HINTS
        .iter()
        .find(|(_, names)| names.iter().any(|h| keys.iter().any(|k| hint_matches(k, &norm(h)))))
        .map(|(b, _)| *b)
}

/// 名字整词相同、以它开头，或者后面只跟版本号（python310）
fn hint_matches(key: &str, hint: &str) -> bool {
    key == hint
        || (key.len() > hint.len() && key.starts_with(hint) && key.as_bytes()[hint.len()] == b' ')
        || key.strip_prefix(hint).is_some_and(|rest| {
            let rest = rest.trim();
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == ' ')
        })
}

/// 运行时目录的好认的名字：Python310 → Python 3.10，v20.18.1 → Node.js 20.18.1
pub fn runtime_display(dir: &str) -> Option<String> {
    let l = dir.to_ascii_lowercase();
    if let Some(rest) = l.strip_prefix("python") {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 2 && digits.len() == rest.len() {
            return Some(format!("Python {}.{}", &digits[..1], &digits[1..]));
        }
    }
    if let Some(ver) = l.strip_prefix('v') {
        if ver.starts_with(|c: char| c.is_ascii_digit()) && ver.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(format!("Node.js {ver}"));
        }
    }
    None
}

/// 个人文件夹按名字（和里面有什么）猜放进哪个箱子
pub fn personal_hint(name: &str, kids: &[String]) -> Option<&'static str> {
    let n = norm(name);
    let has = |k: &str| kids.iter().any(|c| c.eq_ignore_ascii_case(k));
    if has(".git") || has("node_modules") || has("src-tauri") || has("cargo.toml") {
        return Some("p-code");
    }
    // 里面有 QQ 数据的只有 QQ 号文件夹整个算聊天记录；别的文件夹里混着一份不说明整个是
    let qq_number = name.len() >= 5 && name.chars().all(|c| c.is_ascii_digit());
    if qq_number && (has("nt_qq") || has("msg3.0.db") || has("filerecv")) {
        return Some("p-chat");
    }
    let table: &[(&str, &[&str])] = &[
        ("p-code", &["my pro", "projects", "project", "code", "source", "repos", "github", "workspace", "unity ob", "web ob", "py ob"]),
        ("p-media", &["pictures", "photos", "图片", "照片", "videos", "视频", "录屏", "captures", "screenshots", "sharex", "music", "音乐", "camera", "dcim", "movies", "截图", "壁纸", "wallpaper", "藏图"]),
        ("p-docs", &["documents", "文档", "资料", "docs", "books", "电子书", "论文", "notes", "笔记", "onenote"]),
        ("p-game", &["game", "games", "游戏", "roms", "wii u", "wiiu game", "switch", "mods", "saved games", "存档"]),
        ("p-down", &["downloads", "下载", "baidunetdiskdownload", "installers", "安装包", "迅雷下载", "下载历史整理"]),
        ("p-chat", &["wechat files", "xwechat files", "tencent files", "nt qq", "qqfilerecv", "聊天记录", "backup", "备份", "网盘备份"]),
    ];
    table
        .iter()
        .find(|(_, names)| names.iter().any(|h| n == norm(h) || n.starts_with(&format!("{} ", norm(h)))))
        .map(|(b, _)| *b)
}

/// 目录名对应的常用中文名（没有安装记录时用）
pub fn display_name(dir: &str) -> Option<&'static str> {
    Some(match norm(dir).as_str() {
        "weixin" | "wechat" => "微信",
        "qqnt" => "QQ",
        "wemeet" => "腾讯会议",
        "feishu" => "飞书",
        "dingding" | "dingtalk" => "钉钉",
        "cloudmusic" => "网易云音乐",
        "kugou" => "酷狗音乐",
        "baidunetdisk" => "百度网盘",
        "qqmusic" => "QQ 音乐",
        "tencent" => "腾讯",
        "netease" => "网易",
        _ => return None,
    })
}

/// Program Files 里属于 Windows 自己的目录，合成一项「Windows 组件」
pub fn is_system_program_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "common files" | "windows defender" | "windows defender advanced threat protection" | "windows mail"
            | "windows media player" | "windows multimedia platform" | "windows nt" | "windows photo viewer"
            | "windows portable devices" | "windows security" | "windows sidebar" | "windowspowershell"
            | "internet explorer" | "microsoft update health tools" | "reference assemblies" | "msbuild"
            | "uninstall information" | "modifiablewindowsapps" | "windowsapps" | "dotnet" | "microsoft.net"
            | "windows kits" | "microsoft sdks" | "iis" | "iis express" | "package cache"
    )
}

/// 一家厂商的多个小组件合成一项（驱动、运行库）
pub fn grouped_vendor(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "nvidia corporation" | "nvidia" => "NVIDIA 驱动",
        "intel" | "intel corporation" => "Intel 驱动",
        "realtek" => "Realtek 驱动",
        "amd" => "AMD 驱动",
        _ => return None,
    })
}

/// 装着多个软件数据的厂商目录（AppData、ProgramData 里），要再往下一层认
pub fn is_vendor_folder(name: &str) -> bool {
    matches!(
        norm(name).as_str(),
        "microsoft" | "google" | "tencent" | "adobe" | "netease" | "baidu" | "apple" | "apple computer"
            | "jetbrains" | "bytedance" | "alibaba" | "autodesk" | "mozilla" | "programs" | "packages"
            | "mihoyo" | "huawei" | "nvidia corporation" | "intel" | "electronic arts" | "ea games"
    )
}

/// 软件目录里常见的子目录名：有 exe 也不说明它是另一个软件（Git 的 bin、cmd、mingw64）
pub fn is_generic_subdir(name: &str) -> bool {
    let n = norm(name);
    matches!(
        n.as_str(),
        "bin" | "bin64" | "bin32" | "cmd" | "lib" | "libs" | "usr" | "mingw64" | "mingw32" | "x64" | "x86" | "amd64"
            | "arm64" | "win64" | "win32" | "tools" | "plugins" | "plugin" | "resources" | "res" | "app" | "jre" | "jdk"
            | "runtime" | "runtimes" | "platforms" | "sdk" | "share" | "include" | "scripts" | "dlls" | "redist"
            | "uninstall" | "support files" | "support" | "components" | "modules" | "extensions" | "locales"
            | "assets" | "data" | "program" | "programs" | "core" | "engine" | "binaries" | "common" | "shared"
            | "updater" | "update" | "installer" | "setup" | "packages" | "help" | "docs" | "doc" | "examples"
            | "samples" | "python" | "node" | "electron" | "application" | "current" | "latest" | "stable"
    ) || n.starts_with("app ")
        || n.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// 厂商目录里的组件名太泛（Google\Update、Huawei\Cloud），显示时要带上厂商名
pub fn needs_vendor_prefix(name: &str) -> bool {
    let n = norm(name);
    is_generic_key(&n)
        || is_generic_subdir(name)
        || n.chars().count() <= 4
        || matches!(
            n.as_str(),
            "update" | "cloud" | "hypervisor" | "service" | "basicservice" | "platform" | "driver" | "drivers" | "agent"
                | "browser" | "manager" | "assistant" | "center" | "control center" | "toolbox"
        )
}

/// 盘根下装着多个软件的容器目录（`D:\Software\xxx`）：每个子目录各是一个软件
pub fn is_software_container(name: &str) -> bool {
    matches!(
        norm(name).as_str(),
        "software" | "soft" | "softwares" | "软件" | "green" | "绿色软件" | "portable" | "portableapps" | "program files"
            | "program files x86" | "programs" | "apps" | "applications" | "games" | "游戏"
    )
}

/// 盘根下不当作软件的目录（即使里面有 exe）
pub fn is_not_app_root(name: &str) -> bool {
    matches!(
        norm(name).as_str(),
        "temp" | "tmp" | "cache" | "download" | "downloads" | "下载" | "backup" | "backups" | "备份" | "desktop"
            | "documents" | "文档" | "新建文件夹" | "new folder" | "uninstall" | "installer" | "installers"
            | "setup" | "安装包" | "users" | "windows" | "programdata" | "recovery" | "system volume information"
            | "recycle bin" | "msocache" | "perflogs" | "drivers" | "intel" | "amd" | "nvidia" | "boot" | "efi"
            | "inetpub" | "onedrivetemp" | "esd" | "swsetup" | "config msi"
    )
}

/// 名字像缓存、可以放心清理的目录。安装程序缓存（Package Cache）删了会让软件没法修复 / 卸载，不算。
pub fn is_junk_cache(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // LocalCache：应用商店应用把写到 AppData 的东西重定向到这里（pip 包、Minecraft 存档），不是缓存
    crate::scanner::classify::is_cache_name(&n)
        && !matches!(n.as_str(), "package cache" | "packagecache" | "installer cache" | "localcache")
}

/// 用户目录、文档、盘根下常见的个人文件夹名：就算和某个软件同名也不归给它（`~\code` 不是 VS Code）
pub fn is_personal_folder_name(name: &str) -> bool {
    matches!(
        norm(name).as_str(),
        "code" | "codes" | "dev" | "src" | "source" | "sources" | "project" | "projects" | "work" | "workspace" | "repos"
            | "repo" | "git" | "github" | "gitlab" | "games" | "game" | "music" | "video" | "videos" | "photos" | "pictures"
            | "docs" | "documents" | "downloads" | "download" | "data" | "backup" | "backups" | "tools" | "apps" | "software"
            | "temp" | "tmp" | "test" | "tests" | "files" | "my files" | "notes" | "books" | "study" | "desktop" | "share"
            | "学习" | "工作" | "项目" | "资料" | "文档" | "下载" | "游戏" | "音乐" | "视频" | "图片" | "备份" | "笔记"
    )
}

/// 显示名整理：去掉末尾的版本号、位数说明、语言后缀
pub fn clean_name(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // 有的登记项名字就是卸载程序的标题：「Remove Z-Library」
    for prefix in ["Remove ", "Uninstall ", "卸载"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if !rest.trim().is_empty() {
                s = rest.trim().to_string();
            }
        }
    }
    // 「Microsoft Office 专业增强版 2019 - zh-cn」
    if let Some((head, tail)) = s.rsplit_once(" - ") {
        let t = tail.trim();
        if t.len() <= 6 && t.contains('-') && t.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
            s = head.trim().to_string();
        }
    }
    // 末尾括号：「(64-bit)」「(x64)」「（64位）」
    loop {
        let t = s.trim_end();
        let open = match (t.ends_with(')'), t.ends_with('）')) {
            (true, _) => t.rfind('('),
            (_, true) => t.rfind('（'),
            _ => None,
        };
        match open {
            Some(i) if i > 0 => s = t[..i].trim_end().to_string(),
            _ => break,
        }
    }
    // 末尾的版本号，目录名常用 - 或 _ 拼接：「ryujinx-1.1.1053-win_x64」「Godot_v4.4.1-stable_win64」
    let is_sep = |c: char| c.is_whitespace() || c == '_' || c == '-';
    let mut end = s.len();
    let out = loop {
        let head = s[..end].trim_end_matches(is_sep);
        let Some(i) = head.rfind(is_sep) else { break head };
        let next = i + head[i..].chars().next().map_or(1, char::len_utf8);
        if !is_version_token(&head[next..]) {
            break head;
        }
        end = next;
    };
    let out = out.trim();
    if out.is_empty() { raw.trim().to_string() } else { out.to_string() }
}

fn is_version_token(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    let body = l.strip_prefix('v').unwrap_or(&l);
    (body.starts_with(|c: char| c.is_ascii_digit()) && body.chars().all(|c| c.is_ascii_digit() || c == '.'))
        || matches!(
            l.as_str(),
            "x64" | "x86" | "win64" | "win32" | "amd64" | "arm64" | "stable" | "portable" | "win" | "64bit" | "32bit"
                | "64-bit" | "32-bit" | "版本" | "version" | "ver"
        )
}

/// 常见软件的用途（名字按 `norm` 归一化后比较，整词开头即算）
const PURPOSE_HINTS: &[(&str, &[&str])] = &[
    ("game", &[
        "steam", "epic games launcher", "epic games", "mihoyo launcher", "hoyoplay", "米哈游启动器", "原神", "genshin impact",
        "崩坏 星穹铁道", "star rail", "绝区零", "wegame", "battle net", "战网", "ubisoft connect", "ea", "ea app", "origin",
        "rockstar games launcher", "rockstar games", "gog galaxy", "xbox", "minecraft", "mumu", "mumu模拟器", "bluestacks",
        "雷电模拟器", "ldplayer", "迅游", "xunyou", "雷神加速器", "网易uu加速器", "uu加速器", "小黑盒", "heybox", "wallpaper engine",
        "riot client", "league of legends", "英雄联盟", "valorant", "ryujinx", "yuzu", "cemu", "dolphin", "citra", "pcsx2",
        "retroarch", "wargaming", "wargaming net game center", "云 原神", "taptap", "鸣潮", "wuthering waves",
        "小黑盒加速器", "biubiu加速器", "biubiu", "雷神", "奇游加速器", "modern warships", "modernwarshipslauncher",
        "gaijin", "war thunder", "seerlauncher", "4399游戏盒", "mihoyo",
    ]),
    ("dev", &[
        "microsoft visual studio code", "visual studio code", "code", "cursor", "kiro", "antigravity", "qoder", "trae",
        "windsurf", "zed", "jetbrains toolbox", "intellij idea", "pycharm", "webstorm", "clion", "goland", "rider",
        "android studio", "android sdk", "microsoft visual studio", "visual studio", "visual studio installer", "git", "github desktop",
        "sourcetree", "gitkraken", "node js", "nodejs", "python", "anaconda", "miniconda", "go", "go programming language",
        "rust", "docker desktop", "docker", "postman", "apifox", "insomnia", "navicat", "dbeaver", "postgresql", "mysql",
        "redis", "mongodb compass", "unity hub", "unity", "godot", "unreal engine", "cmake", "llvm", "msys2", "putty",
        "xshell", "termius", "windows terminal", "claude", "codex", "cherry studio", "ollama", "lm studio", "cc switch",
        "wsl", "windows subsystem for linux", "tuanjie hub", "nvm for windows", "nvm", "bun", "powershell",
        "microsoft sql server", "sql server", "openssl", "tdm gcc", "mingw", "ffmpeg", "python launcher",
    ]),
    ("create", &[
        "adobe photoshop", "photoshop", "adobe premiere pro", "premiere pro", "adobe after effects", "after effects",
        "adobe audition", "audition", "adobe illustrator", "illustrator", "adobe lightroom", "lightroom", "adobe indesign",
        "adobe media encoder", "adobe creative cloud", "creative cloud", "3ds max", "autodesk 3ds max", "maya", "autodesk maya",
        "blender", "cinema 4d", "zbrush", "substance", "marmoset toolbag", "toolbag", "fl studio", "ableton live", "reaper",
        "audacity", "obs studio", "剪映", "剪映专业版", "capcut", "davinci resolve", "必剪", "直播姬", "figma", "sketch",
        "krita", "clip studio paint", "paint net", "screentogif", "aegisub", "handbrake", "万兴喵影", "filmora",
        "哔哩哔哩直播姬", "嗨格式录屏大师", "marmoset viewer", "bandicam", "oCam",
    ]),
    ("chat", &[
        "微信", "wechat", "weixin", "qq", "qqnt", "tim", "飞书", "feishu", "lark", "钉钉", "dingtalk", "dingding", "企业微信",
        "wxwork", "telegram", "telegram desktop", "discord", "slack", "microsoft teams", "teams", "zoom", "腾讯会议",
        "wemeet", "skype", "line", "whatsapp", "signal", "mattermost", "kook", "yy",
    ]),
    ("office", &[
        "microsoft office", "microsoft 365", "office", "word", "excel", "powerpoint", "outlook", "onenote", "wps office",
        "wps", "obsidian", "notion", "typora", "logseq", "roam research", "flomo", "wolai", "xmind", "mindmanager",
        "滴答清单", "ticktick", "todoist", "evernote", "印象笔记", "有道云笔记", "adobe acrobat", "acrobat", "foxit",
        "sumatrapdf", "neat reader", "neatreader", "neat converter", "calibre", "z library", "zotero", "anki",
        "腾讯文档", "石墨文档", "geogebra", "有道词典", "网易有道翻译",
    ]),
    ("tool", &[
        "google chrome", "chrome", "firefox", "mozilla firefox", "360安全浏览器", "360浏览器", "brave", "opera", "everything",
        "bandizip", "7 zip", "winrar", "火绒安全", "火绒", "huorong", "360安全卫士", "powertoys", "clash", "clash verge",
        "clash for windows", "mihomo party", "v2rayn", "todesk", "向日葵", "sunlogin", "anydesk", "nomachine", "teamviewer",
        "motrix", "idm", "internet download manager", "迅雷", "thunder", "百度网盘", "baidunetdisk", "夸克网盘", "quark",
        "阿里云盘", "adrive", "pikpak", "onedrive", "microsoft onedrive", "dropbox", "localsend", "crystaldiskinfo",
        "crystaldiskmark", "diskgenius", "windirstat", "wiztree", "sharex", "snipaste", "pixpin", "utools", "quicker",
        "listary", "autohotkey", "ditto", "translucenttb", "rainmeter", "manictime", "cold turkey", "logitech g hub",
        "razer synapse", "华为云盘", "raidrive", "向日葵远程控制", "向日葵", "电脑管家", "华为电脑管家", "跨屏互联",
        "geek uninstaller", "revo uninstaller", "dism", "7zip",
    ]),
    ("media", &[
        "网易云音乐", "cloudmusic", "qq音乐", "qqmusic", "酷狗音乐", "kugou", "酷我音乐", "spotify", "foobar2000",
        "potplayer", "vlc", "vlc media player", "mpv", "爱奇艺", "iqiyi", "腾讯视频", "优酷", "哔哩哔哩", "bilibili",
        "芒果tv", "kodi", "plex",
    ]),
    ("system", &[
        "nvidia", "nvidia 驱动", "intel 驱动", "amd 驱动", "realtek 驱动", "microsoft edge", "edge", "windows sdk",
        "microsoft visual c++", "directx", "windows 组件", "应用商店应用", "googleupdater", "google updater",
        "hms core", "wacom 数位板", "wacom", "nvidia physx", "intel graphics", "realtek audio", "edgewebview",
        "microsoft edge webview2 runtime", "edgecore", "edgeupdate", "公共组件",
    ]),
];

const PUBLISHER_HINTS: &[(&str, &str)] = &[
    ("valve", "game"),
    ("mihoyo", "game"),
    ("cognosphere", "game"),
    ("hoyoverse", "game"),
    ("epic games", "game"),
    ("riot games", "game"),
    ("blizzard", "game"),
    ("ubisoft", "game"),
    ("electronic arts", "game"),
    ("rockstar games", "game"),
    ("adobe", "create"),
    ("autodesk", "create"),
    ("image line", "create"),
    ("jetbrains", "dev"),
    ("python software foundation", "dev"),
    ("github", "dev"),
    ("docker", "dev"),
    ("anthropic", "dev"),
    ("nvidia", "system"),
    ("intel", "system"),
    ("realtek", "system"),
    ("advanced micro devices", "system"),
];

/// 发布者（已归一化）是不是这家公司：完全相同，或者后面只多了公司后缀
/// （「valve corporation」是 valve，「claude usage monitor」不是 claude）
pub fn publisher_is(publisher: &str, name: &str) -> bool {
    if publisher == name {
        return true;
    }
    let Some(rest) = publisher.strip_prefix(name).and_then(|r| r.strip_prefix(' ')) else { return false };
    matches!(
        rest.split(' ').next().unwrap_or(""),
        "corporation" | "corp" | "inc" | "co" | "ltd" | "limited" | "llc" | "gmbh" | "software" | "technologies"
            | "technology" | "tech" | "company" | "games" | "studios" | "entertainment" | "interactive" | "network"
            | "computer" | "科技" | "网络" | "软件"
    )
}

/// 按名字 / 发布者给出用途
pub fn purpose_hint<'a>(keys: impl IntoIterator<Item = &'a String>, publisher: &str) -> Option<&'static str> {
    let keys: Vec<&String> = keys.into_iter().collect();
    for (purpose, names) in PURPOSE_HINTS {
        for hint in names.iter() {
            let hint = norm(hint);
            let versioned = |k: &str| {
                k.strip_prefix(hint.as_str()).is_some_and(|rest| {
                    let rest = rest.trim();
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == ' ')
                })
            };
            if keys.iter().any(|k| **k == hint || (k.len() > hint.len() && k.starts_with(&hint) && k.as_bytes()[hint.len()] == b' ') || versioned(k)) {
                return Some(purpose);
            }
        }
    }
    let p = norm(publisher);
    PUBLISHER_HINTS
        .iter()
        .find(|(name, _)| publisher_is(&p, name))
        .map(|(_, purpose)| *purpose)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_and_match() {
        assert_eq!(norm(".vscode"), "vscode");
        assert_eq!(norm("Clash Verge"), "clash verge");
        assert_eq!(norm("io.github.clash-verge-rev"), "io github clash verge rev");
        assert_eq!(norm("DingTalk_108"), "dingtalk 108");
        assert!(match_score("cursor", "cursor") > match_score("cursor updater", "cursor"));
        assert!(match_score("cursor updater", "cursor") > 0);
        assert_eq!(match_score("codex", "code"), 0, "不是整词开头不算");
        assert!(match_score("clashverge", "clash verge") > 0);
        assert!(match_score("telegram", "telegram desktop") > 0, "软件名多出的是泛泛后缀可以");
        assert_eq!(match_score("claude", "claude usage monitor"), 0, "多出的是实词不行");
        assert_eq!(match_score("qq", "q"), 0);
    }

    #[test]
    fn cleans_display_names() {
        assert_eq!(clean_name("Python 3.11.4 (64-bit)"), "Python");
        assert_eq!(clean_name("7-Zip 23.01 (x64)"), "7-Zip");
        assert_eq!(clean_name("Microsoft Office 专业增强版 2019 - zh-cn"), "Microsoft Office 专业增强版");
        assert_eq!(clean_name("Godot_v4.4.1-stable_win64"), "Godot");
        assert_eq!(clean_name("ryujinx-1.1.1053-win_x64"), "ryujinx");
        assert_eq!(clean_name("微信"), "微信");
        assert_eq!(clean_name("Remove Z-Library"), "Z-Library");
        assert_eq!(clean_name("MW活跃度监控 版本 1.2.3"), "MW活跃度监控");
        assert_eq!(clean_name("Steam"), "Steam");
    }

    #[test]
    fn hints_by_name_and_publisher() {
        let k = |s: &str| vec![norm(s)];
        assert_eq!(purpose_hint(&k("Steam"), ""), Some("game"));
        assert_eq!(purpose_hint(&k("Adobe Photoshop 2025"), "Adobe"), Some("create"));
        assert_eq!(purpose_hint(&k("微信"), "Tencent"), Some("chat"));
        assert_eq!(purpose_hint(&k("Something Odd"), "Valve Corporation"), Some("game"));
        assert_eq!(purpose_hint(&k("Something Odd"), "Nobody"), None);
        assert_eq!(purpose_hint(&k("Python310"), ""), Some("dev"), "名字后面跟版本号");
        assert!(publisher_is("valve corporation", "valve"));
        assert!(!publisher_is("claude usage monitor", "claude"));
    }

    #[test]
    fn junk_cache_excludes_installer_cache() {
        assert!(is_junk_cache("GPUCache"));
        assert!(is_junk_cache("Code Cache"));
        assert!(!is_junk_cache("Package Cache"));
        assert!(!is_junk_cache("LocalCache"));
        assert!(is_personal_folder_name("code"));
        assert!(!is_personal_folder_name("Steam"));
        assert!(!is_junk_cache("Documents"));
    }

    #[test]
    fn personal_hints_by_name_and_contents() {
        let kids = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(personal_hint("1528919811", &kids(&["nt_qq", "FileRecv"])), Some("p-chat"), "QQ 号文件夹");
        assert_eq!(personal_hint("nt_qq", &[]), Some("p-chat"));
        assert_eq!(personal_hint("my files", &kids(&["game", "nt_qq", "录屏", "WeChat Files"])), None, "杂物文件夹里有聊天记录不代表整个是");
        assert_eq!(personal_hint("Disky", &kids(&["src", ".git"])), Some("p-code"));
        assert_eq!(personal_hint("录屏", &[]), Some("p-media"));
    }

    #[test]
    fn devenv_hints() {
        let k = |s: &str| vec![norm(s)];
        assert_eq!(devenv_box(&k("Go")), Some("env-lang"));
        assert_eq!(devenv_box(&k("Go Programming Language amd64 go1.22.3")), Some("env-lang"));
        assert_eq!(devenv_box(&k("GoPro Quik")), None);
        assert_eq!(devenv_box(&k("Python310")), Some("env-python"));
        assert_eq!(jdk_display("jdk-21.0.11+10").as_deref(), Some("JDK 21.0.11"));
        assert_eq!(jdk_display("jdk1.8.0_301").as_deref(), Some("JDK 1.8.0"));
        assert_eq!(jdk_display("jdk"), None);
        assert_eq!(jdk_display("jdkstuff"), None);
    }
}
