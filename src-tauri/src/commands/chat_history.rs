use std::sync::Arc;

use chrono::Utc;

use crate::commands::push::{DEFAULT_BASE_URL, SETTING_BASE_URL_PUBLIC as SETTING_BASE_URL};
use crate::domain::chat_message::{ChatMessage, ChatSyncState, SyncStatusKind};
use crate::domain::target::TargetKind;
use crate::error::AppError;
use crate::kindroid::{
    KindroidClient, RewindMessagesRequest, SendMessageRequest, SuggestUserMessageRequest,
    ToggleMessagePinRequest,
};
use crate::security::secrets::{SecretStoreError, Secrets, API_TOKEN_KEY};
use crate::storage::Repository;

use super::sync_loop::escape_fts_query;
use super::sync_registry::{ActiveSync, SyncRegistry};

pub async fn list_chat_messages(
    repo: Arc<dyn Repository>,
    ai_id: String,
    kind: TargetKind,
    before_ts: Option<i64>,
    limit: u32,
    favourites_only: bool,
) -> Result<Vec<ChatMessage>, AppError> {
    Ok(repo
        .list_chat_messages(&ai_id, kind, before_ts, limit, favourites_only)
        .await?)
}

pub async fn search_chat(
    repo: Arc<dyn Repository>,
    ai_id: String,
    kind: TargetKind,
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
        .search_chat(&ai_id, kind, &escaped, limit, offset, favourites_only)
        .await?)
}

/// Toggle the local favourite flag and reconcile it with the server's
/// `isPinned` response. Returns the canonical post-toggle value.
pub async fn toggle_chat_message_favourite(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    ai_id: String,
    kind: TargetKind,
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
    let token = Secrets::get(API_TOKEN_KEY).map_err(map_secret_err)?;
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
                kind,
                message_id: trimmed_msg.to_string(),
            },
        )
        .await?;
    // Canonical write: store what the server reports, even if it flipped
    // in the opposite direction to what the user clicked (e.g. another
    // client toggled the same message in parallel).
    let canonical = repo
        .set_chat_message_favourite(trimmed_ai, kind, trimmed_msg, resp.is_pinned)
        .await?;
    Ok(canonical)
}

fn map_secret_err(e: SecretStoreError) -> AppError {
    AppError::from(e)
}

pub async fn chat_message_count(
    repo: Arc<dyn Repository>,
    ai_id: String,
    kind: TargetKind,
) -> Result<u64, AppError> {
    Ok(repo.chat_message_count(&ai_id, kind).await?)
}

pub async fn get_chat_sync_state(
    repo: Arc<dyn Repository>,
    registry: Arc<SyncRegistry>,
    ai_id: String,
    kind: TargetKind,
) -> Result<Option<ChatSyncState>, AppError> {
    let Some(mut state) = repo.get_chat_sync_state(&ai_id, kind).await? else {
        return Ok(None);
    };

    let current = registry.current().await;
    let state_is_active = state.is_syncing
        || matches!(
            state.status_kind,
            SyncStatusKind::Running | SyncStatusKind::Backoff
        );
    if current.as_ref()
        != Some(&ActiveSync {
            ai_id: ai_id.clone(),
            kind,
        })
        && state_is_active
    {
        state.is_syncing = false;
        state.status_kind = SyncStatusKind::Idle;
        state.backoff_until = None;
        state.status_message = None;
        repo.upsert_chat_sync_state(&state).await?;
    }

    Ok(Some(state))
}

pub async fn get_current_sync(registry: Arc<SyncRegistry>) -> Result<Option<ActiveSync>, AppError> {
    Ok(registry.current().await)
}

pub async fn cancel_chat_sync(registry: Arc<SyncRegistry>) -> Result<(), AppError> {
    let _ = registry.cancel().await;
    Ok(())
}

