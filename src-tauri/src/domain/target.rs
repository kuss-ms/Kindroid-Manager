use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    #[default]
    Ai,
    Group,
}

impl TargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Group => "group",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "group" => Self::Group,
            _ => Self::Ai,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub id: Uuid,
    pub ai_id: String,
    #[serde(default)]
    pub kind: TargetKind,
    pub label: String,
    pub created_at: DateTime<Utc>,
}
