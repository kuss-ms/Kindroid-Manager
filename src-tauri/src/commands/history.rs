use uuid::Uuid;

use crate::domain::push_log::PushLogEntry;
use crate::error::AppError;
use crate::storage::Repository;

pub async fn list_push_history(
    repo: std::sync::Arc<dyn Repository>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PushLogEntry>, AppError> {
    Ok(repo.list_push_history(limit, offset).await?)
}

pub async fn get_push_log(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<PushLogEntry, AppError> {
    Ok(repo.get_push_log(id).await?)
}
