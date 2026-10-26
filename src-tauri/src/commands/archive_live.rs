//! 档案馆的实时更新：盯住扫过的每块盘（外接盘除外），哪里变了就只重数那一处，更新那块盘的摘要，
//! 档案馆跟着刷新，不用重扫整盘。启动时先核对一遍摘要里的位置还在不在（Disky 没开着时删掉的）。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::archive::digest::{self, Change, DriveDigest, LayerCache, Remeasure, Unit};
use crate::archive::fswatch::{self, FsEvent};
use crate::scanner::{is_strict_ancestor, normalize_path, SEP};

const TICK: Duration = Duration::from_secs(5);
/// 一个位置安静这么久才重数（解压、安装时一直在写）
const QUIET: Duration = Duration::from_secs(5);
/// 一直在变的位置（浏览器缓存）最多攒这么久重数一次
const MAX_WAIT: Duration = Duration::from_secs(120);
/// 攒着没处理的路径上限：再多就当漏了，改成核对整块盘
const MAX_PENDING: usize = 50_000;
/// 漏了变化时核对整块盘（几千次查文件在不在）：同一块盘隔这么久才再核对
const CHECK_GAP: Duration = Duration::from_secs(60);
/// 一处重数花了多久，至少隔它的这么多倍才再数：一直在变的大目录（系统临时目录）最多占这一处 2% 的时间，
/// 中间的变化攒着，下次一起算
const COST_RATIO: u32 = 50;
/// 变化不到这么多不写摘要：下次重数同一处时，差量是和摘要里的旧值比，会一起算上
const SAVE_MIN: i64 = 1024 * 1024;
/// 一处变了这么多（装了、删了个软件，下载了个大文件）马上让档案馆重新读；
/// 零碎的小变化攒够这么多也读，但隔得久一些（重建一次档案馆要过一遍所有盘的摘要）
const NOTIFY_MIN: i64 = 10 * 1024 * 1024;
const NOTIFY_GAP: Duration = Duration::from_secs(5);
const NOTIFY_GAP_SMALL: Duration = Duration::from_secs(300);

struct Seen {
    path: String,
    /// 收到这条变化的那块盘
    root: String,
    first: Instant,
    last: Instant,
}

#[derive(Default)]
struct Pending {
    /// 小写路径 → 原样路径和最早、最近一次变化的时间
    paths: HashMap<String, Seen>,
    /// 漏了变化、要核对的盘
    lost: HashSet<String>,
}

#[derive(Default)]
struct Live {
    pending: Mutex<Pending>,
    /// 正在盯的盘
    watched: Mutex<HashSet<String>>,
    /// 正在整盘扫描的盘：扫描结束会存一份新摘要，期间的变化先攒着，存完再按新摘要重数
    scanning: Mutex<HashSet<String>>,
    /// Disky 自己的数据目录（小写）：摘要、数据库、WebView 缓存一直在写，不算
    ignored: Mutex<Vec<String>>,
}

#[derive(Default)]
pub struct LiveState(Arc<Live>);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 扫描时跳过的位置（macOS 的 /System/Volumes、/Volumes 等）里的变化不算；只看盘根以下的部分
fn skipped(root: &str, path: &str) -> bool {
    let mut cur = path;
    while is_strict_ancestor(root, cur) {
        if crate::scanner::walk::should_skip_dir(cur) {
            return true;
        }
        match cur.rsplit_once(SEP) {
            Some((head, _)) if !head.is_empty() => cur = head,
            _ => break,
        }
    }
    false
}

impl Live {
    fn note(&self, root: &str, path: String) {
        let lower = path.to_ascii_lowercase();
        if lock(&self.ignored).iter().any(|d| lower == *d || is_strict_ancestor(d, &lower)) || skipped(root, &path) {
            return;
        }
        let now = Instant::now();
        let mut p = lock(&self.pending);
        if let Some(seen) = p.paths.get_mut(&lower) {
            seen.last = now;
        } else if p.paths.len() >= MAX_PENDING {
            p.lost.insert(root.to_string());
        } else {
            p.paths.insert(lower, Seen { path, root: root.to_string(), first: now, last: now });
        }
    }

    /// 安静够久、或者等得够久的路径，以及要核对的盘；正在整盘扫描的盘先不动
    fn take_ready(&self, now: Instant) -> (Vec<String>, HashSet<String>) {
        let scanning = lock(&self.scanning).clone();
        let mut p = lock(&self.pending);
        let mut ready = Vec::new();
        p.paths.retain(|_, seen| {
            let due = !scanning.contains(&seen.root) && (now - seen.last >= QUIET || now - seen.first >= MAX_WAIT);
            if due {
                ready.push(std::mem::take(&mut seen.path));
            }
            !due
        });
        let lost = p.lost.iter().filter(|r| !scanning.contains(*r)).cloned().collect::<HashSet<_>>();
        p.lost.retain(|r| scanning.contains(r));
        (ready, lost)
    }
}

