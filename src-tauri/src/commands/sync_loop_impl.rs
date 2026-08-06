use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use crate::commands::push::{DEFAULT_BASE_URL, SETTING_BASE_URL};
use crate::commands::sync_loop::compute_local_rewind;
use crate::commands::sync_registry::SyncRegistry;
use crate::domain::chat_message::{ChatMessage, ChatSyncState, SyncStatusKind};
use crate::domain::target::TargetKind;
use crate::error::AppError;
use crate::kindroid::{ChatMessagesPage, KindroidClient, KindroidError, ListChatMessagesRequest};
use crate::security::secrets::{Secrets, API_TOKEN_KEY};
use crate::storage::Repository;

pub const SYNC_INTERVAL: Duration = Duration::from_secs(120);
const BACKOFF_CAP: Duration = Duration::from_secs(60 * 60);
const DEFAULT_BACKOFF: Duration = Duration::from_secs(60 * 60);
/// Safety cap per sync cycle: 200 pages × 100 msgs/page = 20 000 messages,
/// which matches the documented per-character maximum. After this we
/// force a sleep and resume on the next cycle so the loop can never
/// wedge against a misbehaving API.
const MAX_PAGES_PER_CYCLE: u64 = 200;

#[derive(Debug, Clone, Copy, Default)]
struct SyncLoopStats {
    requests: u64,
    last_batch_size: u64,
    last_deleted_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SyncProgress {
    pub ai_id: String,
    pub kind: TargetKind,
    pub total: u64,
    pub last_timestamp: i64,
    pub full_sync_done: bool,
    pub status_kind: String,
    pub status_message: Option<String>,
    /// Number of API requests made in this run so far. Lets the UI show
    /// a "Request #N" indicator during the initial backfill.
    pub requests: u64,
    /// Number of messages returned by the most recent API call (0 if the
    /// last call was empty). Useful for the progress hint.
    pub last_batch_size: u64,
    /// Whether the latest call returned a non-empty page (helps the UI
    /// distinguish "still fetching" from "caught up to the cursor").
    pub last_batch_had_messages: bool,
    /// Number of local messages removed by the latest poll because they
    /// were no longer present on the server (deletion detection).
    pub last_deleted_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SyncComplete {
    pub ai_id: String,
    pub kind: TargetKind,
    pub total: u64,
    pub status_kind: String,
    pub status_message: Option<String>,
    pub requests: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SleepResult {
    Slept,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrainOutcome {
    /// Inner loop drained: the API returned an empty page or `has_more = false`.
    Drained,
    /// Cancellation was requested.
    Cancelled,
    /// A non-recoverable error occurred.
    Error,
}

/// Run the long-running sync loop for a single `(ai_id, kind)`.
///
/// The loop continuously **drains** pages from the API (one call per page,
/// each page holds up to 100 messages) until the API returns an empty page
/// or `has_more = false`. Only then does it sleep `SYNC_INTERVAL` between
/// cycles. 429s are honoured inside the inner loop (retry the same page
/// after `Retry-After`); other errors stop the loop, preserving the cursor
/// so the user can resume by clicking Sync again.
#[allow(clippy::too_many_arguments)]
pub async fn run_sync_loop(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    ai_client: Arc<dyn crate::kindroid::ai::AiClient>,
    registry: Arc<SyncRegistry>,
    ai_id: String,
    kind: TargetKind,
    cancel_rx: watch::Receiver<bool>,
    app: AppHandle,
) {
    if let Err(e) = run_loop_inner(
        repo.clone(),
        client.clone(),
        ai_client.clone(),
        ai_id.clone(),
        kind,
        cancel_rx,
        app.clone(),
    )
    .await
    {
        // Preserve the existing cursor + total + full_sync_done so the
        // user can resume by clicking Sync again. We only flip to
        // `Error` and reset `is_syncing`.
        let existing = repo.get_chat_sync_state(&ai_id, kind).await.ok().flatten();
        let last_timestamp = existing.as_ref().map(|s| s.last_timestamp).unwrap_or(0);
        let full_sync_done = existing.as_ref().map(|s| s.full_sync_done).unwrap_or(false);
        let total = existing.as_ref().map(|s| s.total).unwrap_or(0);
        let _ = repo
            .upsert_chat_sync_state(&ChatSyncState {
                ai_id: ai_id.clone(),
                kind,
                last_synced_at: Utc::now(),
                last_timestamp,
                full_sync_done,
                is_syncing: false,
                status_kind: SyncStatusKind::Error,
                status_message: Some(e.to_string()),
                backoff_until: None,
                total,
            })
            .await;
        let _ = app.emit(
            "chat-sync-complete",
            SyncComplete {
                ai_id: ai_id.clone(),
                kind,
                total,
                status_kind: "error".into(),
                status_message: Some(e.to_string()),
                requests: 0,
            },
        );
    }
    // Always release the slot so a future sync can take it.
    let _ = registry.release().await;
}

async fn run_loop_inner(
    repo: Arc<dyn Repository>,
    client: Arc<dyn KindroidClient>,
    ai_client: Arc<dyn crate::kindroid::ai::AiClient>,
    ai_id: String,
    kind: TargetKind,
    cancel_rx: watch::Receiver<bool>,
    app: AppHandle,
) -> Result<(), AppError> {
    let mut state = ensure_state(&repo, &ai_id, kind).await?;
    let mut stats = SyncLoopStats::default();
    state = mark_status(
        &repo,
        &app,
        &ai_id,
        kind,
        SyncStatusKind::Running,
        None,
        None,
        true,
        &state,
        stats,
    )
    .await;

    let mut cancel_rx = cancel_rx;

    loop {
        if *cancel_rx.borrow() {
            finalize_cancelled(&repo, &app, &ai_id, kind, &state, stats).await;
            return Ok(());
        }

        // Drain pages until the API signals we're caught up. The 2-minute
        // sleep happens after the drain, not between every page.
        match drain_pages(
            &repo,
            &client,
            &ai_id,
            kind,
            &mut state,
            &mut stats,
            &mut cancel_rx,
            &app,
        )
        .await?
        {
            DrainOutcome::Drained => {
                if state.full_sync_done {
                    super::chat_automation::run_automation_cycle(
                        repo.clone(),
                        client.clone(),
                        ai_client.clone(),
                        &ai_id,
                    )
                    .await;
                }
            }
            DrainOutcome::Cancelled => {
                finalize_cancelled(&repo, &app, &ai_id, kind, &state, stats).await;
                return Ok(());
            }
            DrainOutcome::Error => {
                // drain_pages already wrote the error state and emitted
                // the complete event.
                return Ok(());
            }
        }

        if sleep_or_cancel(&mut cancel_rx, SYNC_INTERVAL).await == SleepResult::Cancelled {
            finalize_cancelled(&repo, &app, &ai_id, kind, &state, stats).await;
            return Ok(());
        }
    }
}

/// Inner loop: keep paging the API until the page is empty, `has_more` is
/// false, or we hit the per-cycle safety cap. 429s are honoured in place
/// (sleep, then retry the same cursor). Non-429 errors are reported to
/// the caller. On every complete page (`has_more = false`) the loop
/// also sweeps the `(start_after, prev_cursor]` window for messages
/// that were deleted on the server and removes them locally — partial
/// pages (where we don't yet know whether the messages with `ts >
/// lastTimestamp` still exist) defer this to the final page of the
/// cycle.
#[allow(clippy::too_many_arguments)]
async fn drain_pages(
    repo: &Arc<dyn Repository>,
    client: &Arc<dyn KindroidClient>,
    ai_id: &str,
    kind: TargetKind,
    state: &mut ChatSyncState,
    stats: &mut SyncLoopStats,
    cancel_rx: &mut watch::Receiver<bool>,
    app: &AppHandle,
) -> Result<DrainOutcome, AppError> {
    let mut pages_this_cycle: u64 = 0;
    loop {
        if *cancel_rx.borrow() {
            return Ok(DrainOutcome::Cancelled);
        }
        if pages_this_cycle >= MAX_PAGES_PER_CYCLE {
            // Forced pause: defer the remaining pages to the next cycle.
            return Ok(DrainOutcome::Drained);
        }

        let prev_cursor = state.last_timestamp;
        // Compute the rewind from the local DB rather than the cursor
        // or wall-clock time. Kindroid only lets the user edit and
        // delete the last 10 messages, so we only ever need to re-check
        // those. We use the 12 most recent local messages as the
        // boundary (`REWIND_MESSAGE_COUNT` with a +2 margin over the
        // 10-position edit window to handle the strict-`>` API
        // boundary plus one position just outside the window). If we
        // have fewer than 12 messages locally, we rewind to the oldest
        // so every locally-known message is re-checked.
        let start_after =
            compute_local_rewind(&**repo, ai_id, kind, state.full_sync_done, prev_cursor).await?;

        let token = match Secrets::get(API_TOKEN_KEY) {
            Ok(t) => t,
            Err(_) => {
                finalize_error(
                    repo,
                    app,
                    ai_id,
                    kind,
                    "API token cleared",
                    state.total,
                    *stats,
                )
                .await;
                return Ok(DrainOutcome::Error);
            }
        };
        let base_url = repo
            .get_setting(SETTING_BASE_URL)
            .await?
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let req = ListChatMessagesRequest {
            ai_id: ai_id.to_string(),
            kind,
            limit: 100,
            start_after_timestamp: start_after,
        };
        let page: ChatMessagesPage = match client.list_chat_messages(&token, &base_url, req).await {
            Ok(p) => p,
            Err(KindroidError::RateLimited { retry_after, .. }) => {
                let backoff = retry_after.unwrap_or(DEFAULT_BACKOFF).min(BACKOFF_CAP);
                let until = Utc::now() + chrono::Duration::from_std(backoff).unwrap_or_default();
                stats.requests += 1;
                *state = write_state(
                    repo,
                    app,
                    ai_id,
                    kind,
                    state,
                    SyncStatusKind::Backoff,
                    Some(format!(
                        "Rate-limited; retrying in {}",
                        format_countdown(until)
                    )),
                    Some(until),
                    *stats,
                )
                .await;
                if sleep_or_cancel(cancel_rx, backoff).await == SleepResult::Cancelled {
                    return Ok(DrainOutcome::Cancelled);
                }
                *state = write_state(
                    repo,
                    app,
                    ai_id,
                    kind,
                    state,
                    SyncStatusKind::Running,
                    None,
                    None,
                    *stats,
                )
                .await;
                // Re-fetch the same page.
                continue;
            }
            Err(e) => {
                finalize_error(repo, app, ai_id, kind, &e.to_string(), state.total, *stats).await;
                return Ok(DrainOutcome::Error);
            }
        };

        // Regardless of size, count the request and bump the safety cap.
        stats.requests += 1;
        pages_this_cycle += 1;

        if page.messages.is_empty() {
            // Empty page. We can't tell whether the API is just at the
            // end of history (`has_more = false`) or the response was
            // truncated, but the only safe call on an empty response is
            // to do nothing and let the next poll try again.
            stats.last_batch_size = 0;
            stats.last_deleted_count = 0;
            repo.upsert_chat_sync_state(state).await?;
            emit_progress(app, ai_id, kind, state, *stats);
            return Ok(DrainOutcome::Drained);
        }

        let incoming: Vec<ChatMessage> = page
            .messages
            .into_iter()
            .map(|m| {
                let text = m.message.clone().unwrap_or_default();
                ChatMessage {
                    id: uuid::Uuid::new_v4(),
                    ai_id: ai_id.to_string(),
                    kind,
                    fetched_at: Utc::now(),
                    message: text,
                    ..m.into()
                }
            })
            .collect();

        let touched = repo.upsert_chat_messages(ai_id, kind, &incoming).await?;
        // `touched` is "inserts + content-actually-changed updates",
        // thanks to the WHERE clause on the upsert. We don't add it to
        // `state.total` because the same row can be touched on later
        // polls; instead we recompute the unique message count from the
        // DB so the displayed total stays exact.
        state.total = repo.chat_message_count(ai_id, kind).await?;

        // Advance the cursor. Prefer the server's `pagination.lastTimestamp`
        // (the API's documented cursor); fall back to the max of the
        // inserted rows when the field is missing or zero. In either case
        // we only advance on a non-empty page so an empty page can never
        // rewind the cursor.
        let api_cursor = page.pagination_last_timestamp.filter(|ts| *ts > 0);
        let computed_cursor = incoming.iter().map(|m| m.timestamp).max();
        if let Some(next) = api_cursor.or(computed_cursor) {
            state.last_timestamp = next;
        }

        // Server-side deletion detection. Only do this on a complete
        // response (`has_more = false`): the API has returned every
        // message with `ts > start_after`, so local messages in
        // `(start_after, prev_cursor]` that aren't in the response have
        // been confirmed-deleted on the server. On a partial page we
        // defer — we don't yet know whether messages with `ts >
        // lastTimestamp` still exist, and over-deleting would be wrong.
        if !page.has_more && prev_cursor > 0 {
            if let Some(sa) = start_after {
                let keep_ids: Vec<&str> = incoming
                    .iter()
                    .map(|m| m.kindroid_msg_id.as_str())
                    .collect();
                let deleted = repo
                    .delete_missing_chat_messages(ai_id, kind, sa, prev_cursor, &keep_ids)
                    .await?;
                let _ = deleted;
                stats.last_deleted_count = deleted as u64;
                // The deletes may have changed the row count, so refresh
                // the cached total.
                state.total = repo.chat_message_count(ai_id, kind).await?;
            }
        } else {
            stats.last_deleted_count = 0;
        }

        state.last_synced_at = Utc::now();
        if !state.full_sync_done && !page.has_more {
            state.full_sync_done = true;
        }
        state.status_kind = SyncStatusKind::Running;
        state.status_message = None;
        state.backoff_until = None;
        stats.last_batch_size = incoming.len() as u64;
        let _ = touched; // surfaced via the per-poll progress bar in the UI
        repo.upsert_chat_sync_state(state).await?;
        emit_progress(app, ai_id, kind, state, *stats);

        if !page.has_more {
            // Server says we're done.
            return Ok(DrainOutcome::Drained);
        }
        // Loop again: fetch the next page.
    }
}

fn format_countdown(until: chrono::DateTime<Utc>) -> String {
    let diff = (until - Utc::now()).num_seconds().max(0);
    let m = diff / 60;
    let s = diff % 60;
    if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

async fn ensure_state(
    repo: &Arc<dyn Repository>,
    ai_id: &str,
    kind: TargetKind,
) -> Result<ChatSyncState, AppError> {
    if let Some(s) = repo.get_chat_sync_state(ai_id, kind).await? {
        return Ok(s);
    }
    let total = repo.chat_message_count(ai_id, kind).await?;
    let s = ChatSyncState {
        ai_id: ai_id.to_string(),
        kind,
        last_synced_at: Utc::now(),
        last_timestamp: 0,
        full_sync_done: total > 0,
        is_syncing: true,
        status_kind: SyncStatusKind::Running,
        status_message: None,
        backoff_until: None,
        total,
    };
    repo.upsert_chat_sync_state(&s).await?;
    Ok(s)
}

#[allow(clippy::too_many_arguments)]
async fn mark_status(
    repo: &Arc<dyn Repository>,
    app: &AppHandle,
    ai_id: &str,
    kind: TargetKind,
    sync_kind: SyncStatusKind,
    message: Option<String>,
    backoff_until: Option<chrono::DateTime<Utc>>,
    is_syncing: bool,
    state: &ChatSyncState,
    stats: SyncLoopStats,
) -> ChatSyncState {
    let s = ChatSyncState {
        ai_id: ai_id.to_string(),
        kind,
        last_synced_at: Utc::now(),
        last_timestamp: state.last_timestamp,
        full_sync_done: state.full_sync_done,
        is_syncing,
        status_kind: sync_kind,
        status_message: message,
        backoff_until,
        total: state.total,
    };
    let _ = repo.upsert_chat_sync_state(&s).await;
    emit_progress(app, ai_id, kind, &s, stats);
    s
}

#[allow(clippy::too_many_arguments)]
async fn write_state(
    repo: &Arc<dyn Repository>,
    app: &AppHandle,
    ai_id: &str,
    kind: TargetKind,
    prev: &ChatSyncState,
    sync_kind: SyncStatusKind,
    message: Option<String>,
    backoff_until: Option<chrono::DateTime<Utc>>,
    stats: SyncLoopStats,
) -> ChatSyncState {
    let mut s = prev.clone();
    s.status_kind = sync_kind;
    s.status_message = message;
    s.backoff_until = backoff_until;
    s.last_synced_at = Utc::now();
    let _ = repo.upsert_chat_sync_state(&s).await;
    emit_progress(app, ai_id, kind, &s, stats);
    s
}

async fn finalize_cancelled(
    repo: &Arc<dyn Repository>,
    app: &AppHandle,
    ai_id: &str,
    kind: TargetKind,
    prev: &ChatSyncState,
    stats: SyncLoopStats,
) {
    let mut s = prev.clone();
    s.is_syncing = false;
    s.status_kind = SyncStatusKind::Cancelled;
    s.status_message = None;
    s.backoff_until = None;
    s.last_synced_at = Utc::now();
    let _ = repo.upsert_chat_sync_state(&s).await;
    let _ = app.emit(
        "chat-sync-complete",
        SyncComplete {
            ai_id: ai_id.to_string(),
            kind,
            total: s.total,
            status_kind: s.status_kind.as_str().to_string(),
            status_message: s.status_message.clone(),
            requests: stats.requests,
        },
    );
}

async fn finalize_error(
    repo: &Arc<dyn Repository>,
    app: &AppHandle,
    ai_id: &str,
    kind: TargetKind,
    message: &str,
    total: u64,
    stats: SyncLoopStats,
) {
    // Preserve the cursor so the user can resume the backfill by
    // clicking Sync again. We only flip to Error status.
    let existing = repo.get_chat_sync_state(ai_id, kind).await.ok().flatten();
    let last_timestamp = existing.as_ref().map(|s| s.last_timestamp).unwrap_or(0);
    let full_sync_done = existing.as_ref().map(|s| s.full_sync_done).unwrap_or(false);
    let s = ChatSyncState {
        ai_id: ai_id.to_string(),
        kind,
        last_synced_at: Utc::now(),
        last_timestamp,
        full_sync_done,
        is_syncing: false,
        status_kind: SyncStatusKind::Error,
        status_message: Some(message.to_string()),
        backoff_until: None,
        total,
    };
    let _ = repo.upsert_chat_sync_state(&s).await;
    let _ = app.emit(
        "chat-sync-complete",
        SyncComplete {
            ai_id: ai_id.to_string(),
            kind,
            total: s.total,
            status_kind: s.status_kind.as_str().to_string(),
            status_message: s.status_message.clone(),
            requests: stats.requests,
        },
    );
}

fn emit_progress(
    app: &AppHandle,
    ai_id: &str,
    kind: TargetKind,
    state: &ChatSyncState,
    stats: SyncLoopStats,
) {
    let _ = app.emit(
        "chat-sync-progress",
        SyncProgress {
            ai_id: ai_id.to_string(),
            kind,
            total: state.total,
            last_timestamp: state.last_timestamp,
            full_sync_done: state.full_sync_done,
            status_kind: state.status_kind.as_str().to_string(),
            status_message: state.status_message.clone(),
            requests: stats.requests,
            last_batch_size: stats.last_batch_size,
            last_batch_had_messages: stats.last_batch_size > 0,
            last_deleted_count: stats.last_deleted_count,
        },
    );
}

async fn sleep_or_cancel(rx: &mut watch::Receiver<bool>, d: Duration) -> SleepResult {
    tokio::select! {
        _ = rx.changed() => SleepResult::Cancelled,
        _ = tokio::time::sleep(d) => SleepResult::Slept,
    }
}

impl From<crate::kindroid::RawChatMessage> for ChatMessage {
    fn from(m: crate::kindroid::RawChatMessage) -> Self {
        ChatMessage {
            id: uuid::Uuid::new_v4(),
            ai_id: String::new(),
            // Placeholder; the caller (`drain_pages`) overrides this with
            // the kind of the target whose chat history is being synced.
            kind: TargetKind::Ai,
            kindroid_msg_id: m.id,
            sender: m.sender,
            display_name: m.display_name,
            timestamp: m.timestamp,
            message: m.message.unwrap_or_default(),
            image_urls: m.image_urls.unwrap_or_default(),
            image_description: m.image_description,
            video_description: m.video_description,
            internet_response: m.internet_response,
            link_url: m.link_url,
            link_description: m.link_description,
            fetched_at: Utc::now(),
            // `get-chat-messages` does not return isPinned; the local
            // DB keeps user-set pin state via `set_chat_message_favourite`
            // and survives subsequent syncs because the upsert omits
            // `favourite` from both SET and WHERE.
            favourite: false,
        }
    }
}
