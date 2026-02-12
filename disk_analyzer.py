import os
import sys
import json
import time
import hashlib
import string
import ctypes
import multiprocessing
from pathlib import Path
from dataclasses import dataclass, field
from concurrent.futures import ProcessPoolExecutor, as_completed
from datetime import datetime

import plotly.graph_objects as go


def is_junction(path: str) -> bool:
    """检测路径是否为 Windows junction point 或符号链接"""
    try:
        if os.path.islink(path):
            return True

        if os.name == 'nt':
            FILE_ATTRIBUTE_REPARSE_POINT = 0x400
            attrs = ctypes.windll.kernel32.GetFileAttributesW(str(path))
            if attrs != -1 and (attrs & FILE_ATTRIBUTE_REPARSE_POINT):
                return True

        return False
    except (OSError, ValueError):
        return False


@dataclass
class DirNode:
    """目录树节点，存储精确大小和子节点（仅 >1MB 的条目）"""
    name: str
    path: str
    size: int          # 精确总大小（包含所有子文件/子目录）
    is_file: bool
    children: list['DirNode'] = field(default_factory=list)


CONFIG_FILE = "config.json"
REPORTS_BASE_DIR = "reports"
DEFAULT_CONFIG = {
    "min_size_mb": 600,
    "treemap_depth": 8,
    "generate_treemap": True
}

MIN_DISPLAY_SIZE = 1024 * 1024  # 1MB: treemap 和子节点的最小显示阈值
MAX_RECURSION_DEPTH = 200       # _get_dir_size 的栈深度保护


def get_available_drives() -> list[str]:
    """检测系统中所有可用的磁盘驱动器"""
    drives = []
    if os.name == 'nt':
        for letter in string.ascii_uppercase:
            drive = f"{letter}:\\"
            if os.path.exists(drive) and os.path.isdir(drive):
                drives.append(drive)
    else:
        drives.append('/')
    return drives


def select_drive_interactive(available_drives: list[str]) -> list[str]:
    """交互式选择要扫描的磁盘"""
    print("\n" + "=" * 50)
    print("检测到以下可用磁盘：")
    print("=" * 50)

    for idx, drive in enumerate(available_drives, 1):
        print(f"{idx}. {drive}")

    print(f"{len(available_drives) + 1}. 扫描所有磁盘")
    print("=" * 50)

    while True:
        try:
            choice = input("\n请选择要扫描的磁盘编号（输入数字）: ").strip()
            choice_num = int(choice)

            if 1 <= choice_num <= len(available_drives):
                selected = [available_drives[choice_num - 1]]
                print(f"\n已选择: {selected[0]}")
                return selected
            elif choice_num == len(available_drives) + 1:
                print("\n已选择: 扫描所有磁盘")
                return available_drives
            else:
                print(f"无效选择，请输入 1-{len(available_drives) + 1} 之间的数字")
        except ValueError:
            print("请输入有效的数字")
        except KeyboardInterrupt:
            print("\n\n用户取消操作")
            sys.exit(0)


def load_config() -> dict:
    """加载或生成配置文件"""
    if not os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(DEFAULT_CONFIG, f, indent=4)
        print(f"[INIT] 已生成默认配置文件：{CONFIG_FILE}")
        return DEFAULT_CONFIG
    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        return json.load(f)


def get_output_paths() -> dict[str, str]:
    """生成基于年月的输出路径和文件名"""
    now = datetime.now()
    year_month = now.strftime("%Y-%m")
    timestamp = now.strftime("%Y%m%d_%H%M%S")

    report_dir = os.path.join(REPORTS_BASE_DIR, year_month)
    image_dir = os.path.join(report_dir, "images")
    os.makedirs(image_dir, exist_ok=True)

    md_filename = f"disk_report_{timestamp}.md"
    md_path = os.path.join(report_dir, md_filename)

    return {
        'report_dir': report_dir,
        'image_dir': image_dir,
        'md_path': md_path,
        'md_filename': md_filename
    }


def format_size(size_bytes: int) -> str:
    """格式化字节为可读大小"""
    gb = size_bytes / (1024 ** 3)
    if gb >= 1:
        return f"{gb:.1f}GB"
    mb = size_bytes / (1024 ** 2)
    return f"{mb:.1f}MB"


def _get_dir_size(path: str, _depth: int = 0) -> int:
    """仅计算目录大小，不构建树（用于超过 max_depth 的目录）"""
    if _depth > MAX_RECURSION_DEPTH:
        return 0
    total = 0
    try:
        for entry in os.scandir(path):
            try:
                if entry.is_file(follow_symlinks=False):
                    total += entry.stat(follow_symlinks=False).st_size
                elif entry.is_dir(follow_symlinks=False):
                    if is_junction(entry.path):
                        continue
                    total += _get_dir_size(entry.path, _depth + 1)
            except (OSError, PermissionError):
                continue
    except (OSError, PermissionError):
        pass
    return total


