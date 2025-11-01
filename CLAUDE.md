# Disky - 磁盘分析工具

- **核心功能**: Python 多进程磁盘扫描器，递归分析文件夹大小（≥阈值MB），生成按体积排序的 Markdown 报告
- **关键文件**: `disk_analyzer.py` 主程序，`config.json` 配置（扫描路径/大小阈值），`disk_report.md` 输出报告
- **技术特点**: 使用 `ProcessPoolExecutor` 并行扫描，处理权限错误，支持深度限制（默认5层）
