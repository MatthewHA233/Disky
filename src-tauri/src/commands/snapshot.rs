//! 每块盘最近一次整盘扫描的完整目录树：压缩存一份（app 数据目录 snapshots/<盘>.bin），
//! 重启以后「目录」视图选这块盘直接打开，不用重扫。每块盘只留最新的一份（压缩后 C 盘约 25 MB）。
//! 删除了东西之后前端会让这里按内存里的树重存一遍，免得打开快照时删掉的东西又冒出来。
//!
//! 文件格式：DSKS | 版本 u32 | 扫描时间 i64 | 盘根长度 u16 | 盘根 UTF-8 | deflate(ScanTree::encode)

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::scan::ScanState;
use crate::archive::digest;
use crate::scanner::{DirEntry, ScanTree, ROOT};

const MAGIC: &[u8; 4] = b"DSKS";
const VERSION: u32 = 1;
/// 打开时最多等后台写完多久（压缩 C 盘那份一般几秒）
const SAVE_WAIT: Duration = Duration::from_secs(60);

/// 正在后台写的快照文件。扫完马上又打开同一块盘时等它写完再读，免得读到上一次的
static SAVING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static SAVED: Condvar = Condvar::new();

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SnapshotInfo {
    pub root: String,
    /// Unix 秒
    pub scanned_at: i64,
    /// 文件大小（压缩后）
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct LoadSnapshotResult {
    root_path: String,
    children: Vec<DirEntry>,
    scanned_at: i64,
}

pub(crate) fn snapshot_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("snapshots"))
}

fn file_for(dir: &Path, root: &str) -> PathBuf {
    let key: String = root.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let key = key.trim_matches('_');
    dir.join(format!("{}.bin", if key.is_empty() { "root" } else { key }))
}

/// 记上这块盘的快照正在写；交回来的东西放掉（写完或写失败）时去掉
pub struct Saving(PathBuf);

pub fn saving(dir: &Path, root: &str) -> Saving {
    let file = file_for(dir, root);
    SAVING.lock().unwrap_or_else(|e| e.into_inner()).push(file.clone());
    Saving(file)
}

impl Drop for Saving {
    fn drop(&mut self) {
        let mut list = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = list.iter().position(|f| *f == self.0) {
            list.swap_remove(i);
        }
        SAVED.notify_all();
    }
}

fn wait_saved(file: &Path) {
    let list = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let _ = SAVED.wait_timeout_while(list, SAVE_WAIT, |l| l.iter().any(|f| f == file));
}

/// 已经编码好的树压缩写盘：先写临时文件再改名，写到一半断电也不会坏掉旧的那份
pub fn save_blob(dir: &Path, root: &str, scanned_at: i64, blob: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = file_for(dir, root);
    let tmp = target.with_extension("tmp");
    let write = || -> std::io::Result<()> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&scanned_at.to_le_bytes())?;
        let root_bytes = root.as_bytes();
        out.write_all(&(root_bytes.len() as u16).to_le_bytes())?;
        out.write_all(root_bytes)?;
        let mut enc = flate2::write::DeflateEncoder::new(out, flate2::Compression::fast());
        enc.write_all(blob)?;
        enc.finish()?.flush()
    };
    write().map_err(|e| format!("保存扫描快照失败: {e}"))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("保存扫描快照失败: {e}"))
}

fn read_header(r: &mut impl Read) -> Option<(String, i64)> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).ok()?;
    let mut u32b = [0u8; 4];
    r.read_exact(&mut u32b).ok()?;
    if &magic != MAGIC || u32::from_le_bytes(u32b) != VERSION {
        return None;
    }
    let mut i64b = [0u8; 8];
    r.read_exact(&mut i64b).ok()?;
    let mut u16b = [0u8; 2];
    r.read_exact(&mut u16b).ok()?;
    let mut root = vec![0u8; u16::from_le_bytes(u16b) as usize];
    r.read_exact(&mut root).ok()?;
    Some((String::from_utf8(root).ok()?, i64::from_le_bytes(i64b)))
}

/// 各盘的快照（只读文件头）
pub fn list(dir: &Path) -> Vec<SnapshotInfo> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SnapshotInfo> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "bin"))
        .filter_map(|e| {
            let bytes = e.metadata().ok()?.len();
            let (root, scanned_at) = read_header(&mut std::fs::File::open(e.path()).ok()?)?;
            Some(SnapshotInfo { root, scanned_at, bytes })
        })
        .collect();
    out.sort_by(|a, b| a.root.cmp(&b.root));
    out
}

