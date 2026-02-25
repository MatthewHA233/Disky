import os
import json
import time
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed
import multiprocessing

CONFIG_FILE = "config.json"
OUTPUT_FILE = "disk_report.md"
DEFAULT_CONFIG = {
    "root_path": "C:\\",
    "min_size_mb": 600
}


def load_config():
    """加载或生成配置文件"""
    if not os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(DEFAULT_CONFIG, f, indent=4)
        print(f"[INIT] 已生成默认配置文件：{CONFIG_FILE}")
        return DEFAULT_CONFIG
    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        return json.load(f)


def get_folder_size(path):
    """递归计算文件夹大小，仅返回数字"""
    total = 0
    try:
        for entry in os.scandir(path):
            try:
                if entry.is_file(follow_symlinks=False):
                    total += entry.stat(follow_symlinks=False).st_size
                elif entry.is_dir(follow_symlinks=False):
                    total += get_folder_size(entry.path)
            except (PermissionError, FileNotFoundError):
                continue
    except (PermissionError, FileNotFoundError):
        pass
    return total


def folder_size_task(path):
    """多进程任务：返回 (path, size)"""
    return path, get_folder_size(path)


def format_size(size_bytes):
    gb = size_bytes / (1024 ** 3)
    if gb >= 1:
        return f"{gb:.1f}GB"
    mb = size_bytes / (1024 ** 2)
    return f"{mb:.1f}MB"


def analyze_folder_parallel(root, min_size_bytes, max_depth=5):
    """并行分析目录结构，输出 Markdown（按体积排序）"""
    executor = ProcessPoolExecutor(max_workers=max(2, multiprocessing.cpu_count() - 1))

    def scan_dir(path, depth=1):
        indent = "  " * (depth - 1)
        sub_results = []

        try:
            dirs = [entry.path for entry in os.scandir(path) if entry.is_dir(follow_symlinks=False)]
        except (PermissionError, FileNotFoundError):
            print(f"{indent}[跳过无权限] {path}")
            return ""

        futures = {executor.submit(folder_size_task, d): d for d in dirs}

        # 收集所有结果
        size_results = []
        for future in as_completed(futures):
            sub_path, folder_size = future.result()
            size_results.append((sub_path, folder_size))
            print(f"{indent}[扫描] {sub_path} -> {format_size(folder_size)}")

        # 按大小排序（大到小）
        size_results.sort(key=lambda x: x[1], reverse=True)

        # 递归构建 Markdown
        md_content = ""
        for sub_path, folder_size in size_results:
            if folder_size >= min_size_bytes:
                line = f"{'#' * depth} {Path(sub_path).name} - {format_size(folder_size)}\n"
                sub_md = ""
                if depth < max_depth:
                    sub_md = scan_dir(sub_path, depth + 1)
                md_content += line + sub_md

        return md_content

    md_report = scan_dir(root)
    executor.shutdown(wait=True)
    return md_report


def main():
    start = time.time()
    config = load_config()
    root = config.get("root_path", "C:\\")
    min_size_mb = config.get("min_size_mb", 600)
    min_size_bytes = min_size_mb * 1024 * 1024

    print(f"开始并行分析：{root}")
    print(f"最小文件夹阈值：{min_size_mb} MB")
    print(f"使用 {max(2, multiprocessing.cpu_count() - 1)} 个进程\n")

    report = analyze_folder_parallel(root, min_size_bytes)
    Path(OUTPUT_FILE).write_text(report, encoding="utf-8")

    elapsed = time.time() - start
    print(f"\n分析完成！报告已生成：{OUTPUT_FILE}")
    print(f"耗时：{elapsed:.1f} 秒")


if __name__ == "__main__":
    main()
