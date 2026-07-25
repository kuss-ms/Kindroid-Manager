use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Character {
    pub id: Uuid,
    pub name: String,
    pub ai_name: Option<String>,
    pub ai_gender: Option<String>,
    pub ai_backstory: Option<String>,
    pub ai_memory: Option<String>,
    pub ai_directive: Option<String>,
    pub ai_example_message: Option<String>,
    pub ai_additional_context: Option<String>,
    pub current_scene: Option<String>,
    pub user_name: Option<String>,
    pub user_gender: Option<String>,
    pub greeting: Option<String>,
    pub notes: Option<String>,
    /// Local-only description of the avatar's appearance. Not pushed to
    /// Kindroid (their API has no field for it); the push dialog exposes a
    /// "Copy" button so the user can paste it manually.
    #[serde(default)]
    pub ai_avatar_description: Option<String>,
    /// Path to the cover image relative to the data dir (e.g. `images/{id}.png`).
    #[serde(default)]
    pub cover_image: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Character {
    pub fn persona_field(&self, field: &str) -> Option<String> {
        match field {
            "ai_name" => self.ai_name.clone(),
            "ai_gender" => self.ai_gender.clone(),
            "ai_backstory" => self.ai_backstory.clone(),
            "ai_memory" => self.ai_memory.clone(),
            "ai_directive" => self.ai_directive.clone(),
            "ai_example_message" => self.ai_example_message.clone(),
            "ai_additional_context" => self.ai_additional_context.clone(),
            "current_scene" => self.current_scene.clone(),
            other => panic!("unknown persona field: {other}"),
        }
    }

    pub const PERSONA_FIELDS: &'static [&'static str] = &[
        "ai_name",
        "ai_gender",
        "ai_backstory",
        "ai_memory",
        "ai_directive",
        "ai_example_message",
        "ai_additional_context",
        "current_scene",
    ];

    pub const AI_FIELDS: &'static [&'static str] = &[
        "ai_name",
        "ai_gender",
        "ai_backstory",
        "ai_memory",
        "ai_directive",
        "ai_example_message",
        "ai_additional_context",
        "current_scene",
    ];
}