/// Wipe all locally-cached chat history and sync state for `(ai_id, kind)`.
/// The next sync will start from scratch.
pub async fn reset_chat_history(
    repo: Arc<dyn Repository>,
    ai_id: String,
    kind: TargetKind,
) -> Result<usize, AppError> {
    let trimmed = ai_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    Ok(repo.reset_chat_history(trimmed, kind).await?)
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SendChatMessageInput {
    pub ai_id: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RewindChatInput {
    pub ai_id: String,
    pub count: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SuggestChatUserMessageInput {
    pub ai_id: String,
    pub existing_message: String,
}

/// Insert the user bubble, POST `/send-message`, insert the AI bubble.
/// Both local rows use a synthetic `local:<uuid>` `kindroid_msg_id`; the
/// next sync reconciles them with the server's real ids. The returned
/// `ChatMessage` is the inserted AI bubble so the UI can optimistically
/// append it.
pub async fn send_chat_message(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    input: SendChatMessageInput,
) -> Result<ChatMessage, AppError> {
    let trimmed_ai = input.ai_id.trim();
    if trimmed_ai.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    // Chat mode is single-AI only. A Group target reaches here only if
    // the frontend bypassed the Suggest/Send hide rule, but the backend
    // still rejects to defend against direct invoke.
    if repo
        .get_target_by_kind(trimmed_ai, TargetKind::Ai)
        .await?
        .is_none()
    {
        return Err(AppError::invalid(format!(
            "chat-mode is only available for AI targets, not groups or unknown ids (ai_id='{trimmed_ai}')"
        )));
    }
    let message = input.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::invalid("message is required"));
    }
    let token = Secrets::get(API_TOKEN_KEY).map_err(map_secret_err)?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    // Insert the user bubble with a synthetic id. The timestamp is
    // captured BEFORE the POST so the user bubble's timestamp is
    // strictly older than the AI bubble's, even when the request
    // completes within the same millisecond.
    let user_ts = Utc::now().timestamp_millis();
    let user_row = ChatMessage {
        id: uuid::Uuid::new_v4(),
        ai_id: trimmed_ai.to_string(),
        kind: TargetKind::Ai,
        kindroid_msg_id: format!("local:{}", uuid::Uuid::new_v4()),
        sender: "user".into(),
        display_name: None,
        timestamp: user_ts,
        message: message.clone(),
        image_urls: Vec::new(),
        image_description: None,
        video_description: None,
        internet_response: None,
        link_url: None,
        link_description: None,
        fetched_at: Utc::now(),
        favourite: false,
    };
    repo.upsert_chat_messages(trimmed_ai, TargetKind::Ai, std::slice::from_ref(&user_row))
        .await?;

    // POST the message. A failure here leaves the synthetic user bubble
    // in the DB so the user can resend; the error bubbles up to the
    // frontend as a toast.
    client
        .send_message(
            &token,
            &base_url,
            SendMessageRequest {
                ai_id: trimmed_ai.to_string(),
                message,
            },
        )
        .await?;

    // Insert the AI bubble a millisecond after the user row so the
    // DESC-by-timestamp ordering in the chat view keeps the AI bubble
    // at the bottom (newest).
    let ai_ts = user_ts + 1;
    let ai_row = ChatMessage {
        id: uuid::Uuid::new_v4(),
        ai_id: trimmed_ai.to_string(),
        kind: TargetKind::Ai,
        kindroid_msg_id: format!("local:{}", uuid::Uuid::new_v4()),
        sender: "ai".into(),
        display_name: None,
        timestamp: ai_ts,
        message: String::new(),
        image_urls: Vec::new(),
        image_description: None,
        video_description: None,
        internet_response: None,
        link_url: None,
        link_description: None,
        fetched_at: Utc::now(),
        favourite: false,
    };
    repo.upsert_chat_messages(trimmed_ai, TargetKind::Ai, std::slice::from_ref(&ai_row))
        .await?;
    Ok(ai_row)
}

/// Validate, capture the rows the server is about to delete, POST
/// `/rewind-messages`, then drop the local copies (including any
/// synthetic `local:` twins the server never knew about).
pub async fn rewind_chat(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    input: RewindChatInput,
) -> Result<usize, AppError> {
    let trimmed_ai = input.ai_id.trim();
    if trimmed_ai.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    if input.count == 0 {
        return Err(AppError::invalid("count must be greater than zero"));
    }
    // Kindroid's `/rewind-messages` operates on `(user, ai)` pairs and
    // rejects odd counts. The UI only offers even options; defend here.
    if input.count % 2 != 0 {
        return Err(AppError::invalid(
            "count must be even — Kindroid rewinds in user/AI pairs",
        ));
    }
    if repo
        .get_target_by_kind(trimmed_ai, TargetKind::Ai)
        .await?
        .is_none()
    {
        return Err(AppError::invalid(format!(
            "chat-mode is only available for AI targets, not groups or unknown ids (ai_id='{trimmed_ai}')"
        )));
    }

    // Capture the rows to delete BEFORE the POST so the user sees the
    // bubbles disappear even if the API call races with the next sync.
    let captured = repo
        .last_n_chat_messages(trimmed_ai, TargetKind::Ai, input.count)
        .await?;
    let token = Secrets::get(API_TOKEN_KEY).map_err(map_secret_err)?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    client
        .rewind_messages(
            &token,
            &base_url,
            RewindMessagesRequest {
                ai_id: trimmed_ai.to_string(),
                count: input.count,
            },
        )
        .await?;

    let mut deleted = 0usize;
    for m in &captured {
        // Delete by content fingerprint so any synthetic twin (the
        // server may have already reconciled one of the pair but not
        // the other) also goes away.
        deleted += repo
            .delete_chat_messages_by_content(
                trimmed_ai,
                TargetKind::Ai,
                &m.sender,
                m.timestamp,
                &m.message,
            )
            .await?;
    }
    Ok(deleted)
}

/// Request a suggested user-message seed from Kindroid and return the
/// plain-text body. Empty `existing_message` is allowed — the server
/// uses it as the optional seed.
pub async fn suggest_user_message(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    input: SuggestChatUserMessageInput,
) -> Result<String, AppError> {
    let trimmed_ai = input.ai_id.trim();
    if trimmed_ai.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    if repo
        .get_target_by_kind(trimmed_ai, TargetKind::Ai)
        .await?
        .is_none()
    {
        return Err(AppError::invalid(format!(
            "chat-mode is only available for AI targets, not groups or unknown ids (ai_id='{trimmed_ai}')"
        )));
    }
    let token = Secrets::get(API_TOKEN_KEY).map_err(map_secret_err)?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let resp = client
        .suggest_user_message(
            &token,
            &base_url,
            SuggestUserMessageRequest {
                ai_id: trimmed_ai.to_string(),
                existing_message: input.existing_message,
                stream: false,
            },
        )
        .await?;
    Ok(resp.body.trim().to_string())
}

/// Validate inputs and spawn the background sync loop. The actual loop
/// lives in `super::sync_loop_impl::run_sync_loop`; this function only does
/// pre-flight validation and starts the tokio task.
#[cfg(not(test))]
pub async fn start_chat_sync(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    ai_client: Arc<dyn crate::kindroid::ai::AiClient>,
    registry: Arc<SyncRegistry>,
    ai_id: String,
    kind: TargetKind,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    let trimmed = ai_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    // Ensure the target exists for this kind. Group targets cannot be
    // synced here either — the frontend never offers the Sync button on
    // them, but the backend still rejects to defend against direct
    // Tauri invokes.
    if repo.get_target_by_kind(trimmed, kind).await?.is_none() {
        return Err(AppError::invalid(format!(
            "target with id '{trimmed}' (kind={kind:?}) not found"
        )));
    }
    // Ensure a token is configured.
    if !crate::security::secrets::Secrets::exists(API_TOKEN_KEY) {
        return Err(AppError::TokenMissing);
    }

    let handle = match registry.start(trimmed, kind).await {
        Ok(h) => h,
        Err(current) => {
            return Err(AppError::SyncConflict {
                ai_id: current.ai_id,
                target_kind: current.kind,
            });
        }
    };

    let repo_c = repo.clone();
    let client_c = client.clone();
    let ai_client_c = ai_client.clone();
    let reg_c = registry.clone();
    let ai = trimmed.to_string();
    tauri::async_runtime::spawn(async move {
        super::sync_loop_impl::run_sync_loop(
            repo_c,
            client_c,
            ai_client_c,
            reg_c,
            ai,
            kind,
            handle.cancel_rx,
            app,
        )
        .await;
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat_message::{ChatSyncState, SyncStatusKind};
    use crate::domain::target::Target;
    use crate::storage::sqlite::SqliteRepository;
    use chrono::Utc;
    use uuid::Uuid;

    async fn seed_target(repo: &Arc<dyn Repository>, ai_id: &str) -> Uuid {
        let t = Target {
            id: Uuid::new_v4(),
            ai_id: ai_id.into(),
            kind: TargetKind::Ai,
            label: "test".into(),
            created_at: Utc::now(),
        };
        repo.upsert_target(t.clone()).await.unwrap();
        t.id
    }

    #[tokio::test]
    async fn get_chat_sync_state_resets_stale_running_flag() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let _ = seed_target(&repo, "ai_stale").await;
        let registry = Arc::new(SyncRegistry::new());
        repo.upsert_chat_sync_state(&ChatSyncState {
            ai_id: "ai_stale".into(),
            kind: TargetKind::Ai,
            last_synced_at: Utc::now(),
            last_timestamp: 0,
            full_sync_done: false,
            is_syncing: true,
            status_kind: SyncStatusKind::Running,
            status_message: Some("from a previous run".into()),
            backoff_until: Some(Utc::now()),
            total: 0,
        })
        .await
        .unwrap();

        let got = super::get_chat_sync_state(
            repo.clone(),
            registry.clone(),
            "ai_stale".into(),
            TargetKind::Ai,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!got.is_syncing);
        assert_eq!(got.status_kind, SyncStatusKind::Idle);
        assert!(got.status_message.is_none());
        assert!(got.backoff_until.is_none());

        let persisted = repo
            .get_chat_sync_state("ai_stale", TargetKind::Ai)
            .await
            .unwrap()
            .unwrap();
        assert!(!persisted.is_syncing);
        assert_eq!(persisted.status_kind, SyncStatusKind::Idle);
    }

    #[tokio::test]
    async fn get_chat_sync_state_preserves_active_running_flag() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let _ = seed_target(&repo, "ai_active").await;
        let registry = Arc::new(SyncRegistry::new());
        repo.upsert_chat_sync_state(&ChatSyncState {
            ai_id: "ai_active".into(),
            kind: TargetKind::Ai,
            last_synced_at: Utc::now(),
            last_timestamp: 0,
            full_sync_done: false,
            is_syncing: true,
            status_kind: SyncStatusKind::Running,
            status_message: None,
            backoff_until: None,
            total: 0,
        })
        .await
        .unwrap();

        let h = registry.start("ai_active", TargetKind::Ai).await.unwrap();
        let got = super::get_chat_sync_state(
            repo.clone(),
            registry.clone(),
            "ai_active".into(),
            TargetKind::Ai,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(got.is_syncing);
        assert_eq!(got.status_kind, SyncStatusKind::Running);
        drop(h);
        registry.release().await;
    }

    fn set_test_token() {
        crate::security::secrets::Secrets::set(
            crate::security::secrets::API_TOKEN_KEY,
            "test-token",
        )
        .expect("test token write");
    }

    #[tokio::test]
    async fn send_chat_message_rejects_empty_after_trim() {
        set_test_token();
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        seed_target(&repo, "ai_x").await;
        let client: Arc<dyn crate::kindroid::KindroidClient> =
            Arc::new(crate::kindroid::http::HttpKindroidClient::new());
        let err = super::send_chat_message(
            repo.clone(),
            client,
            SendChatMessageInput {
                ai_id: "ai_x".into(),
                message: "   ".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[tokio::test]
    async fn send_chat_message_rejects_group_target() {
        set_test_token();
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        // No AI target exists for "gc_1" — must fail the AI guard.
        let client: Arc<dyn crate::kindroid::KindroidClient> =
            Arc::new(crate::kindroid::http::HttpKindroidClient::new());
        let err = super::send_chat_message(
            repo.clone(),
            client,
            SendChatMessageInput {
                ai_id: "gc_1".into(),
                message: "hi".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[tokio::test]
    async fn rewind_chat_rejects_odd_count() {
        set_test_token();
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        seed_target(&repo, "ai_x").await;
        let client: Arc<dyn crate::kindroid::KindroidClient> =
            Arc::new(crate::kindroid::http::HttpKindroidClient::new());
        let err = super::rewind_chat(
            repo.clone(),
            client,
            RewindChatInput {
                ai_id: "ai_x".into(),
                count: 3,
            },
        )
        .await
        .unwrap_err();
        match err {
            AppError::Invalid { message } => {
                assert!(message.contains("even"), "msg: {message}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
