const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatSize(bytes: number): string {
  let i = 0;
  let size = bytes;
  while (size >= 1024 && i < UNITS.length - 1) {
    size /= 1024;
    i++;
  }
  return `${size.toFixed(1)} ${UNITS[i]}`;
}

export function formatNumber(n: number): string {
  return n.toLocaleString();
}

/** 大小变化：+2.1 GB / −300.0 MB */
export function formatDelta(bytes: number): string {
  return `${bytes >= 0 ? "+" : "−"}${formatSize(Math.abs(bytes))}`;
}

/** 多久以前：刚刚 / 12 分钟前 / 3 小时前 / 2 天前 */
export function timeAgo(unixSecs: number): string {
  const s = Math.max(0, Date.now() / 1000 - unixSecs);
  if (s < 90) return "刚刚";
  if (s < 3600) return `${Math.round(s / 60)} 分钟前`;
  if (s < 86400 * 2) return `${Math.round(s / 3600)} 小时前`;
  return `${Math.round(s / 86400)} 天前`;
}
