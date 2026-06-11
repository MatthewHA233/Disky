use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, State};
use tokio::sync::mpsc;

pub struct FsWatchState {
    /// Holds the active watcher — dropping it stops the watch.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// The directory currently being watched (= navPath from frontend).
    watched_dir: Arc<Mutex<String>>,
    /// Sender to push new watch-target paths into the debounce task.
    path_tx: Mutex<Option<mpsc::Sender<String>>>,
    /// Sender to push "something changed" signals.
    event_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl FsWatchState {
    pub fn new() -> Self {
        Self {
            watcher: Mutex::new(None),
            watched_dir: Arc::new(Mutex::new(String::new())),
            path_tx: Mutex::new(None),
            event_tx: Mutex::new(None),
        }
    }
}

fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_string()
}

#[tauri::command]
pub async fn start_watch(
    root: String,
    app: AppHandle,
    watch_state: State<'_, FsWatchState>,
) -> Result<(), String> {
    // Stop any existing watcher
    *watch_state.watcher.lock().unwrap() = None;

    let watched_dir = Arc::clone(&watch_state.watched_dir);
    // Set initial watch target to root
    *watched_dir.lock().unwrap() = norm(&root);

    let (event_tx, mut event_rx) = mpsc::channel::<()>(64);
    let (path_tx, mut path_rx) = mpsc::channel::<String>(16);

    *watch_state.event_tx.lock().unwrap() = Some(event_tx.clone());
    *watch_state.path_tx.lock().unwrap() = Some(path_tx);

    // Build a watcher that watches the initial nav_path (non-recursive)
    let event_tx_cb = event_tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };
            match event.kind {
                EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                    let _ = event_tx_cb.blocking_send(());
                }
                _ => {}
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    // Watch just the root dir to start (non-recursive — no sub-dir handles!)
    watcher
        .watch(Path::new(&norm(&root)), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    *watch_state.watcher.lock().unwrap() = Some(watcher);

    // Spawn the combined debounce + path-switch + refresh task
    tokio::spawn(async move {
        use crate::commands::scan::{find_node_mut, ScanState};
        use crate::scanner::walk::{scan_tree, ScanProgress};
        use std::sync::atomic::AtomicU64;
        use tauri::{Emitter, Manager};

        let debounce = Duration::from_millis(800);
        let mut pending = false;
        let mut deadline = tokio::time::Instant::now();

        loop {
            tokio::select! {
                // New nav path from frontend — switch watch target (handled by set_watch_path)
                new_path = path_rx.recv() => {
                    let Some(_) = new_path else { break };
                    // path already updated in watched_dir by set_watch_path;
                    // cancel any pending refresh for the old dir
                    pending = false;
                }

                // File system event on the current watched dir
                result = event_rx.recv() => {
                    if result.is_none() { break; }
                    pending = true;
                    deadline = tokio::time::Instant::now() + debounce;
                }

                // Debounce timer fired — do the refresh
                _ = tokio::time::sleep_until(deadline), if pending => {
                    pending = false;
                    let nav = watched_dir.lock().unwrap().clone();
                    if nav.is_empty() { continue; }

                    let nav_clone = nav.clone();
                    let scanned = tokio::task::spawn_blocking(move || {
                        let progress = Arc::new(ScanProgress {
                            files_scanned: AtomicU64::new(0),
                            dirs_scanned: AtomicU64::new(0),
                            total_size: AtomicU64::new(0),
                        });
                        scan_tree(&nav_clone, &progress)
                    })
                    .await;

                    let Ok(scanned) = scanned else { continue };

                    let scan_state = app.state::<ScanState>();
                    let mut guard = scan_state.0.lock().unwrap();
                    if let Some(tree) = guard.as_mut() {
                        if let Some(node) = find_node_mut(tree, &nav) {
                            node.children = scanned.children;
                            node.logical_size = scanned.logical_size;
                            node.files = scanned.files;
                            node.subdirs = scanned.subdirs;

                            let mut entries: Vec<_> =
                                node.children.iter().map(|c| c.to_dir_entry()).collect();
                            entries.sort_by(|a, b| b.logical_size.cmp(&a.logical_size));
                            drop(guard);
                            let _ = app.emit("live-update", entries);
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_watch(watch_state: State<'_, FsWatchState>) {
    *watch_state.watcher.lock().unwrap() = None;
    *watch_state.event_tx.lock().unwrap() = None;
    *watch_state.path_tx.lock().unwrap() = None;
}

/// Called by frontend whenever navPath changes.
/// Switches the watcher to the new directory (non-recursive, single handle).
#[tauri::command]
pub fn set_watch_path(path: String, watch_state: State<'_, FsWatchState>) {
    let new_dir = norm(&path);
    let old_dir = {
        let mut d = watch_state.watched_dir.lock().unwrap();
        let old = d.clone();
        *d = new_dir.clone();
        old
    };

    if old_dir == new_dir { return; }

    // Swap the watcher target: unwatch old, watch new (non-recursive)
    let mut watcher_guard = watch_state.watcher.lock().unwrap();
    if let Some(w) = watcher_guard.as_mut() {
        if !old_dir.is_empty() {
            let _ = w.unwatch(Path::new(&old_dir));
        }
        let _ = w.watch(Path::new(&new_dir), RecursiveMode::NonRecursive);
    }

    // Signal the task that the path changed (cancels pending debounce)
    if let Some(tx) = watch_state.path_tx.lock().unwrap().as_ref() {
        let _ = tx.blocking_send(new_dir);
    }
}
