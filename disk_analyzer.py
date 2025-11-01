import os
import json
import time
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed
import multiprocessing
from datetime import datetime
import squarify
import matplotlib.pyplot as plt
import matplotlib.patches as patches
import matplotlib
matplotlib.rcParams['font.sans-serif'] = ['Microsoft YaHei', 'SimHei', 'Arial Unicode MS']
matplotlib.rcParams['axes.unicode_minus'] = False  # 解决负号显示问题

CONFIG_FILE = "config.json"
REPORTS_BASE_DIR = "reports"
DEFAULT_CONFIG = {
    "root_path": "C:\\",
    "min_size_mb": 600,
    "treemap_depth": 8
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


def get_output_paths():
    """生成基于年月的输出路径和文件名"""
    now = datetime.now()
    year_month = now.strftime("%Y-%m")  # 2025-11
    timestamp = now.strftime("%Y%m%d_%H%M%S")  # 20251101_143025

    # 创建年月文件夹
    report_dir = os.path.join(REPORTS_BASE_DIR, year_month)
    image_dir = os.path.join(report_dir, "images")
    os.makedirs(image_dir, exist_ok=True)

    # 生成文件名
    md_filename = f"disk_report_{timestamp}.md"
    md_path = os.path.join(report_dir, md_filename)

    return {
        'report_dir': report_dir,
        'image_dir': image_dir,
        'md_path': md_path,
        'md_filename': md_filename
    }


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


def collect_folder_data(path, max_depth=8):
    """递归收集文件夹和文件数据（用于treemap），区分文件和文件夹"""
    items = []
    size_cache = {}  # 缓存已计算的文件夹大小

    def get_size_cached(folder_path):
        """带缓存的文件夹大小计算"""
        if folder_path in size_cache:
            return size_cache[folder_path]
        size = get_folder_size(folder_path)
        size_cache[folder_path] = size
        return size

    def scan_level(current_path, depth=0):
        if depth >= max_depth:
            return

        try:
            for entry in os.scandir(current_path):
                try:
                    if entry.is_file(follow_symlinks=False):
                        size = entry.stat(follow_symlinks=False).st_size
                        if size > 0:
                            items.append({
                                'name': entry.name,
                                'size': size,
                                'path': entry.path,
                                'type': 'file'  # 标记为文件
                            })
                    elif entry.is_dir(follow_symlinks=False):
                        sub_size = get_size_cached(entry.path)
                        if sub_size > 0:
                            items.append({
                                'name': entry.name,
                                'size': sub_size,
                                'path': entry.path,
                                'type': 'folder'  # 标记为文件夹
                            })
                            # 递归扫描子文件夹
                            if depth < max_depth - 1:
                                scan_level(entry.path, depth + 1)
                except (PermissionError, FileNotFoundError, OSError):
                    continue
        except (PermissionError, FileNotFoundError, OSError):
            pass

    scan_level(path)
    return items


def generate_treemap(folder_path, output_image_path, max_depth=8):
    """为指定文件夹生成矩形分块图，区分文件和文件夹"""
    print(f"  [生成treemap] {folder_path} (深度: {max_depth})")

    # 收集数据
    items = collect_folder_data(folder_path, max_depth)

    if not items:
        print(f"  [跳过] {folder_path} 无可用数据")
        return False

    # 按大小排序，取前50个最大项
    items.sort(key=lambda x: x['size'], reverse=True)
    items = items[:50]

    # 准备数据
    sizes = [item['size'] for item in items]
    labels = [f"{item['name']}\n{format_size(item['size'])}" for item in items]

    # 根据类型生成不同颜色：文件夹用蓝色系，文件用橙色系
    colors = []
    for item in items:
        if item['type'] == 'folder':
            colors.append('#4A90E2')  # 蓝色 - 文件夹
        else:
            colors.append('#F5A623')  # 橙色 - 文件

    # 创建图形
    fig, ax = plt.subplots(figsize=(20, 16), dpi=100)

    # 绘制treemap - 字体调大到14
    squarify.plot(sizes=sizes, label=labels, color=colors, alpha=0.7,
                  text_kwargs={'fontsize': 14, 'weight': 'bold', 'color': 'white'},
                  ax=ax, edgecolor='white', linewidth=2)

    plt.axis('off')
    plt.title(f"磁盘空间分析: {Path(folder_path).name}\n🔵 文件夹  🟠 文件",
              fontsize=20, weight='bold', pad=20)
    plt.tight_layout()

    # 保存图片
    plt.savefig(output_image_path, bbox_inches='tight', dpi=100)
    plt.close()

    print(f"  [已保存] {output_image_path}")
    return True


def analyze_folder_parallel(root, min_size_bytes, image_dir, max_depth=5, treemap_depth=8):
    """并行分析目录结构，输出 Markdown（按体积排序）+ 生成treemap"""
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
                folder_name = Path(sub_path).name
                line = f"{'#' * depth} {folder_name} - {format_size(folder_size)}\n"

                # 生成treemap，使用配置的深度
                image_filename = f"{folder_name}_{hash(sub_path) % 100000}.png"
                image_path = os.path.join(image_dir, image_filename)

                if generate_treemap(sub_path, image_path, treemap_depth):
                    # Obsidian格式嵌入图片（相对路径）
                    line += f"![[images/{image_filename}]]\n\n"

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
    treemap_depth = config.get("treemap_depth", 8)
    min_size_bytes = min_size_mb * 1024 * 1024

    # 获取输出路径
    paths = get_output_paths()

    print(f"开始并行分析：{root}")
    print(f"最小文件夹阈值：{min_size_mb} MB")
    print(f"Treemap深度：{treemap_depth} 层")
    print(f"使用 {max(2, multiprocessing.cpu_count() - 1)} 个进程")
    print(f"报告保存位置：{paths['md_path']}\n")

    report = analyze_folder_parallel(root, min_size_bytes, paths['image_dir'], treemap_depth=treemap_depth)
    Path(paths['md_path']).write_text(report, encoding="utf-8")

    elapsed = time.time() - start
    print(f"\n分析完成！")
    print(f"报告文件：{paths['md_path']}")
    print(f"图片目录：{paths['image_dir']}")
    print(f"耗时：{elapsed:.1f} 秒")


if __name__ == "__main__":
    main()
