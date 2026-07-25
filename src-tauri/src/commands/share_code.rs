use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::image_share::{
    decode_image, encode_image, strip_kindroid_metadata, ImageShareError,
};
use crate::domain::share_code::PartialCharacter;
use crate::error::AppError;
use crate::storage::Repository;

/// Decode a Kindroid share image: extract the persona payload, save the
/// image, and create a new character.
pub async fn import_share_image(
    repo: std::sync::Arc<dyn Repository>,
    bytes: Vec<u8>,
) -> Result<Character, AppError> {
    let partial = decode_image(&bytes).map_err(image_share_err_to_app)?;
    let mut draft = partial_to_character(&partial);
    let stored = repo
        .save_character_image_bytes(draft.id, &bytes)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    // `save_character_image_bytes` already wrote the value to the row,
    // but the in-memory `draft` doesn't see that change. Set the field
    // here so the returned character matches the DB.
    draft.cover_image = Some(stored);
    Ok(repo.upsert_character(draft).await?)
}

/// Encode a Character into a PNG with the persona payload embedded.
pub async fn export_share_image(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Vec<u8>, AppError> {
    let character = repo.get_character(id).await?;
    let image_bytes = repo
        .read_character_image_bytes(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))?
        .ok_or_else(|| AppError::invalid("character has no cover image"))?;
    encode_image(&image_bytes, &character).map_err(image_share_err_to_app)
}

/// Upload an image to an existing character. Returns the updated character.
/// Any `kindroid` metadata in the image is stripped before saving — the
/// editor's upload path is for cover images, not for re-importing a
/// share code (use the global drag-drop / paste for that).
pub async fn set_character_image(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
    bytes: Vec<u8>,
) -> Result<Character, AppError> {
    let cleaned = strip_kindroid_metadata(&bytes);
    let _stored = repo
        .save_character_image_bytes(id, &cleaned)
        .await
        .map_err(|e| AppError::database(e.to_string()))?;
    repo.get_character(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))
}

/// Load the cover image bytes for a character. Returns `None` if no image.
pub async fn get_character_image(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<Option<Vec<u8>>, AppError> {
    repo.read_character_image_bytes(id)
        .await
        .map_err(|e| AppError::database(e.to_string()))
}

fn image_share_err_to_app(e: ImageShareError) -> AppError {
    AppError::share(format!("{e}"))
}

pub fn partial_to_character(p: &PartialCharacter) -> Character {
    let now = Utc::now();
    Character {
        id: Uuid::new_v4(),
        name: default_name_from_partial(p),
        ai_name: p.ai_name.clone(),
        ai_gender: p.ai_gender.clone(),
        ai_backstory: p.ai_backstory.clone(),
        ai_memory: p.ai_memory.clone(),
        ai_directive: p.ai_directive.clone(),
        ai_example_message: p.ai_example_message.clone(),
        ai_additional_context: p.ai_additional_context.clone(),
        current_scene: p.current_scene.clone(),
        user_name: None,
        user_gender: None,
        greeting: p.greeting.clone(),
        notes: None,
        cover_image: None,
        created_at: now,
        updated_at: now,
    }
}

fn default_name_from_partial(p: &PartialCharacter) -> String {
    p.ai_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Imported character".to_string())
}

#[derive(Debug, Serialize)]
pub struct ExportShareCodeResponse {
    pub code: String,
}

#[allow(dead_code)]
pub async fn export_share_code_full(
    repo: std::sync::Arc<dyn Repository>,
    id: Uuid,
) -> Result<ExportShareCodeResponse, AppError> {
    let c = repo.get_character(id).await?;
    Ok(ExportShareCodeResponse {
        code: crate::domain::share_code::encode(&c),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::image_share::encode_image;
    use crate::storage::sqlite::SqliteRepository;
    use chrono::Utc;
    use std::sync::Arc;

    fn make_png() -> Vec<u8> {
        let mut data = Vec::with_capacity(4);
        for _ in 0..1 {
            data.extend_from_slice(&[255, 0, 0, 255]);
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    fn make_character() -> Character {
        Character {
            id: Uuid::new_v4(),
            name: "Test".into(),
            ai_name: Some("Aria".into()),
            ai_gender: Some("Female".into()),
            ai_backstory: None,
            ai_memory: None,
            ai_directive: None,
            ai_example_message: None,
            ai_additional_context: None,
            current_scene: None,
            user_name: None,
            user_gender: None,
            greeting: Some("Hello".into()),
            notes: None,
            cover_image: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn import_share_image_returns_character_with_cover_image() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        let png = make_png();
        let encoded = encode_image(&png, &character).unwrap();
        let returned = import_share_image(repo.clone(), encoded).await.unwrap();
        assert!(
            returned.cover_image.is_some(),
            "returned character must have cover_image"
        );
        // The DB row should also have cover_image set.
        let stored = repo.get_character(returned.id).await.unwrap();
        assert_eq!(stored.cover_image, returned.cover_image);
    }

    #[tokio::test]
    async fn character_field_does_not_include_user_name_or_gender() {
        let repo: Arc<dyn Repository> = Arc::new(SqliteRepository::open_in_memory().unwrap());
        let character = make_character();
        let png = make_png();
        let encoded = encode_image(&png, &character).unwrap();
        let returned = import_share_image(repo.clone(), encoded).await.unwrap();
        assert!(returned.user_name.is_none());
        assert!(returned.user_gender.is_none());
    }
}
