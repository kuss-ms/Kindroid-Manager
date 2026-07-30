use std::sync::Arc;

use crate::commands::push::{DEFAULT_BASE_URL, SETTING_BASE_URL_PUBLIC as SETTING_BASE_URL};
use crate::domain::chat_message::{ChatMessage, ChatSyncState};
use crate::error::AppError;
use crate::kindroid::{KindroidClient, ToggleMessagePinRequest};
use crate::security::secrets::{SecretStoreError, Secrets};
use crate::storage::Repository;

use super::sync_loop::escape_fts_query;
use super::sync_registry::SyncRegistry;

pub async fn list_chat_messages(
    repo: Arc<dyn Repository>,
    ai_id: String,
    before_ts: Option<i64>,
    limit: u32,
    favourites_only: bool,
) -> Result<Vec<ChatMessage>, AppError> {
    Ok(repo
        .list_chat_messages(&ai_id, before_ts, limit, favourites_only)
        .await?)
}

pub async fn search_chat(
    repo: Arc<dyn Repository>,
    ai_id: String,
    query: String,
    limit: u32,
    offset: u32,
    favourites_only: bool,
) -> Result<Vec<ChatMessage>, AppError> {
    let escaped = escape_fts_query(&query);
    if escaped.is_empty() {
        return Ok(Vec::new());
    }
    Ok(repo
        .search_chat(&ai_id, &escaped, limit, offset, favourites_only)
        .await?)
}

/// Toggle the local favourite flag and reconcile it with the server's
/// `isPinned` response. Returns the canonical post-toggle value.
pub async fn toggle_chat_message_favourite(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    ai_id: String,
    kindroid_msg_id: String,
) -> Result<bool, AppError> {
    let trimmed_ai = ai_id.trim();
    if trimmed_ai.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    let trimmed_msg = kindroid_msg_id.trim();
    if trimmed_msg.is_empty() {
        return Err(AppError::invalid("kindroid_msg_id is required"));
    }
    let token = Secrets::get().map_err(map_secret_err)?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let resp = client
        .toggle_message_pin(
            &token,
            &base_url,
            ToggleMessagePinRequest {
                ai_id: trimmed_ai.to_string(),
                message_id: trimmed_msg.to_string(),
            },
        )
        .await?;
    // Canonical write: store what the server reports, even if it flipped
    // in the opposite direction to what the user clicked (e.g. another
    // client toggled the same message in parallel).
    let canonical = repo
        .set_chat_message_favourite(trimmed_ai, trimmed_msg, resp.is_pinned)
        .await?;
    Ok(canonical)
}

fn map_secret_err(e: SecretStoreError) -> AppError {
    AppError::Secret(e)
}

pub async fn chat_message_count(repo: Arc<dyn Repository>, ai_id: String) -> Result<u64, AppError> {
    Ok(repo.chat_message_count(&ai_id).await?)
}

pub async fn get_chat_sync_state(
    repo: Arc<dyn Repository>,
    ai_id: String,
) -> Result<Option<ChatSyncState>, AppError> {
    Ok(repo.get_chat_sync_state(&ai_id).await?)
}

pub async fn get_current_sync(registry: Arc<SyncRegistry>) -> Result<Option<String>, AppError> {
    Ok(registry.current().await)
}

pub async fn cancel_chat_sync(registry: Arc<SyncRegistry>) -> Result<(), AppError> {
    let _ = registry.cancel().await;
    Ok(())
}

/// Wipe all locally-cached chat history and sync state for `ai_id`.
/// The next sync will start from scratch.
pub async fn reset_chat_history(
    repo: Arc<dyn Repository>,
    ai_id: String,
) -> Result<usize, AppError> {
    let trimmed = ai_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    Ok(repo.reset_chat_history(trimmed).await?)
}

/// Validate inputs and spawn the background sync loop. The actual loop
/// lives in `super::sync_loop_impl::run_sync_loop`; this function only does
/// pre-flight validation and starts the tokio task.
#[cfg(not(test))]
pub async fn start_chat_sync(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    registry: Arc<SyncRegistry>,
    ai_id: String,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    let trimmed = ai_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    // Ensure the target exists.
    let targets = repo.list_targets().await?;
    if !targets.iter().any(|t| t.ai_id == trimmed) {
        return Err(AppError::invalid(format!(
            "target with ai_id '{trimmed}' not found"
        )));
    }
    // Ensure a token is configured.
    if !crate::security::secrets::Secrets::exists() {
        return Err(AppError::TokenMissing);
    }

    let handle = match registry.start(trimmed).await {
        Ok(h) => h,
        Err(current) => {
            return Err(AppError::SyncConflict { ai_id: current });
        }
    };

    let repo_c = repo.clone();
    let client_c = client.clone();
    let reg_c = registry.clone();
    let ai = trimmed.to_string();
    tauri::async_runtime::spawn(async move {
        super::sync_loop_impl::run_sync_loop(repo_c, client_c, reg_c, ai, handle.cancel_rx, app)
            .await;
    });

    Ok(())
}
