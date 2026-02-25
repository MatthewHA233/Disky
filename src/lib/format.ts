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
