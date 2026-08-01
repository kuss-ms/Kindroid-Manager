use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::character::Character;
#[allow(unused_imports)]
use crate::domain::chat_message::{ChatMessage, ChatSyncState};
use crate::domain::journal_entry::JournalEntry;
use crate::domain::push_log::{truncate_body, PushLogEntry};
use crate::error::{AppError, JournalEntryStep, PushResult, StepResult};
use crate::kindroid::{
    ChatBreakRequest, HttpResponse, JournalCreateRequest, KindroidClient, KindroidError,
    UpdateInfoRequest,
};
use crate::security::secrets::Secrets;
use crate::storage::Repository;

pub const SETTING_BASE_URL: &str = "base_url";
pub const SETTING_BASE_URL_PUBLIC: &str = SETTING_BASE_URL;
pub const DEFAULT_BASE_URL: &str = "https://api.kindroid.ai/v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct PushRequest {
    pub character_id: Uuid,
    pub target_id: Uuid,
    pub fields: Vec<String>,
    pub chat_break: Option<ChatBreakInput>,
    #[serde(default)]
    pub journal_entry_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatBreakInput {
    pub greeting: String,
    pub wipe_cascaded: bool,
}

pub async fn push_to_target(
    repo: std::sync::Arc<dyn Repository>,
    client: std::sync::Arc<dyn KindroidClient>,
    req: PushRequest,
) -> Result<PushResult, AppError> {
    do_push(&*repo, &*client, req).await
}

pub async fn do_push(
    repo: &dyn Repository,
    client: &dyn KindroidClient,
    req: PushRequest,
) -> Result<PushResult, AppError> {
    let character = repo.get_character(req.character_id).await?;
    let mut target = repo.get_target(req.target_id).await?;
    let base_url = repo
        .get_setting(SETTING_BASE_URL)
        .await?
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let token = Secrets::get()?;

    for f in &req.fields {
        if !Character::PERSONA_FIELDS.iter().any(|p| *p == f) {
            return Err(AppError::invalid(format!("unknown field: {f}")));
        }
    }
    let chat_break = match &req.chat_break {
        Some(cb) => {
            let g = cb.greeting.trim();
            if g.is_empty() {
                return Err(AppError::MissingGreeting);
            }
            Some((g.to_string(), cb.wipe_cascaded))
        }
        None => None,
    };

    // Resolve journal entries to push (if any were requested).
    let journal_entries: Vec<JournalEntry> =
        match req.journal_entry_ids.as_ref().filter(|ids| !ids.is_empty()) {
            Some(ids) => {
                let all = repo.list_journal_entries(character.id).await?;
                let mut ordered: Vec<JournalEntry> = all
                    .into_iter()
                    .filter(|e| ids.iter().any(|i| i == &e.id))
                    .collect();
                // Preserve request ordering for visual consistency.
                ordered.sort_by_key(|e| ids.iter().position(|i| i == &e.id).unwrap_or(usize::MAX));
                if ordered.is_empty() {
                    return Err(AppError::invalid("no matching journal entries"));
                }
                for e in &ordered {
                    JournalEntry::validate(&e.entry, &e.keyphrases).map_err(AppError::invalid)?;
                }
                ordered
            }
            None => Vec::new(),
        };

    if req.fields.is_empty() && chat_break.is_none() && journal_entries.is_empty() {
        return Err(AppError::NothingToPush);
    }

    // Build update-info body. greeting is NEVER included.
    let mut body = serde_json::json!({ "ai_id": target.ai_id });
    for f in &req.fields {
        if let Some(v) = character.persona_field(f) {
            body[f] = serde_json::Value::String(v);
        }
    }

    let update_resp = client
        .update_info(&token, &base_url, UpdateInfoRequest { body })
        .await;

    let (update_info_result, journal_entries_result, chat_break_result) = match update_resp {
        Ok(r) => {
            let step = step_result(r);
            let journal_steps = if journal_entries.is_empty() {
                Vec::new()
            } else {
                let mut out = Vec::with_capacity(journal_entries.len());
                for e in &journal_entries {
                    let keyphrases = e.keyphrases.clone();
                    let req = JournalCreateRequest {
                        ai_id: &target.ai_id,
                        entry: &e.entry,
                        keyphrases: &keyphrases,
                    };
                    let resp = client.journal_create(&token, &base_url, req).await;
                    let (status, ok, message) = match resp {
                        Ok(r2) => (r2.status, r2.ok, truncate_body(&r2.body)),
                        Err(err) => {
                            let s = error_step_result(&err);
                            (s.status, s.ok, s.message)
                        }
                    };
                    out.push(JournalEntryStep {
                        id: e.id.clone(),
                        status,
                        ok,
                        message,
                    });
                }
                out
            };
            let cb_step = if let Some((greeting, wipe)) = chat_break {
                let cb_req = ChatBreakRequest {
                    ai_id: target.ai_id.clone(),
                    greeting: greeting.clone(),
                    wipe_cascaded: wipe,
                };
                let resp = client.chat_break(&token, &base_url, cb_req).await;
                Some(match resp {
                    Ok(r) => step_result(r),
                    Err(e) => error_step_result(&e),
                })
            } else {
                None
            };
            (step, journal_steps, cb_step)
        }
        Err(e) => (error_step_result(&e), Vec::new(), None),
    };

    let fields_sent = req.fields.clone();
    let journal_ids_sent = if journal_entries_result.is_empty() {
        None
    } else {
        Some(
            journal_entries_result
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
        )
    };
    // If the AI name was part of this push, keep the target's local label
    // in sync with the AI name so the Targets list reflects the persona
    // that was just pushed. The local `character.name` is intentionally
    // NOT used — the label mirrors the AI identity that lives on the
    // server. Skip the update when the AI name is missing or empty so we
    // never blank out an existing label.
    if req.fields.iter().any(|f| f == "ai_name") {
        if let Some(ai_name) = character.ai_name.as_ref() {
            if !ai_name.is_empty() {
                target.label = ai_name.clone();
            }
        }
    }
    let target = repo.upsert_target(target).await?;
    let entry = PushLogEntry {
        id: Uuid::new_v4(),
        at: Utc::now(),
        character_id: character.id,
        character_name: character.name.clone(),
        target_id: target.id,
        target_ai_id: target.ai_id.clone(),
        fields_sent,
        did_chat_break: req.chat_break.is_some(),
        greeting: req
            .chat_break
            .as_ref()
            .map(|cb| cb.greeting.trim().to_string()),
        wipe_cascaded: req.chat_break.as_ref().map(|cb| cb.wipe_cascaded),
        update_info_status: update_info_result.status,
        update_info_body: update_info_result.message.clone(),
        chat_break_status: chat_break_result.as_ref().map(|s| s.status),
        chat_break_body: chat_break_result.as_ref().map(|s| s.message.clone()),
        journal_entry_ids: journal_ids_sent,
    };
    let stored = repo.append_push_log(entry).await?;

    Ok(PushResult {
        update_info: update_info_result,
        journal_entries: journal_entries_result,
        chat_break: chat_break_result,
        log_id: stored.id,
    })
}