/// 每处上次重数的时间和花了多久
#[derive(Default)]
struct Costs(HashMap<String, (Instant, Duration)>);

impl Costs {
    fn key(u: &Unit) -> String {
        let kind = if matches!(u, Unit::Whole(_)) { "w:" } else { "l:" };
        format!("{kind}{}", u.path().to_ascii_lowercase())
    }

    /// 还在冷却的话，什么时候能再数
    fn ready_at(&self, u: &Unit, now: Instant) -> Option<Instant> {
        let (at, took) = self.0.get(&Self::key(u))?;
        let at = *at + took.saturating_mul(COST_RATIO);
        (at > now).then_some(at)
    }

    fn record(&mut self, u: &Unit, at: Instant, took: Duration) {
        self.0.retain(|_, (t, d)| at.duration_since(*t) < d.saturating_mul(COST_RATIO));
        self.0.insert(Self::key(u), (at, took));
    }
}

/// 攒着改动，够多了才让档案馆重新读
#[derive(Default)]
struct Notify {
    /// 有一处变化够大
    big: bool,
    /// 零碎变化的累计
    small: i64,
    last: Option<Instant>,
}

impl Notify {
    fn add(&mut self, c: Change) {
        if c.delta.abs() >= NOTIFY_MIN {
            self.big = true;
        } else {
            self.small = self.small.saturating_add(c.delta.abs());
        }
    }

    fn flush(&mut self, app: &AppHandle) {
        let since = self.last.map_or(Duration::MAX, |t| t.elapsed());
        let due = (self.big && since >= NOTIFY_GAP) || (self.small >= NOTIFY_MIN && since >= NOTIFY_GAP_SMALL);
        if due {
            let _ = app.emit("archive-changed", ());
            *self = Notify { last: Some(Instant::now()), ..Notify::default() };
        }
    }
}

/// 应用启动时调用：后台核对一遍各盘摘要，然后开始盯
pub fn start(app: &AppHandle) {
    let live = Arc::clone(&app.state::<LiveState>().0);
    let paths = app.path();
    *lock(&live.ignored) = [paths.app_data_dir(), paths.app_local_data_dir(), paths.app_cache_dir(), paths.app_log_dir()]
        .into_iter()
        .flatten()
        .map(|d| normalize_path(&d.to_string_lossy()).to_ascii_lowercase())
        .collect();
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("archive-live".into()).spawn(move || run(&app, &live));
    if let Err(e) = spawned {
        eprintln!("档案馆实时更新没能启动: {e}");
    }
}

/// 整盘扫描期间拿着：扫描一开始就盯着这块盘（扫描期间的变化不漏），变化先攒着，丢掉时（摘要存好了）再处理
pub struct ScanPause {
    live: Arc<Live>,
    root: String,
}

impl Drop for ScanPause {
    fn drop(&mut self) {
        lock(&self.live.scanning).remove(&self.root);
    }
}

/// 开始整盘扫描 `root`
pub fn scan_begin(app: &AppHandle, root: &str) -> ScanPause {
    let live = Arc::clone(&app.state::<LiveState>().0);
    lock(&live.scanning).insert(root.to_string());
    watch_root(&live, root.to_string());
    ScanPause { live, root: root.to_string() }
}

/// 正在实时更新的盘
pub fn watched(app: &AppHandle) -> HashSet<String> {
    lock(&app.state::<LiveState>().0.watched).clone()
}

fn watch_root(live: &Arc<Live>, root: String) {
    if fswatch::is_external(&root) || !lock(&live.watched).insert(root.clone()) {
        return;
    }
    let l = Arc::clone(live);
    let r = root.clone();
    let res = fswatch::watch(&root, move |ev| match ev {
        FsEvent::Changed(path) => l.note(&r, path),
        FsEvent::Overflow => {
            lock(&l.pending).lost.insert(r.clone());
        }
        FsEvent::Stopped => {
            lock(&l.watched).remove(&r);
        }
    });
    if let Err(e) = res {
        eprintln!("没法实时更新 {root}: {e}");
        lock(&live.watched).remove(&root);
    }
}

/// 核对一块盘：查文件在不在放在锁外面（几千次，机械盘上要好一会儿），锁里只去掉找到的
fn check(dir: &Path, root: &str) -> Option<Change> {
    let missing = digest::load(dir, root)?.missing_paths(digest::exists)?;
    let mut change = None;
    let res = digest::update(dir, root, |d| {
        change = d.forget_all(&missing);
        change.is_some()
    });
    match res {
        Ok(true) => change,
        Ok(false) => None,
        Err(e) => {
            eprintln!("核对 {root} 的摘要失败: {e}");
            None
        }
    }
}

