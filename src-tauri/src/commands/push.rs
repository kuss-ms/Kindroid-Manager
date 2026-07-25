use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::push_log::{truncate_body, PushLogEntry};
use crate::error::{AppError, PushResult, StepResult};
use crate::kindroid::{
    ChatBreakRequest, HttpResponse, KindroidClient, KindroidError, UpdateInfoRequest,
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
    if req.fields.is_empty() && chat_break.is_none() {
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

    let (update_info_result, chat_break_result) = match update_resp {
        Ok(r) => {
            let step = step_result(r);
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
            (step, cb_step)
        }
        Err(e) => (error_step_result(&e), None),
    };

    let fields_sent = req.fields.clone();
    // After a successful push (update-info OK, chat-break optional),
    // keep the target's local label in sync with the source character's
    // name so the Targets list reflects the persona that was just pushed.
    target.label = character.name.clone();
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
    };
    let stored = repo.append_push_log(entry).await?;

    Ok(PushResult {
        update_info: update_info_result,
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
    }

    impl FakeRepo {
        fn new(c: Character, t: Target) -> Self {
            Self {
                characters: Mutex::new(vec![c]),
                targets: Mutex::new(vec![t]),
                log: Mutex::new(Vec::new()),
                images: Mutex::new(std::collections::HashMap::new()),
            }
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
    }

    struct FakeClient {
        update: Mutex<Option<Result<HttpResponse, KindroidError>>>,
        chat_break: Mutex<Option<Result<HttpResponse, KindroidError>>>,
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
            }
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
        };
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: Some(ChatBreakInput {
                greeting: "Hi".into(),
                wipe_cascaded: false,
            }),
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
        };
        let req = PushRequest {
            character_id: c.id,
            target_id: t.id,
            fields: vec!["ai_name".into()],
            chat_break: Some(ChatBreakInput {
                greeting: "Hi".into(),
                wipe_cascaded: false,
            }),
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
        };
        let err = do_push(&repo, &client, req).await.unwrap_err();
        matches!(err, AppError::MissingGreeting);
    }

    #[tokio::test]
    async fn successful_push_renames_target_to_character_name() {
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
        };
        let res = do_push(&repo, &client, req).await.unwrap();
        assert!(res.update_info.ok, "update-info should have succeeded");
        let updated = repo.get_target(t.id).await.unwrap();
        assert_eq!(updated.label, c.name);
        assert_ne!(updated.label, original_label);
    }
}
