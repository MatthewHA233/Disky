# Disky - 磁盘分析工具

- **核心功能**: Python 多进程磁盘扫描器，生成矩形分块图(treemap)可视化 + Markdown 报告，文件(橙色)/文件夹(蓝色)区分
- **关键文件**: `disk_analyzer.py` 主程序，`config.json` 配置，报告输出到 `reports/YYYY-MM/disk_report_YYYYMMDD_HHMMSS.md`
- **技术特点**: 多进程并行扫描，squarify 生成 treemap，支持 8 层深度递归，自动按年月归档报告
