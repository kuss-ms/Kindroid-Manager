use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub id: Uuid,
    pub ai_id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
}
