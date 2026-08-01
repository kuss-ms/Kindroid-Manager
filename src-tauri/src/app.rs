use std::sync::Arc;

use tauri::Manager;

use crate::commands::sync_registry::SyncRegistry;
use crate::commands::tauri_wrappers;
use crate::kindroid::http::HttpKindroidClient;
use crate::kindroid::KindroidClient;
use crate::storage::sqlite::SqliteRepository;
use crate::storage::Repository;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("kindroid-manager.db");
            let repo: Arc<dyn Repository> =
                Arc::new(SqliteRepository::open(&db_path).expect("open sqlite"));
            let client: Arc<dyn KindroidClient> = Arc::new(HttpKindroidClient::new());
            app.manage(repo);
            app.manage(client);
            app.manage(Arc::new(SyncRegistry::new()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tauri_wrappers::list_characters,
            tauri_wrappers::get_character,
            tauri_wrappers::save_character,
            tauri_wrappers::delete_character,
            tauri_wrappers::duplicate_character,
            tauri_wrappers::list_targets,
            tauri_wrappers::get_target,
            tauri_wrappers::save_target,
            tauri_wrappers::delete_target,
            tauri_wrappers::push_to_target,
            tauri_wrappers::push_create_new_kin,
            tauri_wrappers::list_push_history,
            tauri_wrappers::get_push_log,
            tauri_wrappers::import_share_image,
            tauri_wrappers::export_share_image,
            tauri_wrappers::set_character_image,
            tauri_wrappers::get_character_image,
            tauri_wrappers::get_settings,
            tauri_wrappers::set_settings,
            tauri_wrappers::token_status,
            tauri_wrappers::set_token,
            tauri_wrappers::clear_token,
            tauri_wrappers::test_token,
            tauri_wrappers::list_chat_messages,
            tauri_wrappers::search_chat,
            tauri_wrappers::toggle_chat_message_favourite,
            tauri_wrappers::chat_message_count,
            tauri_wrappers::get_chat_sync_state,
            tauri_wrappers::get_current_sync,
            tauri_wrappers::start_chat_sync,
            tauri_wrappers::cancel_chat_sync,
            tauri_wrappers::reset_chat_history,
            tauri_wrappers::list_journal_entries,
            tauri_wrappers::save_journal_entry,
            tauri_wrappers::delete_journal_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).try_init();
}
