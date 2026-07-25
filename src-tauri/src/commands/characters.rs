use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::character::Character;
use crate::error::AppError;
use crate::storage::Repository;

#[derive(Debug, Serialize, Deserialize)]
pub struct CharacterInput {
    pub id: Option<Uuid>,
    pub name: String,
    #[serde(flatten)]
    pub fields: CharacterFields,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CharacterFields {
    pub ai_name: Option<String>,
    pub ai_gender: Option<String>,
    pub ai_backstory: Option<String>,
    pub ai_memory: Option<String>,
    pub ai_directive: Option<String>,
    pub ai_example_message: Option<String>,
    pub ai_additional_context: Option<String>,
    pub current_scene: Option<String>,
    pub greeting: Option<String>,
    pub notes: Option<String>,
    pub ai_avatar_description: Option<String>,
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let t = v.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

fn normalize(fields: CharacterFields) -> CharacterFields {
    CharacterFields {
        ai_name: empty_to_none(fields.ai_name),
        ai_gender: empty_to_none(fields.ai_gender),
        ai_backstory: empty_to_none(fields.ai_backstory),
        ai_memory: empty_to_none(fields.ai_memory),
        ai_directive: empty_to_none(fields.ai_directive),
        ai_example_message: empty_to_none(fields.ai_example_message),
        ai_additional_context: empty_to_none(fields.ai_additional_context),
        current_scene: empty_to_none(fields.current_scene),
        greeting: empty_to_none(fields.greeting),
        notes: fields.notes,
        ai_avatar_description: empty_to_none(fields.ai_avatar_description),
    }
}

pub async fn list_characters(
    repo: std::sync::Arc<dyn Repository>,
) -> Result<Vec<Character>, AppError> {
    Ok(repo.list_characters().await?)
}

pub async fn get_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Character, AppError> {
    Ok(repo.get_character(id).await?)
}

pub async fn save_character(
    repo: std::sync::Arc<dyn Repository>,
    input: CharacterInput,
) -> Result<Character, AppError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("name is required"));
    }
    let fields = normalize(input.fields);
    let new_id = input.id.unwrap_or_else(Uuid::new_v4);
    // Preserve the existing cover image when updating an existing character.
    // The image is managed separately by `set_character_image`; the form
    // never sets it directly.
    let cover_image = if input.id.is_some() {
        repo.get_character(new_id)
            .await
            .ok()
            .and_then(|c| c.cover_image)
    } else {
        None
    };
    let character = Character {
        id: new_id,
        name: name.to_string(),
        ai_name: fields.ai_name,
        ai_gender: fields.ai_gender,
        ai_backstory: fields.ai_backstory,
        ai_memory: fields.ai_memory,
        ai_directive: fields.ai_directive,
        ai_example_message: fields.ai_example_message,
        ai_additional_context: fields.ai_additional_context,
        current_scene: fields.current_scene,
        user_name: None,
        user_gender: None,
        greeting: fields.greeting,
        notes: fields.notes,
        ai_avatar_description: fields.ai_avatar_description,
        cover_image,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    Ok(repo.upsert_character(character).await?)
}

pub async fn delete_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<(), AppError> {
    Ok(repo.delete_character(id).await?)
}

