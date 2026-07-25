use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::target::Target;
use crate::error::AppError;
use crate::storage::Repository;

#[derive(Debug, Serialize, Deserialize)]
pub struct TargetInput {
    pub id: Option<Uuid>,
    pub ai_id: String,
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
    let target = Target {
        id: input.id.unwrap_or_else(Uuid::new_v4),
        ai_id: ai_id.to_string(),
        label: label.to_string(),
        created_at: chrono::Utc::now(),
    };
    Ok(repo.upsert_target(target).await?)
}

pub async fn delete_target(repo: std::sync::Arc<dyn Repository>, id: Uuid) -> Result<(), AppError> {
    Ok(repo.delete_target(id).await?)
}