fn run(app: &AppHandle, live: &Arc<Live>) {
    let Ok(dir) = super::archive::digest_dir(app) else { return };
    let mut notify = Notify::default();
    let roots: Vec<String> = digest::load_all(&dir).into_iter().map(|d| d.root).collect();
    for root in &roots {
        check(&dir, root).into_iter().for_each(|c| notify.add(c));
    }
    notify.flush(app);
    for root in roots {
        watch_root(live, root);
    }
    let mut checked: HashMap<String, Instant> = HashMap::new();
    let mut deferred: HashSet<String> = HashSet::new();
    let mut costs = Costs::default();
    let mut layers = LayerCache::default();
    // 还在冷却、这一轮没数的位置：到点了再数
    let mut later: HashMap<String, (Instant, String)> = HashMap::new();
    loop {
        std::thread::sleep(TICK);
        let now = Instant::now();
        let (mut ready, lost) = live.take_ready(now);
        later.retain(|_, (at, path)| {
            let due = *at <= now;
            if due {
                ready.push(std::mem::take(path));
            }
            !due
        });
        deferred.extend(lost);
        // 盘上一直很忙、事件老是溢出时，别每一轮都核对一遍
        let due: HashSet<String> = deferred.iter().filter(|r| checked.get(*r).map_or(true, |t| t.elapsed() >= CHECK_GAP)).cloned().collect();
        deferred.retain(|r| !due.contains(r));
        for root in &due {
            checked.insert(root.clone(), Instant::now());
            // 漏了变化：只数一层时记下的小子目录大小不一定对了
            layers.forget_root(root);
            check(&dir, root).into_iter().for_each(|c| notify.add(c));
        }
        if !ready.is_empty() {
            let (changes, wait) = refresh(&dir, &ready, &mut costs, &mut layers);
            changes.into_iter().for_each(|c| notify.add(c));
            for (at, path) in wait {
                let slot = later.entry(path.to_ascii_lowercase()).or_insert((at, String::new()));
                *slot = (slot.0.max(at), path);
            }
        }
        notify.flush(app);
    }
}

/// 同一个位置几块盘都管时（macOS 的 / 和 /Volumes/X）归最具体的那块
fn owner<'a>(digests: &'a [DriveDigest], path: &str) -> Option<&'a DriveDigest> {
    digests.iter().filter(|d| d.covers(path)).max_by_key(|d| d.root.len())
}

/// 同一处只数一次，整个重数的位置里面的不用再数；深的排前面：
/// 只数一层的目录要用记着的子目录的大小，子目录得先更新
fn plan(units: Vec<Unit>) -> Vec<Unit> {
    let wholes: Vec<String> = units.iter().filter_map(|u| matches!(u, Unit::Whole(_)).then(|| u.path().to_string())).collect();
    let mut out: Vec<Unit> = Vec::new();
    for u in units {
        let p = u.path();
        let inside_whole = wholes.iter().any(|w| is_strict_ancestor(w, p) || (w.eq_ignore_ascii_case(p) && matches!(u, Unit::Layer(_))));
        let dup = out.iter().any(|o| std::mem::discriminant(o) == std::mem::discriminant(&u) && o.path().eq_ignore_ascii_case(p));
        if !inside_whole && !dup {
            out.push(u);
        }
    }
    out.sort_by_key(|u| std::cmp::Reverse(depth(u.path())));
    out
}

/// 路径有几层（盘根 C:、/ 为 0）
fn depth(path: &str) -> usize {
    let n = path.matches(SEP).count();
    if path.ends_with(SEP) { n.saturating_sub(1) } else { n }
}

