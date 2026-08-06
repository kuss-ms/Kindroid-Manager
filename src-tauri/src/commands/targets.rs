use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::target::{Target, TargetKind};
use crate::error::AppError;
use crate::storage::Repository;

#[derive(Debug, Serialize, Deserialize)]
pub struct TargetInput {
    pub id: Option<Uuid>,
    pub ai_id: String,
    #[serde(default)]
    pub kind: TargetKind,
    pub label: String,
}

pub async fn list_targets(repo: std::sync::Arc<dyn Repository>) -> Result<Vec<Target>, AppError> {
    Ok(repo.list_targets().await?)
}

pub async fn get_target(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Target, AppError> {
    Ok(repo.get_target(id).await?)
}

pub async fn save_target(
    repo: std::sync::Arc<dyn Repository>,
    input: TargetInput,
) -> Result<Target, AppError> {
    let ai_id = input.ai_id.trim();
    let label = input.label.trim();
    if ai_id.is_empty() {
        return Err(AppError::invalid("ai_id is required"));
    }
    if label.is_empty() {
        return Err(AppError::invalid("label is required"));
    }
    // Kind is immutable after creation (see plan §8). When editing an
    // existing target the kind must already match — flipping it would
    // orphan every chat_messages / chat_sync_state row whose (ai_id,
    // kind) no longer resolves. Look up the existing row by id and
    // reject mismatches with a friendly message so the user gets
    // something better than a SQLite UNIQUE conflict.
    if let Some(id) = input.id {
        match repo.get_target(id).await {
            Ok(existing) => {
                if existing.kind != input.kind {
                    return Err(AppError::invalid("target kind cannot be changed"));
                }
            }
            Err(crate::storage::StorageError::NotFound) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let target = Target {
        id: input.id.unwrap_or_else(Uuid::new_v4),
        ai_id: ai_id.to_string(),
        kind: input.kind,
        label: label.to_string(),
        created_at: chrono::Utc::now(),
    };
    Ok(repo.upsert_target(target).await?)
}

pub async fn delete_target(repo: std::sync::Arc<dyn Repository>, id: Uuid) -> Result<(), AppError> {
    Ok(repo.delete_target(id).await?)
}
