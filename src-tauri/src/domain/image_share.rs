//! Image-based share codes (ComfyUI-style).
//!
//! A share-image is a PNG (or any format the user supplies, which we
//! re-encode to PNG) with a `tEXt` chunk under the keyword `kindroid`
//! whose value is our JSON payload `{"v":1, "p":{...persona}}`.
//!
//! The decoder is a tiny PNG walker that needs no `png` crate at
//! decode time so even small malformed files produce a clean error.

use serde::{Deserialize, Serialize};

use crate::domain::character::Character;
use crate::domain::share_code::{build_partial, PartialCharacter, CURRENT_VERSION};

pub const KEYWORD: &[u8] = b"kindroid";

#[derive(Debug, Serialize, Deserialize)]
pub struct SharePayload {
    v: u32,
    p: PartialCharacter,
}

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageShareError {
    #[error("not a PNG file")]
    NotPng,
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("no kindroid metadata in image")]
    NoPayload,
    #[error("unsupported share-code version: {0}")]
    UnsupportedVersion(u32),
    #[error("image codec error: {0}")]
    Codec(String),
}

/// Extract the persona payload from a PNG that has a `kindroid` `tEXt`
/// chunk. Non-PNG inputs are rejected with `NoPayload`.
pub fn decode_image(image_bytes: &[u8]) -> Result<PartialCharacter, ImageShareError> {
    if image_bytes.len() < 8 || &image_bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(ImageShareError::NoPayload);
    }
    let text = read_kindroid_text(image_bytes)?.ok_or(ImageShareError::NoPayload)?;
    let payload: SharePayload = serde_json::from_str(&text)
        .map_err(|e| ImageShareError::Malformed(format!("json: {e}")))?;
    if payload.v != CURRENT_VERSION {
        return Err(ImageShareError::UnsupportedVersion(payload.v));
    }
    Ok(payload.p)
}

/// Encode `(character, image)` into a PNG with the persona payload
/// embedded as a `tEXt` chunk.
pub fn encode_image(image_bytes: &[u8], character: &Character) -> Result<Vec<u8>, ImageShareError> {
    let dynamic =
        image::load_from_memory(image_bytes).map_err(|e| ImageShareError::Codec(e.to_string()))?;
    let rgba = dynamic.to_rgba8();
    let (width, height) = rgba.dimensions();

    let payload = serde_json::to_string(&SharePayload {
        v: CURRENT_VERSION,
        p: build_partial(character),
    })
    .map_err(|e| ImageShareError::Malformed(format!("json: {e}")))?;

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| ImageShareError::Codec(e.to_string()))?;
        writer
            .write_image_data(rgba.as_raw())
            .map_err(|e| ImageShareError::Codec(e.to_string()))?;

        let mut chunk_data = Vec::with_capacity(KEYWORD.len() + 1 + payload.len());
        chunk_data.extend_from_slice(KEYWORD);
        chunk_data.push(0);
        chunk_data.extend_from_slice(payload.as_bytes());
        writer
            .write_chunk(png::chunk::ChunkType(*b"tEXt"), &chunk_data)
            .map_err(|e| ImageShareError::Codec(e.to_string()))?;
    }
    Ok(out)
}

/// Walk the PNG chunks looking for `tEXt` with our keyword. Returns
/// the chunk's text value (UTF-8 since our payload is ASCII JSON).
fn read_kindroid_text(png: &[u8]) -> Result<Option<String>, ImageShareError> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(ImageShareError::NotPng);
    }
    let mut pos = 8;
    while pos + 12 <= png.len() {
        let length =
            u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        pos += 4;
        let chunk_type = &png[pos..pos + 4];
        pos += 4;
        if pos + length > png.len() {
            return Err(ImageShareError::Malformed("truncated chunk".into()));
        }
        let data = &png[pos..pos + length];
        pos += length + 4; // skip data + CRC

        if chunk_type == b"tEXt" {
            let null_pos = data
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| ImageShareError::Malformed("tEXt missing null".into()))?;
            let keyword = std::str::from_utf8(&data[..null_pos])
                .map_err(|e| ImageShareError::Malformed(format!("tEXt keyword: {e}")))?;
            if keyword == "kindroid" {
                let text = std::str::from_utf8(&data[null_pos + 1..])
                    .map_err(|e| ImageShareError::Malformed(format!("tEXt text: {e}")))?;
                return Ok(Some(text.to_string()));
            }
        }
    }
    Ok(None)
}