def build_dir_tree(path: str, max_depth: int, current_depth: int = 0) -> DirNode:
    """单次遍历构建目录树，自底向上计算精确大小。

    - total_size 始终精确（包含所有文件和子目录）
    - children 仅保留 > 1MB 的条目（用于显示）
    """
    name = os.path.basename(path) or path
    total_size = 0
    children: list[DirNode] = []

    try:
        entries = list(os.scandir(path))
    except (OSError, PermissionError):
        return DirNode(name=name, path=path, size=0, is_file=False)

    for entry in entries:
        try:
            if entry.is_file(follow_symlinks=False):
                file_size = entry.stat(follow_symlinks=False).st_size
                total_size += file_size
                if file_size > MIN_DISPLAY_SIZE:
                    children.append(DirNode(
                        name=entry.name,
                        path=entry.path,
                        size=file_size,
                        is_file=True,
                    ))

            elif entry.is_dir(follow_symlinks=False):
                if is_junction(entry.path):
                    continue

                if current_depth < max_depth - 1:
                    child_node = build_dir_tree(entry.path, max_depth, current_depth + 1)
                else:
                    dir_size = _get_dir_size(entry.path)
                    child_node = DirNode(
                        name=entry.name,
                        path=entry.path,
                        size=dir_size,
                        is_file=False,
                    )

                total_size += child_node.size
                if child_node.size > MIN_DISPLAY_SIZE:
                    children.append(child_node)

        except (OSError, PermissionError):
            continue

    # 按大小降序排列
    children.sort(key=lambda n: n.size, reverse=True)

    return DirNode(
        name=name,
        path=path,
        size=total_size,
        is_file=False,
        children=children,
    )


def tree_to_markdown(
    node: DirNode,
    depth: int,
    min_size_bytes: int,
    image_dir: str,
    treemap_depth: int,
    enable_treemap: bool,
) -> str:
    """从 DirNode 树递归生成 Markdown 报告（纯内存操作，无磁盘 I/O）"""
    md = ""

    for child in node.children:
        if child.is_file:
            continue
        if child.size < min_size_bytes:
            continue

        # depth=1 → ##, depth=2 → ###, ...
        heading = f"{'#' * (depth + 1)} {child.name} - {format_size(child.size)}\n"

        if enable_treemap:
            image_filename = _make_image_filename(child.name, child.path)
            image_path = os.path.join(image_dir, image_filename)

            if generate_treemap(child, image_path, treemap_depth):
                heading += f"![[images/{image_filename}]]\n\n"
        else:
            heading += "\n"

        sub_md = tree_to_markdown(
            child, depth + 1, min_size_bytes,
            image_dir, treemap_depth, enable_treemap,
        )
        md += heading + sub_md

    return md


def _make_image_filename(folder_name: str, folder_path: str) -> str:
    """生成确定性、无碰撞的图片文件名"""
    path_hash = hashlib.md5(os.path.normpath(folder_path).encode()).hexdigest()[:8]
    # 清理文件名中的非法字符
    safe_name = "".join(c if c.isalnum() or c in "-_" else "_" for c in folder_name)
    return f"{safe_name}_{path_hash}.png"


def tree_to_treemap_data(
    node: DirNode,
    parent_id: str = "",
) -> tuple[list[str], list[str], list[int], list[str], list[str]]:
    """从 DirNode 树提取 Plotly treemap 数据（纯内存操作）"""
    labels: list[str] = []
    parents: list[str] = []
    values: list[int] = []
    ids: list[str] = []
    item_types: list[str] = []

    for child in node.children:
        if child.size <= MIN_DISPLAY_SIZE:
            continue

        item_id = f"{parent_id}/{child.name}" if parent_id else child.name
        labels.append(child.name)
        parents.append(parent_id)
        values.append(child.size)
        ids.append(item_id)
        item_types.append('file' if child.is_file else 'folder')

        if not child.is_file and child.children:
            sub_labels, sub_parents, sub_values, sub_ids, sub_types = \
                tree_to_treemap_data(child, item_id)
            labels.extend(sub_labels)
            parents.extend(sub_parents)
            values.extend(sub_values)
            ids.extend(sub_ids)
            item_types.extend(sub_types)

    return labels, parents, values, ids, item_types


def generate_treemap(node: DirNode, output_image_path: str, max_depth: int) -> bool:
    """从 DirNode 生成嵌套矩形分块图（使用 Plotly）"""
    print(f"  [生成treemap] {node.path} (深度: {max_depth})")

    labels, parents, values, ids, item_types = tree_to_treemap_data(node)

    if not labels:
        print(f"  [跳过] {node.path} 无可用数据")
        return False

    colors = [
        '#4A90E2' if t == 'folder' else '#F5A623'
        for t in item_types
    ]

    fig = go.Figure(go.Treemap(
        labels=labels,
        parents=parents,
        values=values,
        ids=ids,
        branchvalues="total",
        marker=dict(
            colors=colors,
            line=dict(width=3, color='white'),
            pad=dict(t=30, l=5, r=5, b=5)
        ),
        textposition='top left',
        textfont=dict(size=16, family='Microsoft YaHei, SimHei, Arial', color='white'),
        hovertemplate='<b>%{label}</b><br>大小: %{value:,.0f} bytes<extra></extra>',
        pathbar_visible=False
    ))

    fig.update_layout(
        title=dict(
            text=(
                f"磁盘空间分析: {node.name}"
                "<br><sub style='color: #4A90E2;'>■ 文件夹</sub>"
                " <sub style='color: #F5A623;'>■ 文件</sub>"
            ),
            font=dict(size=24, family='Microsoft YaHei, SimHei, Arial')
        ),
        width=1800,
        height=1400,
        margin=dict(t=120, l=10, r=10, b=10),
        paper_bgcolor='white',
        plot_bgcolor='white'
    )

    try:
        fig.write_image(output_image_path, format='png')
        print(f"  [已保存] {output_image_path}")
        return True
    except Exception as e:
        print(f"  [错误] 保存图片失败: {e}")
        return False