fn step_result(r: HttpResponse) -> StepResult {
    StepResult {
        status: r.status,
        ok: r.ok,
        message: truncate_body(&r.body),
    }
}

fn error_step_result(e: &KindroidError) -> StepResult {
    let status = match e {
        KindroidError::Auth { status, .. }
        | KindroidError::RateLimited { status, .. }
        | KindroidError::BadRequest { status, .. }
        | KindroidError::NotFound { status, .. }
        | KindroidError::Server { status, .. } => *status,
        KindroidError::Network(_) => 0,
    };
    StepResult {
        status,
        ok: false,
        message: format!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::character::Character;
    use crate::domain::push_log::PushLogEntry;
    use crate::domain::target::Target;
    use crate::kindroid::{HttpResponse, KindroidError};
    use crate::storage::StorageError;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct FakeRepo {
        characters: Mutex<Vec<Character>>,
        targets: Mutex<Vec<Target>>,
        log: Mutex<Vec<PushLogEntry>>,
        images: Mutex<std::collections::HashMap<Uuid, Vec<u8>>>,
        journal: Mutex<Vec<JournalEntry>>,
    }

    impl FakeRepo {
        fn new(c: Character, t: Target) -> Self {
            Self {
                characters: Mutex::new(vec![c]),
                targets: Mutex::new(vec![t]),
                log: Mutex::new(Vec::new()),
                images: Mutex::new(std::collections::HashMap::new()),
                journal: Mutex::new(Vec::new()),
            }
        }
        fn set_journal(&self, entries: Vec<JournalEntry>) {
            *self.journal.lock().unwrap() = entries;
        }
    }

    #[async_trait]
    impl Repository for FakeRepo {
        async fn list_characters(&self) -> Result<Vec<Character>, StorageError> {
            Ok(self.characters.lock().unwrap().clone())
        }
        async fn get_character(&self, id: Uuid) -> Result<Character, StorageError> {
            self.characters
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned()
                .ok_or(StorageError::NotFound)
        }
        async fn upsert_character(&self, c: Character) -> Result<Character, StorageError> {
            self.characters.lock().unwrap().push(c.clone());
            Ok(c)
        }
        async fn delete_character(&self, id: Uuid) -> Result<(), StorageError> {
            self.characters.lock().unwrap().retain(|c| c.id != id);
            Ok(())
        }
        async fn list_targets(&self) -> Result<Vec<Target>, StorageError> {
            Ok(self.targets.lock().unwrap().clone())
        }
        async fn get_target(&self, id: Uuid) -> Result<Target, StorageError> {
            self.targets
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or(StorageError::NotFound)
        }
        async fn upsert_target(&self, t: Target) -> Result<Target, StorageError> {
            let mut targets = self.targets.lock().unwrap();
            // Mirror the SqliteRepository contract: if a row with the same
            // `ai_id` already exists, merge into it (keep its id, update label).
            if let Some(existing) = targets.iter_mut().find(|x| x.ai_id == t.ai_id) {
                existing.label = t.label.clone();
                return Ok(existing.clone());
            }
            targets.push(t.clone());
            Ok(t)
        }
        async fn delete_target(&self, id: Uuid) -> Result<(), StorageError> {
            self.targets.lock().unwrap().retain(|t| t.id != id);
            Ok(())
        }
        async fn append_push_log(&self, e: PushLogEntry) -> Result<PushLogEntry, StorageError> {
            self.log.lock().unwrap().push(e.clone());
            Ok(e)
        }
        async fn list_push_history(
            &self,
            _l: u32,
            _o: u32,
        ) -> Result<Vec<PushLogEntry>, StorageError> {
            Ok(self.log.lock().unwrap().clone())
        }
        async fn get_push_log(&self, id: Uuid) -> Result<PushLogEntry, StorageError> {
            self.log
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .ok_or(StorageError::NotFound)
        }
        async fn get_setting(&self, _k: &str) -> Result<Option<String>, StorageError> {
            Ok(Some(DEFAULT_BASE_URL.into()))
        }
        async fn set_setting(&self, _k: &str, _v: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn save_character_image_bytes(
            &self,
            character_id: Uuid,
            bytes: &[u8],
        ) -> Result<String, StorageError> {
            let rel = format!("images/{character_id}.bin");
            self.images
                .lock()
                .unwrap()
                .insert(character_id, bytes.to_vec());
            let mut chars = self.characters.lock().unwrap();
            if let Some(c) = chars.iter_mut().find(|c| c.id == character_id) {
                c.cover_image = Some(rel.clone());
            }
            Ok(rel)
        }
        async fn read_character_image_bytes(
            &self,
            id: Uuid,
        ) -> Result<Option<Vec<u8>>, StorageError> {
            Ok(self.images.lock().unwrap().get(&id).cloned())
        }
        async fn delete_character_image_bytes(&self, id: Uuid) -> Result<(), StorageError> {
            self.images.lock().unwrap().remove(&id);
            Ok(())
        }
        async fn upsert_chat_messages(
            &self,
            _ai_id: &str,
            _msgs: &[ChatMessage],
        ) -> Result<usize, StorageError> {
            Ok(0)
        }
        async fn list_chat_messages(
            &self,
            _ai_id: &str,
            _before_ts: Option<i64>,
            _limit: u32,
            _favourites_only: bool,
        ) -> Result<Vec<ChatMessage>, StorageError> {
            Ok(Vec::new())
        }
        async fn search_chat(
            &self,
            _ai_id: &str,
            _query: &str,
            _limit: u32,
            _offset: u32,
            _favourites_only: bool,
        ) -> Result<Vec<ChatMessage>, StorageError> {
            Ok(Vec::new())
        }
        async fn set_chat_message_favourite(
            &self,
            _ai_id: &str,
            _kindroid_msg_id: &str,
            _favourite: bool,
        ) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn chat_message_count(&self, _ai_id: &str) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn get_chat_sync_state(
            &self,
            _ai_id: &str,
        ) -> Result<Option<ChatSyncState>, StorageError> {
            Ok(None)
        }
        async fn upsert_chat_sync_state(&self, _state: &ChatSyncState) -> Result<(), StorageError> {
            Ok(())
        }
        async fn reset_chat_history(&self, _ai_id: &str) -> Result<usize, StorageError> {
            Ok(0)
        }
        async fn delete_missing_chat_messages(
            &self,
            _ai_id: &str,
            _start_after: i64,
            _last_timestamp_inclusive: i64,
            _keep_ids: &[&str],
        ) -> Result<usize, StorageError> {
            Ok(0)
        }
        async fn list_journal_entries(
            &self,
            character_id: Uuid,
        ) -> Result<Vec<JournalEntry>, StorageError> {
            Ok(self
                .journal
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.character_id == character_id)
                .cloned()
                .collect())
        }
        async fn upsert_journal_entry(&self, entry: &JournalEntry) -> Result<(), StorageError> {
            self.journal.lock().unwrap().push(entry.clone());
            Ok(())
        }
        async fn delete_journal_entry(
            &self,
            character_id: Uuid,
            entry_id: &str,
        ) -> Result<(), StorageError> {
            let mut g = self.journal.lock().unwrap();
            let before = g.len();
            g.retain(|e| !(e.id == entry_id && e.character_id == character_id));
            if g.len() == before {
                return Err(StorageError::NotFound);
            }
            Ok(())
        }
    }

    struct FakeClient {
        update: Mutex<Option<Result<HttpResponse, KindroidError>>>,
        chat_break: Mutex<Option<Result<HttpResponse, KindroidError>>>,
        list_chat: Mutex<Option<Result<crate::kindroid::ChatMessagesPage, KindroidError>>>,
        journal_results: Mutex<Vec<Result<HttpResponse, KindroidError>>>,
        journal_calls: Mutex<Vec<JournalCallRecord>>,
    }

    #[derive(Debug, Clone)]
    struct JournalCallRecord {
        ai_id: String,
        entry: String,
        keyphrases: Vec<String>,
    }

    impl FakeClient {
        fn ok_both() -> Self {
            Self {
                update: Mutex::new(Some(Ok(HttpResponse {
                    status: 200,
                    ok: true,
                    body: "ok".into(),
                }))),
                chat_break: Mutex::new(Some(Ok(HttpResponse {
                    status: 200,
                    ok: true,
                    body: "ok".into(),
                }))),
                list_chat: Mutex::new(None),
                journal_results: Mutex::new(Vec::new()),
                journal_calls: Mutex::new(Vec::new()),
            }
        }
        fn push_journal_result(&self, r: Result<HttpResponse, KindroidError>) {
            self.journal_results.lock().unwrap().push(r);
        }
        fn journal_calls(&self) -> Vec<JournalCallRecord> {
            self.journal_calls.lock().unwrap().clone()
        }
    }
    #[async_trait]
    impl KindroidClient for FakeClient {
        async fn update_info(
            &self,
            _t: &str,
            _u: &str,
            _r: UpdateInfoRequest,
        ) -> Result<HttpResponse, KindroidError> {
            self.update
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(HttpResponse {
                    status: 200,
                    ok: true,
                    body: "ok".into(),
                }))
        }
        async fn chat_break(
            &self,
            _t: &str,
            _u: &str,
            _r: ChatBreakRequest,
        ) -> Result<HttpResponse, KindroidError> {
            self.chat_break
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(HttpResponse {
                    status: 200,
                    ok: true,
                    body: "ok".into(),
                }))
        }
        async fn list_chat_messages(
            &self,
            _t: &str,
            _u: &str,
            _r: crate::kindroid::ListChatMessagesRequest,
        ) -> Result<crate::kindroid::ChatMessagesPage, KindroidError> {
            self.list_chat
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(crate::kindroid::ChatMessagesPage {
                    messages: Vec::new(),
                    has_more: false,
                    limit: 100,
                    pagination_last_timestamp: None,
                }))
        }
        async fn toggle_message_pin(
            &self,
            _t: &str,
            _u: &str,
            _r: crate::kindroid::ToggleMessagePinRequest,
        ) -> Result<crate::kindroid::ToggleMessagePinResponse, KindroidError> {
            Ok(crate::kindroid::ToggleMessagePinResponse { is_pinned: true })
        }
        async fn journal_create(
            &self,
            _t: &str,
            _u: &str,
            r: crate::kindroid::JournalCreateRequest<'_>,
        ) -> Result<HttpResponse, KindroidError> {
            self.journal_calls.lock().unwrap().push(JournalCallRecord {
                ai_id: r.ai_id.to_string(),
                entry: r.entry.to_string(),
                keyphrases: r.keyphrases.to_vec(),
            });
            let scripted = self.journal_results.lock().unwrap().pop();
            scripted.unwrap_or_else(|| {
                Ok(HttpResponse {
                    status: 200,
                    ok: true,
                    body: "ok".into(),
                })
            })
        }
    }

    fn fixtures() -> (Character, Target) {
        (
            Character {
                id: Uuid::new_v4(),
                name: "C".into(),
                ai_name: Some("Aria".into()),
                ai_gender: None,
                ai_backstory: Some("Backstory".into()),
                ai_memory: None,
                ai_directive: None,
                ai_example_message: None,
                ai_additional_context: None,
                current_scene: None,
                user_name: None,
                user_gender: None,
                greeting: Some("Hello!".into()),
                notes: None,
                ai_avatar_description: None,
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            Target {
                id: Uuid::new_v4(),
                ai_id: "ai_1".into(),
                label: "T".into(),
                created_at: Utc::now(),
            },
        )
    }

    fn set_token() {
        crate::security::secrets::Secrets::set("test-token").unwrap();
    }

    #[tokio::test]
    async fn happy_path_no_chat_break() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: None,
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok);
        assert!(res.chat_break.is_none());
        let log = repo.log.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].update_info_body.contains("ok"));
    }

    #[tokio::test]
    async fn happy_path_with_chat_break() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: Some(ChatBreakInput {
                greeting: "Hi there".into(),
                wipe_cascaded: true,
            }),
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok);
        let cb = res.chat_break.unwrap();
        assert!(cb.ok);
        let log = repo.log.lock().unwrap();
        assert!(log[0].did_chat_break);
        assert_eq!(log[0].greeting.as_deref(), Some("Hi there"));
        assert_eq!(log[0].wipe_cascaded, Some(true));
    }

    #[tokio::test]
    async fn update_info_failure_skips_chat_break() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient {
            update: Mutex::new(Some(Err(KindroidError::Auth {
                status: 401,
                body: "nope".into(),
            }))),
            chat_break: Mutex::new(Some(Ok(HttpResponse {
                status: 200,
                ok: true,
                body: "ok".into(),
            }))),
            list_chat: Mutex::new(None),
            journal_results: Mutex::new(Vec::new()),
            journal_calls: Mutex::new(Vec::new()),
        };
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: Some(ChatBreakInput {
                greeting: "Hi".into(),
                wipe_cascaded: false,
            }),
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(!res.update_info.ok);
        assert!(res.chat_break.is_none());
    }

    #[tokio::test]
    async fn chat_break_failure_still_logs_both() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient {
            update: Mutex::new(Some(Ok(HttpResponse {
                status: 200,
                ok: true,
                body: "ok".into(),
            }))),
            chat_break: Mutex::new(Some(Err(KindroidError::Server {
                status: 500,
                body: "boom".into(),
            }))),
            list_chat: Mutex::new(None),
            journal_results: Mutex::new(Vec::new()),
            journal_calls: Mutex::new(Vec::new()),
        };
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: Some(ChatBreakInput {
                greeting: "Hi".into(),
                wipe_cascaded: false,
            }),
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok);
        let cb = res.chat_break.unwrap();
        assert!(!cb.ok);
        assert_eq!(cb.status, 500);
    }

    #[tokio::test]
    async fn validation_rejects_empty_fields_and_no_chat_break() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec![],
            chat_break: None,
            journal_entry_ids: None,
        };
        let err = do_push(&repo, &client, req).await.unwrap_err();
        matches!(err, AppError::NothingToPush);
    }

    #[tokio::test]
    async fn validation_rejects_empty_greeting() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec![],
            chat_break: Some(ChatBreakInput {
                greeting: "   ".into(),
                wipe_cascaded: false,
            }),
            journal_entry_ids: None,
        };
        let err = do_push(&repo, &client, req).await.unwrap_err();
        matches!(err, AppError::MissingGreeting);
    }

    #[tokio::test]
    async fn push_with_ai_name_renames_target_to_ai_name() {
        set_token();
        let (c, t) = fixtures();
        let original_label = t.label.clone();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: None,
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok, "update-info should have succeeded");
        let updated = repo.get_target(t.id).await.unwrap();
        assert_eq!(updated.label, c.ai_name.as_deref().unwrap());
        assert_ne!(updated.label, original_label);
        assert_ne!(updated.label, c.name);
    }

    #[tokio::test]
    async fn push_without_ai_name_does_not_rename_target() {
        set_token();
        let (c, t) = fixtures();
        let original_label = t.label.clone();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_backstory".into()],
            chat_break: None,
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok, "update-info should have succeeded");
        let updated = repo.get_target(t.id).await.unwrap();
        assert_eq!(updated.label, original_label);
    }

    #[tokio::test]
    async fn push_with_empty_ai_name_does_not_rename_target() {
        set_token();
        let (mut c, t) = fixtures();
        c.ai_name = Some(String::new());
        let original_label = t.label.clone();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: None,
            journal_entry_ids: None,
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok);
        let updated = repo.get_target(t.id).await.unwrap();
        assert_eq!(updated.label, original_label);
    }

    fn make_entry(character_id: Uuid, id: &str, entry: &str, kp: &[&str]) -> JournalEntry {
        let now = Utc::now();
        JournalEntry {
            id: id.to_string(),
            character_id,
            entry: entry.to_string(),
            keyphrases: kp.iter().map(|s| s.to_string()).collect(),
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn journal_skipped_when_no_ids() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: None,
            journal_entry_ids: Some(Vec::new()),
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.journal_entries.is_empty());
        assert!(client.journal_calls().is_empty());
    }

    #[tokio::test]
    async fn update_info_failure_skips_journal() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient {
            update: Mutex::new(Some(Err(KindroidError::Auth {
                status: 401,
                body: "nope".into(),
            }))),
            chat_break: Mutex::new(Some(Ok(HttpResponse {
                status: 200,
                ok: true,
                body: "ok".into(),
            }))),
            list_chat: Mutex::new(None),
            journal_results: Mutex::new(Vec::new()),
            journal_calls: Mutex::new(Vec::new()),
        };
        repo.set_journal(vec![make_entry(c.id, "je-1", "e1", &[])]);
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: None,
            journal_entry_ids: Some(vec!["je-1".to_string()]),
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(!res.update_info.ok);
        assert!(res.journal_entries.is_empty());
        assert!(client.journal_calls().is_empty());
    }

    #[tokio::test]
    async fn update_info_success_runs_journal_in_order() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let e1 = make_entry(c.id, "je-1", "first", &["a"]);
        let e2 = make_entry(c.id, "je-2", "second", &["b", "c"]);
        repo.set_journal(vec![e1.clone(), e2.clone()]);
        // Request them in reverse order to confirm we honour the request order.
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec![],
            chat_break: None,
            journal_entry_ids: Some(vec!["je-2".into(), "je-1".into()]),
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert_eq!(res.journal_entries.len(), 2);
        assert!(res.journal_entries.iter().all(|s| s.ok));
        let calls = client.journal_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].entry, "second");
        assert_eq!(calls[1].entry, "first");
        // chat-break did not run.
        assert!(res.chat_break.is_none());
    }

    #[tokio::test]
    async fn journal_partial_failure_continues() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        // Push results are popped LIFO in FakeClient.journal_create.
        client.push_journal_result(Ok(HttpResponse {
            status: 200,
            ok: true,
            body: "ok".into(),
        }));
        client.push_journal_result(Err(KindroidError::Server {
            status: 500,
            body: "boom".into(),
        }));
        let e1 = make_entry(c.id, "je-1", "first", &[]);
        let e2 = make_entry(c.id, "je-2", "second", &[]);
        repo.set_journal(vec![e1, e2]);
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec![],
            chat_break: None,
            journal_entry_ids: Some(vec!["je-1".into(), "je-2".into()]),
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert_eq!(res.journal_entries.len(), 2);
        assert!(!res.journal_entries[0].ok);
        assert_eq!(res.journal_entries[0].status, 500);
        assert!(res.journal_entries[1].ok);
    }

    #[tokio::test]
    async fn journal_validation_error_short_circuits() {
        set_token();
        let (c, t) = fixtures();
        let repo = FakeRepo::new(c.clone(), t.clone());
        let client = FakeClient::ok_both();
        let bad = JournalEntry {
            id: "je-bad".into(),
            character_id: c.id,
            entry: "a".repeat(501),
            keyphrases: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        repo.set_journal(vec![bad]);
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec![],
            chat_break: None,
            journal_entry_ids: Some(vec!["je-bad".into()]),
        };
        let err = do_push(&repo, &client, req).await.unwrap_err();
        assert!(matches!(err, AppError::Invalid { .. }));
        assert!(client.journal_calls().is_empty());
    }
}
