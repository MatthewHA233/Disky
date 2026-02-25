import csv, os, subprocess, sys

CSV_PATH = "D:/my_pro/py_ob/Disky/scan_c.csv"
WINDIRSTAT = r"D:\Program Files (x86)\WinDirStat\WinDirStat.exe"

target_arg = sys.argv[1] if len(sys.argv) > 1 else "C:\\Users"
target = os.path.normpath(target_arg).lower()

# Scan if CSV doesn't exist
if not os.path.exists(CSV_PATH):
    print("正在扫描 C:\\ ...", file=sys.stderr)
    env = os.environ.copy()
    env["MSYS_NO_PATHCONV"] = "1"
    subprocess.run([WINDIRSTAT, "C:\\", "/savetocsv", CSV_PATH], env=env, check=True)

csv_path = CSV_PATH
items = []
with open(csv_path, "r", encoding="utf-8-sig") as f:
    reader = csv.reader(f)
    next(reader)
    for row in reader:
        if len(row) < 5:
            continue
        path = os.path.normpath(row[0])
        parent = os.path.dirname(path).lower()
        if parent != target:
            continue
        name = os.path.basename(path)
        files = int(row[1]) if row[1] else 0
        folders = int(row[2]) if row[2] else 0
        logical = int(row[3]) if row[3] else 0
        physical = int(row[4]) if row[4] else 0
        is_dir = files > 0 or folders > 0
        items.append((name, logical, physical, is_dir, files, folders))

items.sort(key=lambda x: x[1], reverse=True)

def hs(n):
    for u in ("B", "KB", "MB", "GB", "TB"):
        if abs(n) < 1024:
            return f"{n:.1f}{u}"
        n /= 1024
    return f"{n:.1f}PB"

for name, logical, physical, is_dir, files, folders in items[:20]:
    kind = "dir" if is_dir else "file"
    info = f"{files}files/{folders}folders" if is_dir else ""
    print(f"{name}|{logical}|{hs(logical)}|{physical}|{hs(physical)}|{kind}|{info}")
