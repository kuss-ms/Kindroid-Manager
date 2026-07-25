// Tauri command wrappers. The `tauri::State` import and the
// `#[tauri::command]` macro pull in the Tauri runtime DLLs
// (WebView2Loader.dll) at link time, so this module is only compiled
// for the real app build, not for `cargo test`. The `run()` function
// in `lib.rs` is also gated to non-test, so `tauri::generate_handler!`
// never tries to look up these items in test mode.

#[cfg(not(test))]
mod inner {
    use crate::commands::{characters, history, push, settings, share_code, targets};
    use tauri::State;

    type Repo = std::sync::Arc<dyn crate::storage::Repository>;
    type Client = std::sync::Arc<dyn crate::kindroid::KindroidClient>;

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
        id: uuid::Uuid,
    ) -> Result<Vec<u8>, crate::error::AppError> {
        share_code::export_share_image(repo.inner().clone(), id).await
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
    pub fn clear_token() -> Result<(), crate::error::AppError> {
        settings::clear_token()
    }

    #[tauri::command]
    pub async fn test_token(
        repo: State<'_, Repo>,
        client: State<'_, Client>,
    ) -> Result<settings::TestTokenResult, crate::error::AppError> {
        settings::test_token(repo.inner().clone(), client.inner().clone()).await
    }
}

#[cfg(not(test))]
pub use inner::*;