pub async fn duplicate_character(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Character, AppError> {
    let original = repo.get_character(id).await?;
    let now = Utc::now();
    let mut copy = Character {
        id: Uuid::new_v4(),
        name: format!("{} (copy)", original.name),
        ..original.clone()
    };
    copy.created_at = now;
    copy.updated_at = now;
    // Give the duplicate its own copy of the cover image so editing one
    // character doesn't affect the other. Without this the duplicate's
    // `cover_image` would point at the original's file, which the editor
    // can't find (it looks up by the duplicate's own id).
    if let Some(bytes) = repo.read_character_image_bytes(id).await? {
        let new_rel = repo
            .save_character_image_bytes(copy.id, &bytes)
            .await
            .map_err(AppError::from)?;
        copy.cover_image = Some(new_rel);
    } else {
        copy.cover_image = None;
    }
    Ok(repo.upsert_character(copy).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteRepository;
    use std::sync::Arc;

    fn input(name: &str) -> CharacterInput {
        CharacterInput {
            id: None,
            name: name.to_string(),
            fields: CharacterFields::default(),
        }
    }

    #[tokio::test]
    async fn save_character_preserves_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        // Create a character with a cover image.
        let mut c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Test".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        let _ = repo
            .save_character_image_bytes(c.id, b"some-image-bytes")
            .await
            .unwrap();
        c = repo.get_character(c.id).await.unwrap();
        assert!(c.cover_image.is_some(), "cover image should be set");
        let cover_path = c.cover_image.clone().unwrap();

        // Save the character with a new name (no image change).
        let saved = save_character(
            repo.clone(),
            CharacterInput {
                id: Some(c.id),
                name: "Renamed".into(),
                fields: CharacterFields::default(),
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.name, "Renamed");
        assert_eq!(
            saved.cover_image.as_deref(),
            Some(cover_path.as_str()),
            "cover image must survive save"
        );
    }

    #[tokio::test]
    async fn save_character_new_creates_no_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let saved = save_character(repo.clone(), input("Fresh")).await.unwrap();
        assert!(saved.cover_image.is_none());
    }

    #[tokio::test]
    async fn duplicate_copies_cover_image_to_a_new_file() {
        // Image storage needs a real data_dir, so use a temp dir instead
        // of the in-memory repo.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open(&db_path).unwrap());

        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Original".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();
        repo.save_character_image_bytes(c.id, b"\x89PNG\r\n\x1a\nfake-png")
            .await
            .unwrap();
        let original = repo.get_character(c.id).await.unwrap();
        let original_path = original.cover_image.clone().unwrap();
        let original_bytes = repo
            .read_character_image_bytes(c.id)
            .await
            .unwrap()
            .unwrap();

        let dup = duplicate_character(repo.clone(), c.id).await.unwrap();

        // Duplicate owns a different image file.
        assert_ne!(dup.cover_image.as_deref(), Some(original_path.as_str()));
        let dup_bytes = repo
            .read_character_image_bytes(dup.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dup_bytes, original_bytes);

        // Deleting the duplicate must not affect the original's file.
        repo.delete_character(dup.id).await.unwrap();
        let still_there = repo.read_character_image_bytes(c.id).await.unwrap();
        assert_eq!(still_there, Some(original_bytes));
    }

    #[tokio::test]
    async fn overwriting_image_with_different_extension_returns_new_bytes() {
        // Regression: previously `read_character_image_bytes` iterated a
        // hardcoded list of extensions and returned whichever file existed
        // first, so overwriting a PNG with a JPG (or vice versa) would
        // leave the stale file on disk and cause subsequent reads to
        // return the wrong bytes.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open(&db_path).unwrap());

        let c = repo
            .upsert_character(Character {
                id: Uuid::new_v4(),
                name: "Subject".into(),
                cover_image: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                ..make_minimal_character()
            })
            .await
            .unwrap();

        let png_bytes: &[u8] = b"\x89PNG\r\n\x1a\noriginal-png-payload";
        repo.save_character_image_bytes(c.id, png_bytes)
            .await
            .unwrap();
        let jpg_bytes: &[u8] = b"\xff\xd8\xff\xe0replacement-jpg-payload";
        repo.save_character_image_bytes(c.id, jpg_bytes)
            .await
            .unwrap();

        let read_back = repo
            .read_character_image_bytes(c.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            read_back, jpg_bytes,
            "must return the newly uploaded bytes, not the stale PNG"
        );
    }

    fn make_minimal_character() -> Character {
        Character {
            id: Uuid::new_v4(),
            name: "x".into(),
            ai_name: None,
            ai_gender: None,
            ai_backstory: None,
            ai_memory: None,
            ai_directive: None,
            ai_example_message: None,
            ai_additional_context: None,
            current_scene: None,
            user_name: None,
            user_gender: None,
            greeting: None,
            notes: None,
            ai_avatar_description: None,
            cover_image: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
