use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::kindroid::ai::{
    AiClient, AiError, AiMessage, ChatCompletionRequest, ChatCompletionResponse, ResponseFormat,
};
use crate::security::secrets::{SecretStoreError, Secrets, AI_TOKEN_KEY};
use crate::storage::Repository;

pub const SETTING_AI_BASE_URL: &str = "ai_base_url";
pub const SETTING_AI_MODEL: &str = "ai_model";
pub const DEFAULT_AI_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Serialize)]
pub struct AiSettingsDto {
    pub base_url: String,
    pub model: String,
    pub token_configured: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetAiSettingsInput {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct TestAiRequest {
    pub base_url: String,
    pub model: String,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestAiResult {
    pub ok: bool,
    pub status: u16,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct AiChatCompletionRequest {
    pub base_url: String,
    pub model: String,
    pub system: Option<String>,
    pub user: String,
    pub json_mode: bool,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AiChatCompletionResponse {
    pub content: String,
    pub model: Option<String>,
}

pub async fn get_ai_settings(
    repo: std::sync::Arc<dyn Repository>,
) -> Result<AiSettingsDto, AppError> {
    let base_url = repo
        .get_setting(SETTING_AI_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_AI_BASE_URL.to_string());
    let model = repo
        .get_setting(SETTING_AI_MODEL)
        .await?
        .unwrap_or_default();
    Ok(AiSettingsDto {
        base_url,
        model,
        token_configured: Secrets::exists(AI_TOKEN_KEY),
    })
}

pub async fn set_ai_settings(
    repo: std::sync::Arc<dyn Repository>,
    input: SetAiSettingsInput,
) -> Result<(), AppError> {
    let trimmed_url = input.base_url.trim();
    if !trimmed_url.starts_with("http://") && !trimmed_url.starts_with("https://") {
        return Err(AppError::invalid(
            "base_url must start with http:// or https://",
        ));
    }
    repo.set_setting(SETTING_AI_BASE_URL, trimmed_url).await?;
    repo.set_setting(SETTING_AI_MODEL, input.model.trim())
        .await?;
    Ok(())
}

pub fn set_ai_token(token: String) -> Result<(), AppError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("token is required"));
    }
    Secrets::set(AI_TOKEN_KEY, trimmed).map_err(map_secret_err)?;
    Ok(())
}

pub fn clear_ai_token() -> Result<(), AppError> {
    Secrets::clear(AI_TOKEN_KEY).map_err(map_secret_err)?;
    Ok(())
}

pub async fn test_ai_connection(
    client: std::sync::Arc<dyn AiClient>,
    req: TestAiRequest,
) -> Result<TestAiResult, AppError> {
    test_ai_connection_inner(client, req, keychain_token).await
}

pub async fn ai_chat_completion(
    client: std::sync::Arc<dyn AiClient>,
    req: AiChatCompletionRequest,
) -> Result<AiChatCompletionResponse, AppError> {
    ai_chat_completion_inner(client, req, keychain_token).await
}

async fn test_ai_connection_inner<F>(
    client: std::sync::Arc<dyn AiClient>,
    req: TestAiRequest,
    keychain_lookup: F,
) -> Result<TestAiResult, AppError>
where
    F: FnOnce() -> Result<Option<String>, AppError>,
{
    let bearer = resolve_bearer_for_test(req.bearer_token.as_deref(), keychain_lookup)?;
    let model = if req.model.trim().is_empty() {
        None
    } else {
        Some(req.model.trim().to_string())
    };
    let result = client
        .chat_completion(
            &req.base_url,
            bearer.as_deref(),
            ChatCompletionRequest {
                model,
                messages: vec![AiMessage {
                    role: "user".into(),
                    content: "ping".into(),
                }],
                response_format: None,
                stream: false,
            },
        )
        .await;
    Ok(map_test_result(result, &req.base_url))
}

async fn ai_chat_completion_inner<F>(
    client: std::sync::Arc<dyn AiClient>,
    req: AiChatCompletionRequest,
    keychain_lookup: F,
) -> Result<AiChatCompletionResponse, AppError>
where
    F: FnOnce() -> Result<Option<String>, AppError>,
{
    let bearer = resolve_bearer_for_chat(req.bearer_token.as_deref(), keychain_lookup)?;
    let model = if req.model.trim().is_empty() {
        None
    } else {
        Some(req.model.trim().to_string())
    };
    let mut messages: Vec<AiMessage> = Vec::new();
    let trimmed_system = req
        .system
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if req.json_mode {
        match trimmed_system {
            Some(s) => messages.push(AiMessage {
                role: "system".into(),
                content: format!("{s}\n\nRespond with JSON."),
            }),
            None => messages.push(AiMessage {
                role: "system".into(),
                content: "Respond with JSON.".into(),
            }),
        }
    } else if let Some(s) = trimmed_system {
        messages.push(AiMessage {
            role: "system".into(),
            content: s.to_string(),
        });
    }
    messages.push(AiMessage {
        role: "user".into(),
        content: req.user,
    });
    let resp = client
        .chat_completion(
            &req.base_url,
            bearer.as_deref(),
            ChatCompletionRequest {
                model,
                messages,
                response_format: if req.json_mode {
                    Some(ResponseFormat {
                        r#type: "json_object".into(),
                    })
                } else {
                    None
                },
                stream: false,
            },
        )
        .await?;
    Ok(AiChatCompletionResponse {
        content: resp.content,
        model: resp.model,
    })
}

/// Resolve the bearer token for `test_ai_connection`.
///
/// - `Some(non-empty)` → use it (trimmed).
/// - `Some("")` (or whitespace) → explicit "no auth header".
/// - `None` → fall through to the keychain. Missing keychain ⇒ no auth
///   header (auth-less servers can be probed freely).
fn resolve_bearer_for_test<F>(
    inline: Option<&str>,
    keychain_lookup: F,
) -> Result<Option<String>, AppError>
where
    F: FnOnce() -> Result<Option<String>, AppError>,
{
    if let Some(s) = inline {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
        return Ok(None);
    }
    keychain_lookup()
}

/// Resolve the bearer token for `ai_chat_completion`.
///
/// - `Some(non-empty)` → use it (trimmed).
/// - `Some("")` (or whitespace) → explicit "no auth header" (allowed).
/// - `None` → fall through to the keychain. Missing keychain ⇒
///   `AppError::TokenMissing` — the primitive refuses to fire without an
///   explicit user instruction.
fn resolve_bearer_for_chat<F>(
    inline: Option<&str>,
    keychain_lookup: F,
) -> Result<Option<String>, AppError>
where
    F: FnOnce() -> Result<Option<String>, AppError>,
{
    if let Some(s) = inline {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
        return Ok(None);
    }
    match keychain_lookup()? {
        Some(t) => Ok(Some(t)),
        None => Err(AppError::TokenMissing),
    }
}

/// Production keychain lookup. Returns `Ok(Some(token))` if a token is
/// stored, `Ok(None)` if the entry is missing, or `Err(AppError::Secret)`
/// for any other SecretStoreError (including Unavailable).
fn keychain_token() -> Result<Option<String>, AppError> {
    match Secrets::get(AI_TOKEN_KEY) {
        Ok(t) => Ok(Some(t)),
        Err(SecretStoreError::NotFound) => Ok(None),
        Err(e) => Err(AppError::from(e)),
    }
}

fn map_test_result(
    result: Result<ChatCompletionResponse, AiError>,
    base_url: &str,
) -> TestAiResult {
    match result {
        Ok(resp) => {
            let mut msg = format!("OK — reached {base_url}");
            if let Some(m) = resp.model {
                msg.push_str(&format!(" (model {m})"));
            }
            TestAiResult {
                ok: true,
                status: 200,
                message: msg,
            }
        }
        Err(AiError::Auth { status, .. }) => TestAiResult {
            ok: false,
            status,
            message: "Invalid or missing API key".into(),
        },
        Err(AiError::RateLimited {
            status,
            retry_after,
            ..
        }) => {
            let extra = retry_after
                .map(|d| format!(", retry in {}s", d.as_secs()))
                .unwrap_or_default();
            TestAiResult {
                ok: false,
                status,
                message: format!("Rate limited (status {status}{extra})"),
            }
        }
        Err(AiError::Network { message: msg }) => TestAiResult {
            ok: false,
            status: 0,
            message: format!("(network) {msg}"),
        },
        Err(other) => TestAiResult {
            ok: false,
            status: other_status(&other),
            message: format!("Server error: {other}"),
        },
    }
}

fn other_status(err: &AiError) -> u16 {
    match err {
        AiError::Auth { status, .. }
        | AiError::BadRequest { status, .. }
        | AiError::RateLimited { status, .. }
        | AiError::Server { status, .. } => *status,
        AiError::Network { message: _ } | AiError::Decode { message: _ } => 0,
    }
}

fn map_secret_err(e: SecretStoreError) -> AppError {
    AppError::from(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[derive(Default, Clone)]
    struct CapturedRequest {
        model: Option<String>,
        messages: Vec<AiMessage>,
        response_format: Option<ResponseFormat>,
        stream: bool,
        bearer_token: Option<String>,
    }

    struct FakeAiClient {
        last: Mutex<Option<CapturedRequest>>,
    }

    impl FakeAiClient {
        fn new() -> Self {
            Self {
                last: Mutex::new(None),
            }
        }
        fn capture(&self, token: Option<&str>, req: &ChatCompletionRequest) {
            *self.last.lock().unwrap() = Some(CapturedRequest {
                model: req.model.clone(),
                messages: req.messages.clone(),
                response_format: req.response_format.clone(),
                stream: req.stream,
                bearer_token: token.map(str::to_string),
            });
        }
    }

    #[async_trait]
    impl AiClient for FakeAiClient {
        async fn chat_completion(
            &self,
            _base_url: &str,
            bearer_token: Option<&str>,
            req: ChatCompletionRequest,
        ) -> Result<ChatCompletionResponse, AiError> {
            self.capture(bearer_token, &req);
            Ok(ChatCompletionResponse {
                content: "ok".into(),
                model: req.model.clone(),
            })
        }
    }

    struct FakeRepo {
        settings: Mutex<std::collections::HashMap<String, String>>,
        token_in_keychain: Mutex<Option<String>>,
    }

    impl FakeRepo {
        fn new() -> Self {
            Self {
                settings: Mutex::new(std::collections::HashMap::new()),
                token_in_keychain: Mutex::new(None),
            }
        }
        fn set_token(&self, token: &str) {
            *self.token_in_keychain.lock().unwrap() = Some(token.to_string());
        }
        fn clear_token(&self) {
            *self.token_in_keychain.lock().unwrap() = None;
        }
        fn token_in_keychain(&self) -> Option<String> {
            self.token_in_keychain.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Repository for FakeRepo {
        async fn list_characters(
            &self,
        ) -> Result<Vec<crate::domain::character::Character>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn get_character(
            &self,
            _id: Uuid,
        ) -> Result<crate::domain::character::Character, crate::storage::StorageError> {
            Err(crate::storage::StorageError::NotFound)
        }
        async fn upsert_character(
            &self,
            c: crate::domain::character::Character,
        ) -> Result<crate::domain::character::Character, crate::storage::StorageError> {
            Ok(c)
        }
        async fn delete_character(&self, _id: Uuid) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn list_targets(
            &self,
        ) -> Result<Vec<crate::domain::target::Target>, crate::storage::StorageError> {
            Ok(Vec::new())
        }
        async fn get_target(
            &self,
            _id: Uuid,
        ) -> Result<crate::domain::target::Target, crate::storage::StorageError> {
            Err(crate::storage::StorageError::NotFound)
        }
        async fn upsert_target(
            &self,
            t: crate::domain::target::Target,
        ) -> Result<crate::domain::target::Target, crate::storage::StorageError> {
            Ok(t)
        }
        async fn delete_target(&self, _id: Uuid) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn append_push_log(
            &self,
            e: crate::domain::push_log::PushLogEntry,
        ) -> Result<crate::domain::push_log::PushLogEntry, crate::storage::StorageError> {
            Ok(e)
        }
        async fn list_push_history(
            &self,
            _l: u32,
            _o: u32,
        ) -> Result<Vec<crate::domain::push_log::PushLogEntry>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn get_push_log(
            &self,
            _id: Uuid,
        ) -> Result<crate::domain::push_log::PushLogEntry, crate::storage::StorageError> {
            Err(crate::storage::StorageError::NotFound)
        }
        async fn get_setting(
            &self,
            k: &str,
        ) -> Result<Option<String>, crate::storage::StorageError> {
            Ok(self.settings.lock().unwrap().get(k).cloned())
        }
        async fn set_setting(&self, k: &str, v: &str) -> Result<(), crate::storage::StorageError> {
            self.settings
                .lock()
                .unwrap()
                .insert(k.to_string(), v.to_string());
            Ok(())
        }
        async fn save_character_image_bytes(
            &self,
            _id: Uuid,
            _b: &[u8],
        ) -> Result<String, crate::storage::StorageError> {
            Ok(String::new())
        }
        async fn read_character_image_bytes(
            &self,
            _id: Uuid,
        ) -> Result<Option<Vec<u8>>, crate::storage::StorageError> {
            Ok(None)
        }
        async fn delete_character_image_bytes(
            &self,
            _id: Uuid,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn upsert_chat_messages(
            &self,
            _ai_id: &str,
            _msgs: &[crate::domain::chat_message::ChatMessage],
        ) -> Result<usize, crate::storage::StorageError> {
            Ok(0)
        }
        async fn list_chat_messages(
            &self,
            _ai_id: &str,
            _before_ts: Option<i64>,
            _limit: u32,
            _favourites_only: bool,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn search_chat(
            &self,
            _ai_id: &str,
            _q: &str,
            _l: u32,
            _o: u32,
            _favourites_only: bool,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn set_chat_message_favourite(
            &self,
            _ai_id: &str,
            _kindroid_msg_id: &str,
            _favourite: bool,
        ) -> Result<bool, crate::storage::StorageError> {
            Ok(false)
        }
        async fn chat_message_count(
            &self,
            _ai_id: &str,
        ) -> Result<u64, crate::storage::StorageError> {
            Ok(0)
        }
        async fn get_chat_sync_state(
            &self,
            _ai_id: &str,
        ) -> Result<Option<crate::domain::chat_message::ChatSyncState>, crate::storage::StorageError>
        {
            Ok(None)
        }
        async fn upsert_chat_sync_state(
            &self,
            _state: &crate::domain::chat_message::ChatSyncState,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn reset_chat_history(
            &self,
            _ai_id: &str,
        ) -> Result<usize, crate::storage::StorageError> {
            Ok(0)
        }
        async fn delete_missing_chat_messages(
            &self,
            _ai_id: &str,
            _start_after: i64,
            _last_timestamp_inclusive: i64,
            _keep_ids: &[&str],
        ) -> Result<usize, crate::storage::StorageError> {
            Ok(0)
        }

        async fn list_stable_chat_messages(
            &self,
            _: &str,
            _: Option<&crate::domain::chat_automation::StableMessageCursor>,
            _: u32,
            _: u32,
        ) -> Result<Vec<crate::domain::chat_message::ChatMessage>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn latest_stable_cursor(
            &self,
            _: &str,
            _: u32,
        ) -> Result<
            Option<crate::domain::chat_automation::StableMessageCursor>,
            crate::storage::StorageError,
        > {
            Ok(None)
        }
        async fn get_chat_automation_state(
            &self,
            _: &str,
        ) -> Result<crate::domain::chat_automation::ChatAutomationState, crate::storage::StorageError>
        {
            Err(crate::storage::StorageError::NotFound)
        }
        async fn upsert_chat_automation_state(
            &self,
            _: &crate::domain::chat_automation::ChatAutomationState,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn create_auto_journal_run(
            &self,
            _: &crate::domain::chat_automation::AutoJournalRun,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn get_auto_journal_run(
            &self,
            _: &str,
        ) -> Result<crate::domain::chat_automation::AutoJournalRun, crate::storage::StorageError>
        {
            Err(crate::storage::StorageError::NotFound)
        }
        async fn list_pending_auto_journal_runs(
            &self,
            _: &str,
        ) -> Result<Vec<crate::domain::chat_automation::AutoJournalRun>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn update_auto_journal_run(
            &self,
            _: &crate::domain::chat_automation::AutoJournalRun,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn delete_auto_journal_run(
            &self,
            _: &str,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn create_auto_journal_entry(
            &self,
            _: &crate::domain::chat_automation::AutoJournalEntry,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn list_auto_journal_entries(
            &self,
            _: &str,
        ) -> Result<
            Vec<crate::domain::chat_automation::AutoJournalEntry>,
            crate::storage::StorageError,
        > {
            Ok(Vec::new())
        }
        async fn update_auto_journal_entry(
            &self,
            _: &crate::domain::chat_automation::AutoJournalEntry,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn commit_summary_candidate(
            &self,
            _: &str,
            _: &crate::domain::chat_automation::SummaryCandidate,
            _: Option<&crate::domain::chat_automation::StableMessageCursor>,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn clear_summary_candidate(
            &self,
            _: &str,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn reset_chat_summary(&self, _: &str) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn list_recent_successful_auto_journal_entries(
            &self,
            _: &str,
            _: u32,
        ) -> Result<
            Vec<crate::domain::chat_automation::AutoJournalEntry>,
            crate::storage::StorageError,
        > {
            Ok(Vec::new())
        }
        async fn list_journal_entries(
            &self,
            _cid: Uuid,
        ) -> Result<Vec<crate::domain::journal_entry::JournalEntry>, crate::storage::StorageError>
        {
            Ok(Vec::new())
        }
        async fn upsert_journal_entry(
            &self,
            _entry: &crate::domain::journal_entry::JournalEntry,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
        async fn delete_journal_entry(
            &self,
            _cid: Uuid,
            _id: &str,
        ) -> Result<(), crate::storage::StorageError> {
            Ok(())
        }
    }

    fn repo_keychain(repo: &FakeRepo) -> Result<Option<String>, AppError> {
        Ok(repo.token_in_keychain())
    }

    fn last(client: &FakeAiClient) -> CapturedRequest {
        client
            .last
            .lock()
            .unwrap()
            .clone()
            .expect("no request captured")
    }

    fn call_chat<F>(
        client: Arc<FakeAiClient>,
        _repo: Arc<FakeRepo>,
        req: AiChatCompletionRequest,
        keychain_lookup: F,
    ) -> Result<AiChatCompletionResponse, AppError>
    where
        F: FnOnce() -> Result<Option<String>, AppError>,
    {
        let keychain_lookup = move || keychain_lookup();
        tokio_test_block_on(ai_chat_completion_inner(client, req, keychain_lookup))
    }

    fn tokio_test_block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    fn call_test<F>(
        client: Arc<FakeAiClient>,
        req: TestAiRequest,
        keychain_lookup: F,
    ) -> Result<TestAiResult, AppError>
    where
        F: FnOnce() -> Result<Option<String>, AppError>,
    {
        let keychain_lookup = move || keychain_lookup();
        tokio_test_block_on(test_ai_connection_inner(client, req, keychain_lookup))
    }

    #[test]
    fn json_mode_false_with_some_system_sends_one_system_message() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: Some("  be helpful  ".into()),
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.messages.len(), 2);
        assert_eq!(cap.messages[0].role, "system");
        assert_eq!(cap.messages[0].content, "be helpful");
        assert_eq!(cap.messages[1].role, "user");
        assert!(cap.response_format.is_none());
    }

    #[test]
    fn json_mode_false_with_none_system_sends_no_system_message() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: None,
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.messages.len(), 1);
        assert_eq!(cap.messages[0].role, "user");
    }

    #[test]
    fn json_mode_true_with_some_system_appends_respond_with_json() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: Some("you are strict".into()),
                user: "hi".into(),
                json_mode: true,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.messages.len(), 2);
        assert_eq!(
            cap.messages[0].content,
            "you are strict\n\nRespond with JSON."
        );
        assert!(cap.response_format.is_some());
    }

    #[test]
    fn json_mode_true_with_none_system_synthesizes_system_message() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: None,
                user: "hi".into(),
                json_mode: true,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.messages.len(), 2);
        assert_eq!(cap.messages[0].content, "Respond with JSON.");
        assert!(cap.response_format.is_some());
    }

    #[test]
    fn whitespace_only_system_treated_as_none() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: Some("   \n  ".into()),
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.messages.len(), 1);
        assert_eq!(cap.messages[0].role, "user");
    }

    #[test]
    fn inline_token_overrides_keychain() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        repo.set_token("k");
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: None,
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some("override".into()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.bearer_token.as_deref(), Some("override"));
    }

    #[test]
    fn inline_empty_token_skips_keychain_and_sends_no_auth() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        repo.set_token("k");
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: None,
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some("".into()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.bearer_token, None);
    }

    #[test]
    fn token_missing_returns_token_missing_error() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        // No inline, no keychain.
        let err = call_chat(
            client,
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "m".into(),
                system: None,
                user: "hi".into(),
                json_mode: false,
                bearer_token: None,
            },
            || repo_keychain(&repo),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::TokenMissing));
    }

    #[test]
    fn empty_model_omits_model_on_wire() {
        let client = Arc::new(FakeAiClient::new());
        let repo = Arc::new(FakeRepo::new());
        let _ = call_chat(
            client.clone(),
            repo.clone(),
            AiChatCompletionRequest {
                base_url: "http://x".into(),
                model: "".into(),
                system: None,
                user: "hi".into(),
                json_mode: false,
                bearer_token: Some(String::new()),
            },
            || repo_keychain(&repo),
        )
        .unwrap();
        let cap = last(&client);
        assert_eq!(cap.model, None);
    }

    #[test]
    fn test_ai_connection_returns_ok_on_200() {
        struct OkClient;
        #[async_trait]
        impl AiClient for OkClient {
            async fn chat_completion(
                &self,
                _b: &str,
                _t: Option<&str>,
                _r: ChatCompletionRequest,
            ) -> Result<ChatCompletionResponse, AiError> {
                Ok(ChatCompletionResponse {
                    content: "pong".into(),
                    model: Some("gpt-4o-mini".into()),
                })
            }
        }
        let r = tokio_test_block_on(test_ai_connection(
            Arc::new(OkClient),
            TestAiRequest {
                base_url: "http://x".into(),
                model: "gpt-4o-mini".into(),
                bearer_token: Some("t".into()),
            },
        ))
        .unwrap();
        assert!(r.ok);
        assert_eq!(r.status, 200);
        assert!(r.message.contains("gpt-4o-mini"));
    }

    #[test]
    fn test_ai_connection_maps_401() {
        struct AuthClient;
        #[async_trait]
        impl AiClient for AuthClient {
            async fn chat_completion(
                &self,
                _b: &str,
                _t: Option<&str>,
                _r: ChatCompletionRequest,
            ) -> Result<ChatCompletionResponse, AiError> {
                Err(AiError::Auth {
                    status: 401,
                    body: "nope".into(),
                })
            }
        }
        let r = tokio_test_block_on(test_ai_connection(
            Arc::new(AuthClient),
            TestAiRequest {
                base_url: "http://x".into(),
                model: "".into(),
                bearer_token: Some("t".into()),
            },
        ))
        .unwrap();
        assert!(!r.ok);
        assert_eq!(r.status, 401);
        assert_eq!(r.message, "Invalid or missing API key");
    }

    #[test]
    fn set_ai_settings_rejects_non_http_url() {
        let repo = Arc::new(FakeRepo::new());
        let err = tokio_test_block_on(set_ai_settings(
            repo,
            SetAiSettingsInput {
                base_url: "ftp://nope".into(),
                model: "m".into(),
            },
        ))
        .unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }

    #[test]
    fn set_ai_token_rejects_empty() {
        let err = set_ai_token("   ".into()).unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
    }
}