/// 读出某块盘的快照：(扫描时间, 树)
pub fn load(dir: &Path, root: &str) -> Result<(i64, ScanTree), String> {
    let path = file_for(dir, root);
    wait_saved(&path);
    let mut file = std::io::BufReader::new(std::fs::File::open(&path).map_err(|_| "这块盘还没有存下的扫描".to_string())?);
    let (_, scanned_at) = read_header(&mut file).ok_or("扫描快照的格式不对，重新扫描一次就好")?;
    let mut blob = Vec::new();
    flate2::read::DeflateDecoder::new(file).read_to_end(&mut blob).map_err(|e| format!("扫描快照读不出来: {e}"))?;
    Ok((scanned_at, ScanTree::decode(&blob)?))
}

#[tauri::command]
pub fn list_snapshots(app: AppHandle) -> Result<Vec<SnapshotInfo>, String> {
    Ok(list(&snapshot_dir(&app)?))
}

/// 打开某块盘上次存下的扫描，放进内存当作当前的扫描结果
#[tauri::command]
pub async fn load_snapshot(app: AppHandle, root: String, state: State<'_, ScanState>) -> Result<LoadSnapshotResult, String> {
    let dir = snapshot_dir(&app)?;
    let mine = super::scan::next_epoch();
    let (scanned_at, tree) = tokio::task::spawn_blocking(move || load(&dir, &root)).await.map_err(|e| e.to_string())??;
    let (_, children) = tree.child_entries(ROOT, 0, usize::MAX);
    let root_path = tree.root_path().to_string();
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    // 读的这几秒里开始了扫描、或者又打开了别的盘：读出来的不用了，别盖掉
    if super::scan::epoch() != mine {
        return Err("已经换成别的扫描结果了".into());
    }
    *guard = Some(tree);
    Ok(LoadSnapshotResult { root_path, children, scanned_at })
}

/// 删除过东西以后，按内存里的树重存一遍（扫描时间沿用原来那份的）
#[tauri::command]
pub async fn save_snapshot(app: AppHandle, state: State<'_, ScanState>) -> Result<(), String> {
    let (root, blob) = {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        let Some(tree) = guard.as_mut() else { return Ok(()) };
        if !digest::is_volume_root(tree.root_path()) {
            return Ok(());
        }
        tree.compact();
        (tree.root_path().to_string(), tree.encode())
    };
    let dir = snapshot_dir(&app)?;
    let saving = saving(&dir, &root);
    tokio::task::spawn_blocking(move || {
        let _saving = saving;
        let scanned_at = list(&dir).into_iter().find(|s| s.root.eq_ignore_ascii_case(&root)).map_or_else(super::archive::now_secs, |s| s.scanned_at);
        save_blob(&dir, &root, scanned_at, &blob)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::walk::{scan_tree_progressive, ScanProgress};

    #[test]
    fn roundtrip_and_list() {
        let base = std::env::temp_dir().join(format!("disky-snapshot-{}", std::process::id()));
        let data = base.join("data");
        std::fs::create_dir_all(data.join("sub")).unwrap();
        std::fs::write(data.join("sub").join("a.txt"), vec![7u8; 4096]).unwrap();
        std::fs::write(data.join("b.bin"), vec![1u8; 1024]).unwrap();
        let tree = scan_tree_progressive(&data.to_string_lossy(), &ScanProgress::default(), |_| {});
        let store = base.join("snapshots");
        save_blob(&store, "C:", 1_700_000_000, &tree.encode()).unwrap();
        assert_eq!(list(&store), vec![SnapshotInfo { root: "C:".into(), scanned_at: 1_700_000_000, bytes: std::fs::metadata(store.join("C.bin")).unwrap().len() }]);
        let (at, back) = load(&store, "C:").unwrap();
        assert_eq!((at, back.node_count(), back.root_path()), (1_700_000_000, tree.node_count(), tree.root_path()));
        assert!(load(&store, "D:").is_err(), "没存过的盘");
        std::fs::write(store.join("D.bin"), b"garbage").unwrap();
        assert_eq!(list(&store).len(), 1, "坏文件不列");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn opening_waits_for_a_save_in_progress() {
        let base = std::env::temp_dir().join(format!("disky-snapshot-wait-{}", std::process::id()));
        let data = base.join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("a.txt"), vec![7u8; 4096]).unwrap();
        let tree = scan_tree_progressive(&data.to_string_lossy(), &ScanProgress::default(), |_| {});
        let store = base.join("snapshots");
        save_blob(&store, "C:", 1, &tree.encode()).unwrap();
        // 刚扫完：新的那份还在后台写
        let pending = saving(&store, "C:\\");
        let (store2, blob) = (store.clone(), tree.encode());
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            save_blob(&store2, "C:", 2, &blob).unwrap();
            drop(pending);
        });
        let (at, _) = load(&store, "C:").unwrap();
        assert_eq!(at, 2, "等写完读到的是这次扫的");
        writer.join().unwrap();
        std::fs::remove_dir_all(&base).unwrap();
    }
}