def analyze_drive(
    root: str,
    min_size_bytes: int,
    image_dir: str,
    max_depth: int = 5,
    treemap_depth: int = 8,
    enable_treemap: bool = True,
) -> str:
    """并行构建目录树，生成 Markdown 报告 + 可选 treemap。

    对根目录的直接子目录并行调用 build_dir_tree，然后合并为根节点。
    """
    # 收集根目录的直接子目录和文件
    try:
        entries = list(os.scandir(root))
    except (OSError, PermissionError):
        print(f"[跳过无权限] {root}")
        return ""

    root_files: list[DirNode] = []
    dir_paths: list[str] = []
    root_file_size = 0

    for entry in entries:
        try:
            if entry.is_file(follow_symlinks=False):
                file_size = entry.stat(follow_symlinks=False).st_size
                root_file_size += file_size
                if file_size > MIN_DISPLAY_SIZE:
                    root_files.append(DirNode(
                        name=entry.name, path=entry.path,
                        size=file_size, is_file=True,
                    ))
            elif entry.is_dir(follow_symlinks=False):
                if not is_junction(entry.path):
                    dir_paths.append(entry.path)
        except (OSError, PermissionError):
            continue

    # 并行构建子目录树
    num_workers = max(2, multiprocessing.cpu_count() - 1)
    dir_nodes: list[DirNode] = []

    with ProcessPoolExecutor(max_workers=num_workers) as executor:
        futures = {
            executor.submit(build_dir_tree, d, treemap_depth, 1): d
            for d in dir_paths
        }
        for future in as_completed(futures):
            try:
                node = future.result()
                print(f"  [扫描] {node.path} -> {format_size(node.size)}")
                dir_nodes.append(node)
            except Exception as e:
                path = futures[future]
                print(f"  [错误] {path}: {e}")

    # 合并为根节点
    all_children = dir_nodes + root_files
    all_children.sort(key=lambda n: n.size, reverse=True)
    total_size = sum(n.size for n in dir_nodes) + root_file_size

    root_node = DirNode(
        name=os.path.basename(root) or root,
        path=root,
        size=total_size,
        is_file=False,
        children=all_children,
    )

    # 从树生成 Markdown
    return tree_to_markdown(
        root_node, depth=1, min_size_bytes=min_size_bytes,
        image_dir=image_dir, treemap_depth=treemap_depth,
        enable_treemap=enable_treemap,
    )


def main() -> None:
    start = time.time()
    config = load_config()
    min_size_mb = config.get("min_size_mb", 600)
    treemap_depth = config.get("treemap_depth", 8)
    enable_treemap = config.get("generate_treemap", True)
    min_size_bytes = min_size_mb * 1024 * 1024

    available_drives = get_available_drives()
    if not available_drives:
        print("错误：未检测到任何可用磁盘！")
        return

    selected_drives = select_drive_interactive(available_drives)

    paths = get_output_paths()

    print(f"\n最小文件夹阈值：{min_size_mb} MB")
    print(f"生成Treemap图片：{'是' if enable_treemap else '否'}")
    if enable_treemap:
        print(f"Treemap深度：{treemap_depth} 层")
    print(f"使用 {max(2, multiprocessing.cpu_count() - 1)} 个进程")
    print(f"报告保存位置：{paths['md_path']}\n")

    full_report = ""
    for idx, drive in enumerate(selected_drives, 1):
        print(f"\n{'=' * 60}")
        print(f"[{idx}/{len(selected_drives)}] 开始扫描: {drive}")
        print(f"{'=' * 60}\n")

        report = analyze_drive(
            drive, min_size_bytes, paths['image_dir'],
            treemap_depth=treemap_depth,
            enable_treemap=enable_treemap,
        )

        drive_name = drive.rstrip('\\').rstrip('/')
        full_report += f"# 磁盘 {drive_name}\n\n{report}\n\n"

    Path(paths['md_path']).write_text(full_report, encoding="utf-8")

    elapsed = time.time() - start
    print(f"\n{'=' * 60}")
    print("所有磁盘分析完成！")
    print(f"{'=' * 60}")
    print(f"报告文件：{paths['md_path']}")
    if enable_treemap:
        print(f"图片目录：{paths['image_dir']}")
    print(f"总耗时：{elapsed:.1f} 秒")


if __name__ == "__main__":
    main()
