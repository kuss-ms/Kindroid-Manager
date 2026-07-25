use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::character::Character;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Error, Serialize)]
pub enum ShareCodeError {
    #[error("malformed share code: {0}")]
    Malformed(String),
    #[error("unsupported share-code version: {0}")]
    UnsupportedVersion(u32),
}

/// All persona fields plus optional greeting. Mirrors the `p` object
/// in the wire format. Null fields are omitted from the JSON so legacy
/// v1 codes (without `greeting`) round-trip stably.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PartialCharacter {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_backstory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_directive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_example_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ai_additional_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub current_scene: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub greeting: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShareCodePayload {
    v: u32,
    p: PartialCharacter,
}

pub fn encode(character: &Character) -> String {
    let partial = build_partial(character);
    let payload = ShareCodePayload {
        v: CURRENT_VERSION,
        p: partial,
    };
    // Serialise to serde_json::Value, strip nulls, re-stringify for
    // deterministic keys-free JSON. serde_json::to_string is already
    // deterministic in key order (struct field order).
    let json = serde_json::to_string(&payload).expect("serializable");
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

/// Builds the persona-only payload for export. Used by both the
/// text-based `encode` and the image-based `image_share::encode_image`.
pub fn build_partial(character: &Character) -> PartialCharacter {
    PartialCharacter {
        ai_name: character.ai_name.clone(),
        ai_gender: character.ai_gender.clone(),
        ai_backstory: character.ai_backstory.clone(),
        ai_memory: character.ai_memory.clone(),
        ai_directive: character.ai_directive.clone(),
        ai_example_message: character.ai_example_message.clone(),
        ai_additional_context: character.ai_additional_context.clone(),
        current_scene: character.current_scene.clone(),
        greeting: character.greeting.clone(),
    }
}

pub fn decode(s: &str) -> Result<PartialCharacter, ShareCodeError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| ShareCodeError::Malformed(format!("base64: {e}")))?;
    let json =
        std::str::from_utf8(&bytes).map_err(|e| ShareCodeError::Malformed(format!("utf8: {e}")))?;
    let payload: ShareCodePayload =
        serde_json::from_str(json).map_err(|e| ShareCodeError::Malformed(format!("json: {e}")))?;
    if payload.v != CURRENT_VERSION {
        return Err(ShareCodeError::UnsupportedVersion(payload.v));
    }
    Ok(payload.p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_full_character() -> Character {
        Character {
            id: Uuid::new_v4(),
            name: "Test".into(),
            ai_name: Some("Aria".into()),
            ai_gender: Some("female".into()),
            ai_backstory: Some("Backstory".into()),
            ai_memory: Some("Memory".into()),
            ai_directive: Some("Directive".into()),
            ai_example_message: Some("Example".into()),
            ai_additional_context: Some("Context".into()),
            current_scene: Some("Scene".into()),
            user_name: Some("Eric".into()),
            user_gender: Some("male".into()),
            greeting: Some("Hello!".into()),
            notes: Some("local only".into()),
            cover_image: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn round_trip_with_all_fields() {
        let c = make_full_character();
        let code = encode(&c);
        let partial = decode(&code).expect("decode");
        assert_eq!(partial.ai_name.as_deref(), Some("Aria"));
        assert_eq!(partial.greeting.as_deref(), Some("Hello!"));
    }

    #[test]
    fn round_trip_with_partial_fields() {
        let mut c = make_full_character();
        c.ai_name = None;
        c.ai_gender = None;
        c.ai_backstory = None;
        c.ai_memory = None;
        c.ai_directive = None;
        c.ai_example_message = None;
        c.ai_additional_context = None;
        c.current_scene = None;
        c.user_name = None;
        c.user_gender = None;
        c.greeting = None;
        let code = encode(&c);
        let partial = decode(&code).expect("decode");
        assert_eq!(partial, PartialCharacter::default());
    }

    #[test]
    fn unknown_version_rejected() {
        // v=2 payload
        let payload = r#"{"v":2,"p":{}}"#;
        let code = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let err = decode(&code).unwrap_err();
        matches!(err, ShareCodeError::UnsupportedVersion(2));
    }

    #[test]
    fn malformed_base64_rejected() {
        let err = decode("not!base64??").unwrap_err();
        matches!(err, ShareCodeError::Malformed(_));
    }

    #[test]
    fn malformed_json_rejected() {
        let code = URL_SAFE_NO_PAD.encode(b"not json");
        let err = decode(&code).unwrap_err();
        matches!(err, ShareCodeError::Malformed(_));
    }

    #[test]
    fn wrong_field_types_rejected() {
        // ai_name must be string-or-null, not number
        let payload = r#"{"v":1,"p":{"ai_name":42}}"#;
        let code = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let err = decode(&code).unwrap_err();
        matches!(err, ShareCodeError::Malformed(_));
    }

    #[test]
    fn legacy_v1_code_without_greeting_round_trips() {
        // A v1 code emitted before `greeting` existed has no greeting.
        let payload = r#"{"v":1,"p":{"ai_name":"Aria"}}"#;
        let code = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let partial = decode(&code).expect("decode");
        assert_eq!(partial.ai_name.as_deref(), Some("Aria"));
        assert_eq!(partial.greeting, None);

        // Re-encoding from a Character with the same fields produces the
        // same code (encoder omits nulls).
        let mut c = make_full_character();
        c.ai_gender = None;
        c.ai_backstory = None;
        c.ai_memory = None;
        c.ai_directive = None;
        c.ai_example_message = None;
        c.ai_additional_context = None;
        c.current_scene = None;
        c.user_name = None;
        c.user_gender = None;
        c.greeting = None;
        assert_eq!(encode(&c), code);
    }

    #[test]
    fn encode_is_deterministic() {
        let c = make_full_character();
        assert_eq!(encode(&c), encode(&c));
    }

    #[test]
    fn notes_are_not_in_share_code() {
        let c = make_full_character();
        let code = encode(&c);
        let partial = decode(&code).expect("decode");
        // PartialCharacter has no `notes` field at all, so decode must not
        // surface it.
        let json = serde_json::to_string(&partial).unwrap();
        assert!(!json.contains("notes"));
    }
}
