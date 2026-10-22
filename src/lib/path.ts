/**
 * 跨平台路径工具：后端返回的路径在 Windows 用 "\"、macOS 用 "/"。
 * 所有对路径分隔符的判断/拼接都收口到这里，组件里不要再写死 "\\"。
 */

export const IS_WINDOWS =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

export const SEP = IS_WINDOWS ? "\\" : "/";

/** 系统垃圾桶的本地化名称（Windows 回收站 / macOS 废纸篓） */
export const TRASH_NAME = IS_WINDOWS ? "回收站" : "废纸篓";

/** 统一分隔符并去掉尾随分隔符；Unix 根目录 "/" 保留原样 */
export function normalizePath(p: string): string {
  const unified = IS_WINDOWS ? p.replace(/\//g, "\\") : p;
  const trimmed = unified.replace(IS_WINDOWS ? /\\+$/ : /\/+$/, "");
  return trimmed === "" && !IS_WINDOWS ? "/" : trimmed;
}

/** 拼接目录与子项名；处理 Unix 根目录避免 "//name" */
export function joinPath(dir: string, name: string): string {
  return dir.endsWith(SEP) ? dir + name : dir + SEP + name;
}

/** 取路径最后一段作为显示名；根路径（"/"、"C:"）返回其本身 */
export function basename(p: string): string {
  const norm = normalizePath(p);
  const idx = norm.lastIndexOf(SEP);
  const name = idx >= 0 ? norm.slice(idx + 1) : norm;
  return name === "" ? norm : name;
}

/** 父目录路径；已到根时返回 null */
export function parentPath(p: string): string | null {
  const norm = normalizePath(p);
  const idx = norm.lastIndexOf(SEP);
  if (idx < 0) return null;
  if (idx === 0) return norm === "/" ? null : "/"; // "/Users" → "/"
  return norm.slice(0, idx);
}

/** target 是否等于 root 或位于 root 之下（大小写不敏感，NTFS/APFS 默认均如此） */
export function isSameOrInside(root: string, target: string): boolean {
  const r = normalizePath(root).toLowerCase();
  const t = normalizePath(target).toLowerCase();
  if (t === r) return true;
  const prefix = r.endsWith(SEP) ? r : r + SEP;
  return t.startsWith(prefix);
}

/** root 到 target 的逐级路径（含两端）；target 不在 root 下时返回 [root, target] */
export function pathChain(root: string, target: string): string[] {
  const normRoot = normalizePath(root);
  const normTarget = normalizePath(target);
  if (normTarget === normRoot) return [normRoot];
  if (!isSameOrInside(normRoot, normTarget)) return [normRoot, normTarget];
  const relative = normTarget.slice(
    normRoot.endsWith(SEP) ? normRoot.length : normRoot.length + 1
  );
  const segments = relative.split(SEP).filter(Boolean);
  const chain = [normRoot];
  let current = normRoot;
  for (const seg of segments) {
    current = joinPath(current, seg);
    chain.push(current);
  }
  return chain;
}

/** 路径所在的盘：Windows 取盘符（"C"），其他平台取卷名（启动卷为 "/"） */
export function driveOf(p: string): string {
  if (IS_WINDOWS) return /^[A-Za-z]:/.test(p) ? p[0].toUpperCase() : "?";
  const m = /^\/Volumes\/([^/]+)/.exec(p);
  return m ? m[1] : "/";
}

/** 盘的显示名：「C 盘」；macOS 启动卷叫「系统盘」 */
export function driveLabel(drive: string): string {
  if (IS_WINDOWS) return `${drive} 盘`;
  return drive === "/" ? "系统盘" : drive;
}

/** 用户目录缩写成 ~，路径短一些（C:\Users\me\AppData → ~\AppData） */
export function shortenHome(p: string): string {
  return IS_WINDOWS ? p.replace(/^[A-Za-z]:\\Users\\[^\\]+/i, "~") : p.replace(/^\/Users\/[^/]+/, "~");
}
