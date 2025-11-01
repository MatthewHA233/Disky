import os
import json
import time
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed
import multiprocessing
from datetime import datetime
import plotly.graph_objects as go

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


def collect_folder_data_hierarchical(path, max_depth=8, parent_id="", current_depth=0):
    """递归收集文件夹和文件数据，构建层级关系（用于plotly treemap）"""
    labels = []
    parents = []
    values = []
    ids = []
    item_types = []  # 'file' or 'folder'

    if current_depth >= max_depth:
        return labels, parents, values, ids, item_types

    try:
        entries = list(os.scandir(path))
    except (PermissionError, FileNotFoundError, OSError):
        return labels, parents, values, ids, item_types

    for entry in entries:
        try:
            if entry.is_file(follow_symlinks=False):
                size = entry.stat(follow_symlinks=False).st_size
                if size > 1024 * 1024:  # 只记录 > 1MB 的文件
                    item_id = f"{parent_id}/{entry.name}" if parent_id else entry.name
                    labels.append(entry.name)
                    parents.append(parent_id)
                    values.append(size)
                    ids.append(item_id)
                    item_types.append('file')

            elif entry.is_dir(follow_symlinks=False):
                folder_size = get_folder_size(entry.path)
                if folder_size > 1024 * 1024:  # 只记录 > 1MB 的文件夹
                    item_id = f"{parent_id}/{entry.name}" if parent_id else entry.name
                    labels.append(entry.name)
                    parents.append(parent_id)
                    values.append(folder_size)
                    ids.append(item_id)
                    item_types.append('folder')

                    # 递归收集子项
                    if current_depth < max_depth - 1:
                        sub_labels, sub_parents, sub_values, sub_ids, sub_types = \
                            collect_folder_data_hierarchical(entry.path, max_depth, item_id, current_depth + 1)
                        labels.extend(sub_labels)
                        parents.extend(sub_parents)
                        values.extend(sub_values)
                        ids.extend(sub_ids)
                        item_types.extend(sub_types)

        except (PermissionError, FileNotFoundError, OSError):
            continue

    return labels, parents, values, ids, item_types


def generate_treemap(folder_path, output_image_path, max_depth=8):
    """为指定文件夹生成嵌套矩形分块图（使用plotly）"""
    print(f"  [生成treemap] {folder_path} (深度: {max_depth})")

    # 收集层级数据
    labels, parents, values, ids, item_types = collect_folder_data_hierarchical(
        folder_path, max_depth
    )

    if not labels:
        print(f"  [跳过] {folder_path} 无可用数据")
        return False

    # 不添加根节点，所有第一层的父节点设为空字符串
    # 这样可以避免根节点占用空间
    root_name = Path(folder_path).name

    # 根据类型设置颜色
    colors = []
    for item_type in item_types:
        if item_type == 'folder':
            colors.append('#4A90E2')  # 蓝色 - 文件夹
        else:
            colors.append('#F5A623')  # 橙色 - 文件

    # 创建 plotly treemap
    fig = go.Figure(go.Treemap(
        labels=labels,
        parents=parents,
        values=values,
        ids=ids,
        branchvalues="total",  # 父节点值等于所有子节点之和
        marker=dict(
            colors=colors,
            line=dict(width=3, color='white'),
            pad=dict(t=30, l=5, r=5, b=5)  # 增加顶部空间以显示标签
        ),
        textposition='top left',  # 标签位置改为左上角
        textfont=dict(size=16, family='Microsoft YaHei, SimHei, Arial', color='white'),
        hovertemplate='<b>%{label}</b><br>大小: %{value:,.0f} bytes<extra></extra>',
        pathbar_visible=False  # 隐藏路径导航栏
    ))

    fig.update_layout(
        title=dict(
            text=f"磁盘空间分析: {root_name}<br><sub style='color: #4A90E2;'>■ 文件夹</sub> <sub style='color: #F5A623;'>■ 文件</sub>",
            font=dict(size=24, family='Microsoft YaHei, SimHei, Arial')
        ),
        width=1800,
        height=1400,
        margin=dict(t=120, l=10, r=10, b=10),
        paper_bgcolor='white',
        plot_bgcolor='white'
    )

    # 保存为静态图片
    try:
        fig.write_image(output_image_path, format='png')
        print(f"  [已保存] {output_image_path}")
        return True
    except Exception as e:
        print(f"  [错误] 保存图片失败: {e}")
        return False


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
