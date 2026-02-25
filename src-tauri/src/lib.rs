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
            commands::ai::load_ai_settings,
            commands::ai::save_ai_settings,
            commands::ai::send_chat_message,
            commands::ai::list_chat_messages,
            commands::ai::clear_chat_history,
            commands::ai::analyze_paths,
            commands::ai::save_ai_analysis,
            commands::ai::load_ai_analyses,
            commands::ai::load_all_ai_analyses,
            commands::notes::save_note,
            commands::notes::get_notes_for_path,
            commands::notes::delete_note,
            commands::notes::list_all_notes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