/// 把这一批变化落到各盘摘要上，返回写进去的改动，和因为还在冷却、这一轮没数的路径（连同能再数的时间）
fn refresh(dir: &Path, ready: &[String], costs: &mut Costs, layers: &mut LayerCache) -> (Vec<Change>, Vec<(Instant, String)>) {
    let digests = digest::load_all(dir);
    let mut out = Vec::new();
    let mut wait = Vec::new();
    let now = Instant::now();
    for d in &digests {
        let mine: Vec<&String> = ready.iter().filter(|p| owner(&digests, p).is_some_and(|o| o.root == d.root)).collect();
        if mine.is_empty() {
            continue;
        }
        let dirs = d.dir_set();
        let mut units = Vec::new();
        let mut sources: HashMap<String, Vec<String>> = HashMap::new();
        for p in mine {
            match d.unit_for(&dirs, p) {
                Some(u) => match costs.ready_at(&u, now) {
                    Some(at) => wait.push((at, p.clone())),
                    None => {
                        sources.entry(Costs::key(&u)).or_default().push(p.clone());
                        units.push(u);
                    }
                },
                None => {}
            }
        }
        // 重数放在锁外面：大目录要数好一会儿
        let measured: Vec<(Unit, Remeasure)> = plan(units)
            .into_iter()
            .map(|u| {
                let at = Instant::now();
                let changed = sources.get(&Costs::key(&u)).map_or(&[][..], Vec::as_slice);
                let m = digest::measure(&d.root, &u, &dirs, changed, layers);
                costs.record(&u, at, at.elapsed());
                (u, m)
            })
            .collect();
        let mut changes = Vec::new();
        let saved = digest::update(dir, &d.root, |d| {
            let mut save = false;
            for (unit, m) in measured {
                if let Some(c) = d.apply(&unit, m) {
                    save |= c.reshaped || c.delta.abs() >= SAVE_MIN;
                    changes.push(c);
                }
            }
            save
        });
        match saved {
            Ok(true) => out.extend(changes),
            // 变化太小没写：下次重数同一处时一起算
            Ok(false) => {}
            Err(e) => eprintln!("实时更新 {} 的摘要失败: {e}", d.root),
        }
    }
    (out, wait)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn ready_paths_wait_for_quiet_or_max_wait() {
        let live = Live::default();
        let t0 = Instant::now();
        live.note("C:", "C:\\a\\x.txt".into());
        live.note("C:", "C:\\A\\X.TXT".into());
        assert_eq!(lock(&live.pending).paths.len(), 1, "大小写不同算同一个");
        assert!(live.take_ready(t0).0.is_empty(), "刚变的先等等");
        let (ready, lost) = live.take_ready(t0 + QUIET + Duration::from_millis(10));
        assert_eq!(ready, vec!["C:\\a\\x.txt".to_string()]);
        assert!(lost.is_empty());
        assert!(lock(&live.pending).paths.is_empty());
    }

    #[test]
    fn changes_wait_while_the_drive_is_being_scanned() {
        let live = Live::default();
        lock(&live.scanning).insert("D:".into());
        live.note("D:", "D:\\a\\x.txt".into());
        live.note("C:", "C:\\a\\y.txt".into());
        lock(&live.pending).lost.insert("D:".into());
        let later = Instant::now() + MAX_WAIT + QUIET;
        let (ready, lost) = live.take_ready(later);
        assert_eq!(ready, vec!["C:\\a\\y.txt".to_string()], "扫描中的盘先攒着");
        assert!(lost.is_empty());
        lock(&live.scanning).clear();
        let (ready, lost) = live.take_ready(later);
        assert_eq!((ready, lost.len()), (vec!["D:\\a\\x.txt".to_string()], 1), "扫完了再处理");
    }

    #[test]
    fn own_data_dirs_are_ignored() {
        let live = Live::default();
        *lock(&live.ignored) = vec!["c:\\users\\me\\appdata\\roaming\\com.disky.app".into()];
        live.note("C:", "C:\\Users\\me\\AppData\\Roaming\\com.disky.app\\archive\\digest-C.bin".into());
        live.note("C:", "C:\\Users\\me\\AppData\\Roaming\\other\\a".into());
        assert_eq!(lock(&live.pending).paths.len(), 1);
    }

    #[test]
    fn slow_places_cool_down_in_proportion_to_their_cost() {
        let mut costs = Costs::default();
        let t0 = Instant::now();
        let temp = Unit::Layer(r"D:\tmp".into());
        costs.record(&temp, t0, Duration::from_millis(800));
        assert_eq!(costs.ready_at(&temp, t0 + Duration::from_secs(30)), Some(t0 + Duration::from_secs(40)), "数一次 0.8 秒，40 秒后才再数");
        assert_eq!(costs.ready_at(&temp, t0 + Duration::from_secs(41)), None);
        assert_eq!(costs.ready_at(&Unit::Whole(r"D:\tmp".into()), t0), None, "整个重数和只数一层分开算");
    }

    #[test]
    fn plan_counts_each_place_once_deepest_first() {
        let units = vec![
            Unit::Layer("C:\\a".into()),
            Unit::Whole("C:\\a\\b".into()),
            Unit::Layer("C:\\a\\b".into()),
            Unit::Whole("C:\\a\\b\\c".into()),
            Unit::Layer("C:\\A".into()),
            Unit::Layer("C:".into()),
            Unit::Layer("C:\\ab".into()),
        ];
        assert_eq!(
            plan(units),
            vec![Unit::Whole("C:\\a\\b".into()), Unit::Layer("C:\\a".into()), Unit::Layer("C:\\ab".into()), Unit::Layer("C:".into())],
        );
    }
}
