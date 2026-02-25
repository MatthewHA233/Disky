mod commands;
mod scanner;

use commands::scan::ScanState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ScanState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::scan::list_drives,
            commands::scan::scan_disk,
            commands::scan::get_children,
            commands::cleanup::get_items_info,
            commands::cleanup::delete_items,
            commands::history::save_scan,
            commands::history::list_scans,
            commands::history::compare_scans,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
