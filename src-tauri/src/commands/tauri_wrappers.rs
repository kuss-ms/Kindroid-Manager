// Tauri command wrappers. The `tauri::State` import and the
// `#[tauri::command]` macro pull in the Tauri runtime DLLs
// (WebView2Loader.dll) at link time, so this module is only compiled
// for the real app build, not for `cargo test`. The `run()` function
// in `lib.rs` is also gated to non-test, so `tauri::generate_handler!`
// never tries to look up these items in test mode.

#[cfg(not(test))]
mod inner {
    use crate::commands::{
        ai, characters, chat_automation, chat_history, history, journal, push, settings,
        share_code, targets,
    };
    use tauri::State;

    type Repo = std::sync::Arc<dyn crate::storage::Repository>;
    type Client = std::sync::Arc<dyn crate::kindroid::KindroidClient>;
    type Ai = std::sync::Arc<dyn crate::kindroid::ai::AiClient>;

    #[tauri::command]
    pub async fn list_characters(
        repo: State<'_, Repo>,
    ) -> Result<Vec<crate::domain::character::Character>, crate::error::AppError> {
        characters::list_characters(repo.inner().clone()).await
    }

    #[tauri::command]
    pub async fn get_character(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<crate::domain::character::Character, crate::error::AppError> {
        characters::get_character(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn save_character(
        repo: State<'_, Repo>,
        input: characters::CharacterInput,
    ) -> Result<crate::domain::character::Character, crate::error::AppError> {
        characters::save_character(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn delete_character(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<(), crate::error::AppError> {
        characters::delete_character(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn duplicate_character(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<crate::domain::character::Character, crate::error::AppError> {
        characters::duplicate_character(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn list_targets(
        repo: State<'_, Repo>,
    ) -> Result<Vec<crate::domain::target::Target>, crate::error::AppError> {
        targets::list_targets(repo.inner().clone()).await
    }

    #[tauri::command]
    pub async fn get_target(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<crate::domain::target::Target, crate::error::AppError> {
        targets::get_target(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn save_target(
        repo: State<'_, Repo>,
        input: targets::TargetInput,
    ) -> Result<crate::domain::target::Target, crate::error::AppError> {
        targets::save_target(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn delete_target(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<(), crate::error::AppError> {
        targets::delete_target(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn push_to_target(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
        req: push::PushRequest,
    ) -> Result<crate::error::PushResult, crate::error::AppError> {
        push::push_to_target(repo.inner().clone(), client.inner().clone(), req).await
    }

    #[tauri::command]
    pub async fn push_create_new_kin(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
        character_id: uuid::Uuid,
    ) -> Result<crate::error::CreateNewKinResult, crate::error::AppError> {
        push::push_create_new_kin(
            repo.inner().clone(),
            client.inner().clone(),
            push::CreateNewKinRequest { character_id },
        )
        .await
    }

    #[tauri::command]
    pub async fn list_push_history(
        repo: State<'_, Repo>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<crate::domain::push_log::PushLogEntry>, crate::error::AppError> {
        history::list_push_history(repo.inner().clone(), limit, offset).await
    }

    #[tauri::command]
    pub async fn get_push_log(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<crate::domain::push_log::PushLogEntry, crate::error::AppError> {
        history::get_push_log(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn import_share_image(
        repo: State<'_, Repo>,
        bytes: Vec<u8>,
    ) -> Result<crate::domain::character::Character, crate::error::AppError> {
        share_code::import_share_image(repo.inner().clone(), bytes).await
    }

    #[tauri::command]
    pub async fn export_share_image(
        repo: State<'_, Repo>,
        stash: State<'_, std::sync::Arc<share_code::ShareImageStash>>,
        id: uuid::Uuid,
    ) -> Result<Vec<u8>, crate::error::AppError> {
        share_code::export_share_image(repo.inner().clone(), stash.inner().clone(), id).await
    }

    #[tauri::command]
    pub fn take_stashed_share_image(
        stash: State<'_, std::sync::Arc<share_code::ShareImageStash>>,
    ) -> Option<Vec<u8>> {
        share_code::take_stashed_share_image(stash.inner().clone())
    }

    #[tauri::command]
    pub async fn set_character_image(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
        bytes: Vec<u8>,
    ) -> Result<crate::domain::character::Character, crate::error::AppError> {
        share_code::set_character_image(repo.inner().clone(), id, bytes).await
    }

    #[tauri::command]
    pub async fn get_character_image(
        repo: State<'_, Repo>,
        id: uuid::Uuid,
    ) -> Result<Option<Vec<u8>>, crate::error::AppError> {
        share_code::get_character_image(repo.inner().clone(), id).await
    }

    #[tauri::command]
    pub async fn get_settings(
        repo: State<'_, Repo>,
    ) -> Result<settings::SettingsDto, crate::error::AppError> {
        settings::get_settings(repo.inner().clone()).await
    }

    #[tauri::command]
    pub async fn set_settings(
        repo: State<'_, Repo>,
        input: settings::SetSettingsInput,
    ) -> Result<(), crate::error::AppError> {
        settings::set_settings(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub fn token_status() -> settings::TokenStatus {
        settings::token_status()
    }

    #[tauri::command]
    pub fn set_token(token: String) -> Result<(), crate::error::AppError> {
        settings::set_token(token)
    }

    #[tauri::command]
    pub async fn clear_token() -> Result<(), crate::error::AppError> {
        settings::clear_token()
    }

    #[tauri::command]
    pub async fn set_debug_flags(
        repo: State<'_, Repo>,
        input: settings::SetDebugFlagsInput,
    ) -> Result<(), crate::error::AppError> {
        settings::set_debug_flags(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn test_token(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
    ) -> Result<settings::TestTokenResult, crate::error::AppError> {
        settings::test_token(repo.inner().clone(), client.inner().clone()).await
    }

    #[tauri::command]
    pub async fn list_chat_messages(
        repo: State<'_, Repo>,
        ai_id: String,
        before_ts: Option<i64>,
        limit: u32,
        favourites_only: bool,
    ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, crate::error::AppError> {
        chat_history::list_chat_messages(
            repo.inner().clone(),
            ai_id,
            before_ts,
            limit,
            favourites_only,
        )
        .await
    }

    #[tauri::command]
    pub async fn search_chat(
        repo: State<'_, Repo>,
        ai_id: String,
        query: String,
        limit: u32,
        offset: u32,
        favourites_only: bool,
    ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, crate::error::AppError> {
        chat_history::search_chat(
            repo.inner().clone(),
            ai_id,
            query,
            limit,
            offset,
            favourites_only,
        )
        .await
    }

    #[tauri::command]
    pub async fn toggle_chat_message_favourite(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
        ai_id: String,
        kindroid_msg_id: String,
    ) -> Result<bool, crate::error::AppError> {
        chat_history::toggle_chat_message_favourite(
            repo.inner().clone(),
            client.inner().clone(),
            ai_id,
            kindroid_msg_id,
        )
        .await
    }

    #[tauri::command]
    pub async fn chat_message_count(
        repo: State<'_, Repo>,
        ai_id: String,
    ) -> Result<u64, crate::error::AppError> {
        chat_history::chat_message_count(repo.inner().clone(), ai_id).await
    }

    #[tauri::command]
    pub async fn get_chat_sync_state(
        repo: State<'_, Repo>,
        registry: State<'_, std::sync::Arc<crate::commands::sync_registry::SyncRegistry>>,
        ai_id: String,
    ) -> Result<Option<crate::domain::chat_message::ChatSyncState>, crate::error::AppError> {
        chat_history::get_chat_sync_state(repo.inner().clone(), registry.inner().clone(), ai_id)
            .await
    }

    #[tauri::command]
    pub async fn get_current_sync(
        registry: State<'_, std::sync::Arc<crate::commands::sync_registry::SyncRegistry>>,
    ) -> Result<Option<String>, crate::error::AppError> {
        chat_history::get_current_sync(registry.inner().clone()).await
    }

    #[tauri::command]
    pub async fn start_chat_sync(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
        ai_client: State<'_, Ai>,
        registry: State<'_, std::sync::Arc<crate::commands::sync_registry::SyncRegistry>>,
        ai_id: String,
        app: tauri::AppHandle,
    ) -> Result<(), crate::error::AppError> {
        chat_history::start_chat_sync(
            repo.inner().clone(),
            client.inner().clone(),
            ai_client.inner().clone(),
            registry.inner().clone(),
            ai_id,
            app,
        )
        .await
    }

    #[tauri::command]
    pub async fn cancel_chat_sync(
        registry: State<'_, std::sync::Arc<crate::commands::sync_registry::SyncRegistry>>,
    ) -> Result<(), crate::error::AppError> {
        chat_history::cancel_chat_sync(registry.inner().clone()).await
    }

    #[tauri::command]
    pub async fn reset_chat_history(
        repo: State<'_, Repo>,
        ai_id: String,
    ) -> Result<usize, crate::error::AppError> {
        chat_history::reset_chat_history(repo.inner().clone(), ai_id).await
    }

    #[tauri::command]
    pub async fn get_chat_automation_state(
        repo: State<'_, Repo>,
        ai_id: String,
    ) -> Result<chat_automation::ChatAutomationDto, crate::error::AppError> {
        chat_automation::get_chat_automation_state(repo.inner().clone(), ai_id).await
    }

    #[tauri::command]
    pub async fn set_chat_automation_settings(
        repo: State<'_, Repo>,
        input: chat_automation::SetChatAutomationSettingsInput,
    ) -> Result<chat_automation::ChatAutomationDto, crate::error::AppError> {
        chat_automation::set_chat_automation_settings(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn reset_chat_summary(
        repo: State<'_, Repo>,
        input: chat_automation::ResetChatSummaryInput,
    ) -> Result<chat_automation::ChatAutomationDto, crate::error::AppError> {
        chat_automation::reset_chat_summary(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn clear_stuck_auto_journal_runs(
        repo: State<'_, Repo>,
        input: chat_automation::ClearStuckAutoJournalRunsInput,
    ) -> Result<chat_automation::ClearStuckAutoJournalRunsResult, crate::error::AppError> {
        chat_automation::clear_stuck_auto_journal_runs(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn run_summary_now(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
        ai_client: State<'_, Ai>,
        input: chat_automation::RunSummaryNowInput,
    ) -> Result<chat_automation::RunSummaryNowResult, crate::error::AppError> {
        chat_automation::run_summary_now(
            repo.inner().clone(),
            client.inner().clone(),
            ai_client.inner().clone(),
            input,
        )
        .await
    }

    #[tauri::command]
    pub async fn get_automation_instructions_defaults(
    ) -> chat_automation::AutomationInstructionsDefaults {
        chat_automation::get_automation_instructions_defaults().await
    }

    #[tauri::command]
    pub async fn set_automation_instructions(
        repo: State<'_, Repo>,
        input: chat_automation::SetAutomationInstructionsInput,
    ) -> Result<(), crate::error::AppError> {
        chat_automation::set_automation_instructions(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn list_journal_entries(
        repo: State<'_, Repo>,
        character_id: uuid::Uuid,
    ) -> Result<Vec<crate::domain::journal_entry::JournalEntry>, crate::error::AppError> {
        journal::list_journal_entries(repo.inner().clone(), character_id).await
    }

    #[tauri::command]
    pub async fn save_journal_entry(
        repo: State<'_, Repo>,
        character_id: uuid::Uuid,
        input: crate::domain::journal_entry::JournalEntryInput,
    ) -> Result<crate::domain::journal_entry::JournalEntry, crate::error::AppError> {
        journal::save_journal_entry(repo.inner().clone(), character_id, input).await
    }

    #[tauri::command]
    pub async fn delete_journal_entry(
        repo: State<'_, Repo>,
        character_id: uuid::Uuid,
        entry_id: String,
    ) -> Result<(), crate::error::AppError> {
        journal::delete_journal_entry(repo.inner().clone(), character_id, entry_id).await
    }

    #[tauri::command]
    pub async fn get_ai_settings(
        repo: State<'_, Repo>,
    ) -> Result<ai::AiSettingsDto, crate::error::AppError> {
        ai::get_ai_settings(repo.inner().clone()).await
    }

    #[tauri::command]
    pub async fn set_ai_settings(
        repo: State<'_, Repo>,
        input: ai::SetAiSettingsInput,
    ) -> Result<(), crate::error::AppError> {
        ai::set_ai_settings(repo.inner().clone(), input).await
    }

    #[tauri::command]
    pub fn set_ai_token(token: String) -> Result<(), crate::error::AppError> {
        ai::set_ai_token(token)
    }

    #[tauri::command]
    pub fn clear_ai_token() -> Result<(), crate::error::AppError> {
        ai::clear_ai_token()
    }

    #[tauri::command]
    pub async fn test_ai_connection(
        client: State<'_, Ai>,
        input: ai::TestAiRequest,
    ) -> Result<ai::TestAiResult, crate::error::AppError> {
        ai::test_ai_connection(client.inner().clone(), input).await
    }

    #[tauri::command]
    pub async fn ai_chat_completion(
        client: State<'_, Ai>,
        input: ai::AiChatCompletionRequest,
    ) -> Result<ai::AiChatCompletionResponse, crate::error::AppError> {
        ai::ai_chat_completion(client.inner().clone(), input).await
    }
}

#[cfg(not(test))]
pub use inner::*;