/// Remove our `kindroid` `tEXt` chunk from a PNG. Other chunks — including
/// other `tEXt` keys (e.g. ComfyUI's `prompt` / `workflow`) — are preserved.
/// Non-PNG input is returned unchanged.
pub fn strip_kindroid_metadata(image_bytes: &[u8]) -> Vec<u8> {
    if image_bytes.len() < 8 || &image_bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return image_bytes.to_vec();
    }
    let mut out = Vec::with_capacity(image_bytes.len());
    out.extend_from_slice(&image_bytes[..8]);
    let mut pos = 8;
    while pos + 12 <= image_bytes.len() {
        let length = u32::from_be_bytes([
            image_bytes[pos],
            image_bytes[pos + 1],
            image_bytes[pos + 2],
            image_bytes[pos + 3],
        ]) as usize;
        pos += 4;
        let chunk_type = &image_bytes[pos..pos + 4];
        pos += 4;
        if pos + length > image_bytes.len() {
            // Truncated input — bail out with the original bytes.
            return image_bytes.to_vec();
        }
        let data = &image_bytes[pos..pos + length];
        let end = pos + length + 4; // past data + CRC

        let is_kindroid_text = chunk_type == b"tEXt"
            && data
                .iter()
                .position(|&b| b == 0)
                .and_then(|n| std::str::from_utf8(&data[..n]).ok())
                == Some("kindroid");

        if !is_kindroid_text {
            out.extend_from_slice(&image_bytes[pos - 8..end]);
        }
        pos = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) as usize {
            data.extend_from_slice(&color);
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
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
            ai_gender: Some("female".into()),
            ai_backstory: Some("Backstory".into()),
            ai_memory: Some("Memory".into()),
            ai_directive: None,
            ai_example_message: None,
            ai_additional_context: None,
            current_scene: None,
            user_name: Some("Eric".into()),
            user_gender: None,
            greeting: Some("Hello!".into()),
            notes: None,
            cover_image: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn png_crc(chunk_type: &[u8], data: &[u8]) -> [u8; 4] {
        let mut table = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xedb88320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            table[n as usize] = c;
        }
        let mut crc = 0xffffffffu32;
        for &b in chunk_type.iter().chain(data.iter()) {
            crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
        }
        (crc ^ 0xffffffff).to_be_bytes()
    }

    #[test]
    fn round_trip() {
        let png = make_png(4, 4, [255, 0, 0, 255]);
        let character = make_character();
        let encoded = encode_image(&png, &character).unwrap();
        let partial = decode_image(&encoded).unwrap();
        assert_eq!(partial.ai_name.as_deref(), Some("Aria"));
        assert_eq!(partial.greeting.as_deref(), Some("Hello!"));
    }

    #[test]
    fn encoded_image_contains_kindroid_chunk() {
        let png = make_png(2, 2, [0, 0, 0, 255]);
        let character = make_character();
        let encoded = encode_image(&png, &character).unwrap();
        let mut found = false;
        let mut pos = 8;
        while pos + 12 <= encoded.len() {
            let length = u32::from_be_bytes([
                encoded[pos],
                encoded[pos + 1],
                encoded[pos + 2],
                encoded[pos + 3],
            ]) as usize;
            pos += 4;
            let chunk_type = &encoded[pos..pos + 4];
            pos += 4;
            if chunk_type == b"tEXt" && pos + length <= encoded.len() {
                let data = &encoded[pos..pos + length];
                if data.starts_with(b"kindroid\0") {
                    found = true;
                    break;
                }
            }
            pos += length + 4;
        }
        assert!(found, "kindroid tEXt chunk not found");
    }

    #[test]
    fn rejects_image_without_payload() {
        let png = make_png(2, 2, [0, 0, 0, 255]);
        let err = decode_image(&png).unwrap_err();
        matches!(err, ImageShareError::NoPayload);
    }

    #[test]
    fn rejects_non_png() {
        let err = decode_image(b"not a png").unwrap_err();
        matches!(err, ImageShareError::NoPayload);
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut payload = b"kindroid\0".to_vec();
        payload.extend_from_slice(br#"{"v":99,"p":{}}"#);
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        // IHDR (1x1 RGBA)
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&13u32.to_be_bytes());
        ihdr.extend_from_slice(b"IHDR");
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(6);
        ihdr.extend_from_slice(&[0, 0, 0]);
        ihdr.extend_from_slice(&png_crc(b"IHDR", &ihdr[8..]));
        png.extend_from_slice(&ihdr);
        // tEXt
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(&payload);
        chunk.extend_from_slice(&png_crc(b"tEXt", &payload));
        png.extend_from_slice(&chunk);
        // IDAT (1x1 RGBA pixel = 0,0,0,0)
        let raw = [0u8, 0, 0, 0, 0];
        let compressed = {
            use std::io::Write;
            let mut enc =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(&raw).unwrap();
            enc.finish().unwrap()
        };
        let mut idat = Vec::new();
        idat.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        idat.extend_from_slice(b"IDAT");
        idat.extend_from_slice(&compressed);
        idat.extend_from_slice(&png_crc(b"IDAT", &compressed));
        png.extend_from_slice(&idat);
        // IEND
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&png_crc(b"IEND", &[]));

        let err = decode_image(&png).unwrap_err();
        match err {
            ImageShareError::UnsupportedVersion(99) => {}
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_rejected() {
        let mut png = make_png(1, 1, [0, 0, 0, 255]);
        let payload = b"kindroid\0not-json";
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(payload);
        chunk.extend_from_slice(&png_crc(b"tEXt", payload));
        png.extend_from_slice(&chunk);
        let err = decode_image(&png).unwrap_err();
        match err {
            ImageShareError::Malformed(_) => {}
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn encodes_jpeg_input_as_png() {
        let mut buf = Vec::new();
        {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut buf);
            let img = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
            encoder
                .encode(img.as_raw(), 8, 8, image::ExtendedColorType::Rgb8)
                .unwrap();
        }
        let character = make_character();
        let encoded = encode_image(&buf, &character).unwrap();
        let partial = decode_image(&encoded).unwrap();
        assert_eq!(partial.ai_name.as_deref(), Some("Aria"));
    }

    #[test]
    fn partial_character_field_omitted_from_image_metadata() {
        let mut character = make_character();
        character.ai_backstory = None;
        character.ai_memory = None;
        character.greeting = None;
        let png = make_png(2, 2, [0, 255, 0, 255]);
        let encoded = encode_image(&png, &character).unwrap();
        let partial = decode_image(&encoded).unwrap();
        assert_eq!(partial.ai_backstory, None);
        assert_eq!(partial.greeting, None);
    }

    #[test]
    fn strip_kindroid_removes_only_our_chunk() {
        let png = make_png(2, 2, [0, 0, 0, 255]);
        let character = make_character();
        let encoded = encode_image(&png, &character).unwrap();
        // Sanity: the encoded image contains the kindroid chunk.
        assert!(read_kindroid_text(&encoded).unwrap().is_some());

        let stripped = strip_kindroid_metadata(&encoded);
        // After strip, no kindroid chunk.
        assert!(read_kindroid_text(&stripped).unwrap().is_none());
        // The PNG signature is preserved.
        assert!(stripped.starts_with(b"\x89PNG\r\n\x1a\n"));
        // Image is still a valid PNG: the IHDR should be the first chunk.
        let ihdr_len = u32::from_be_bytes([stripped[8], stripped[9], stripped[10], stripped[11]]);
        assert_eq!(ihdr_len, 13);
        assert_eq!(&stripped[12..16], b"IHDR");
    }

    #[test]
    fn strip_kindroid_preserves_other_text_chunks() {
        let png = make_png(1, 1, [0, 0, 0, 255]);
        let character = make_character();
        let mut bytes = encode_image(&png, &character).unwrap();
        // Inject another tEXt chunk with a non-kindroid keyword.
        let payload = b"prompt\0{\"foo\":1}";
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(payload);
        chunk.extend_from_slice(&png_crc(b"tEXt", payload));
        // Insert before IEND (the chunk header's start, not the type field).
        let iend_chunk_start = find_iend(&bytes).unwrap();
        bytes.splice(iend_chunk_start..iend_chunk_start, chunk.iter().cloned());

        let stripped = strip_kindroid_metadata(&bytes);
        // The kindroid chunk is gone, but `prompt` is preserved.
        assert!(read_kindroid_text(&stripped).unwrap().is_none());
        let prompt = read_text_chunk(&stripped, "prompt").unwrap();
        assert_eq!(prompt, "{\"foo\":1}");
    }

    #[test]
    fn strip_kindroid_passes_non_png_through() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        let stripped = strip_kindroid_metadata(&jpeg);
        assert_eq!(stripped, jpeg);
    }

    #[test]
    fn strip_kindroid_passes_png_without_our_chunk_through() {
        let png = make_png(2, 2, [0, 0, 0, 255]);
        let stripped = strip_kindroid_metadata(&png);
        assert_eq!(stripped, png);
    }

    fn find_iend(png: &[u8]) -> Option<usize> {
        let mut pos = 8;
        while pos + 12 <= png.len() {
            let length =
                u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
            pos += 4;
            let chunk_type = &png[pos..pos + 4];
            pos += 4;
            if chunk_type == b"IEND" {
                // Return the start of the chunk's length field, four bytes
                // before the current `pos`.
                return Some(pos - 8);
            }
            pos += length + 4;
        }
        None
    }

    fn read_text_chunk(png: &[u8], key: &str) -> Option<String> {
        let mut pos = 8;
        while pos + 12 <= png.len() {
            let length =
                u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
            pos += 4;
            let chunk_type = &png[pos..pos + 4];
            pos += 4;
            if pos + length > png.len() {
                return None;
            }
            let data = &png[pos..pos + length];
            pos += length + 4;
            if chunk_type == b"tEXt" {
                let null_pos = data.iter().position(|&b| b == 0)?;
                let keyword = std::str::from_utf8(&data[..null_pos]).ok()?;
                if keyword == key {
                    let text = std::str::from_utf8(&data[null_pos + 1..]).ok()?;
                    return Some(text.to_string());
                }
            }
        }
        None
    }
}
