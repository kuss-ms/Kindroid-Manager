use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::push_log::PushLogEntry;
use crate::domain::target::Target;
use crate::storage::{Repository, StorageError};

pub struct SqliteRepository {
    conn: Arc<Mutex<Connection>>,
    data_dir: PathBuf,
}

impl SqliteRepository {
    /// Open or create a DB at `path`, run migrations, set pragmas.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let data_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let conn = Connection::open(&path).map_err(|e| StorageError::Database(e.to_string()))?;
        Self::init(conn, data_dir)
    }

    /// Open an in-memory DB (used for tests).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn =
            Connection::open_in_memory().map_err(|e| StorageError::Database(e.to_string()))?;
        Self::init(conn, PathBuf::new())
    }

    fn init(mut conn: Connection, data_dir: PathBuf) -> Result<Self, StorageError> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        run_migrations(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

fn detect_extension(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        "png"
    } else if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        "jpg"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.len() >= 4 && &bytes[..4] == b"GIF8" {
        "gif"
    } else {
        "bin"
    }
}

fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| StorageError::Database(e.to_string()))?;
    let migrations = discover_migrations().map_err(StorageError::Database)?;
    let tx = conn
        .transaction()
        .map_err(|e| StorageError::Database(e.to_string()))?;
    for (version, body) in migrations.iter() {
        if *version as i64 > current {
            tx.execute_batch(body)
                .map_err(|e| StorageError::Database(format!("migration {version}: {e}")))?;
        }
    }
    let target = migrations.last().map(|(v, _)| *v).unwrap_or(0);
    tx.execute(&format!("PRAGMA user_version = {target}"), [])
        .map_err(|e| StorageError::Database(e.to_string()))?;
    tx.commit()
        .map_err(|e| StorageError::Database(e.to_string()))?;
    Ok(())
}

fn discover_migrations() -> Result<Vec<(u32, String)>, String> {
    let dir = migrations_dir().ok_or("no migrations dir")?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("sql") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let version: u32 = name
            .split('_')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("bad migration name: {name}"))?;
        let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        out.push((version, body));
    }
    out.sort_by_key(|(v, _)| *v);
    Ok(out)
}

fn migrations_dir() -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR is set by cargo at build time.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = manifest.join("src").join("storage").join("migrations");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| StorageError::Database(format!("bad datetime {s}: {e}")))
}

fn id_err(idx: usize, e: impl ToString) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        idx,
        rusqlite::types::Type::Text,
        Box::new(rusqlite::types::FromSqlError::Other(e.to_string().into())),
    )
}

fn now() -> DateTime<Utc> {
    let now = std::time::SystemTime::now();
    let dt: DateTime<Utc> = now.into();
    dt
}

async fn lock<T>(m: &Mutex<T>) -> tokio::sync::MutexGuard<'_, T> {
    m.lock().await
}

#[async_trait]
impl Repository for SqliteRepository {
    async fn list_characters(&self) -> Result<Vec<Character>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, ai_name, ai_gender, ai_backstory, ai_memory, ai_directive,
                        ai_example_message, ai_additional_context, current_scene, user_name,
                        user_gender, greeting, notes, ai_avatar_description, cover_image,
                        created_at, updated_at
                 FROM characters ORDER BY updated_at DESC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_character)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn get_character(&self, id: Uuid) -> Result<Character, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT id, name, ai_name, ai_gender, ai_backstory, ai_memory, ai_directive,
                    ai_example_message, ai_additional_context, current_scene, user_name,
                    user_gender, greeting, notes, ai_avatar_description, cover_image,
                    created_at, updated_at
             FROM characters WHERE id = ?1",
            params![id.to_string()],
            row_to_character,
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))?
        .ok_or(StorageError::NotFound)
    }

    async fn upsert_character(&self, mut c: Character) -> Result<Character, StorageError> {
        let conn = lock(&self.conn).await;
        let now = now();
        let existing: Option<String> = conn
            .query_row(
                "SELECT created_at FROM characters WHERE id = ?1",
                params![c.id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if let Some(prev) = existing {
            c.created_at = parse_dt(&prev)?;
        } else {
            c.created_at = now;
        }
        c.updated_at = now;
        conn.execute(
            "INSERT INTO characters
             (id, name, ai_name, ai_gender, ai_backstory, ai_memory, ai_directive,
              ai_example_message, ai_additional_context, current_scene, user_name,
              user_gender, greeting, notes, ai_avatar_description, cover_image,
              created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, ai_name=excluded.ai_name, ai_gender=excluded.ai_gender,
               ai_backstory=excluded.ai_backstory, ai_memory=excluded.ai_memory,
               ai_directive=excluded.ai_directive, ai_example_message=excluded.ai_example_message,
               ai_additional_context=excluded.ai_additional_context,
               current_scene=excluded.current_scene, user_name=excluded.user_name,
               user_gender=excluded.user_gender, greeting=excluded.greeting,
               notes=excluded.notes, ai_avatar_description=excluded.ai_avatar_description,
               cover_image=excluded.cover_image,
               updated_at=excluded.updated_at",
            params![
                c.id.to_string(),
                c.name,
                c.ai_name,
                c.ai_gender,
                c.ai_backstory,
                c.ai_memory,
                c.ai_directive,
                c.ai_example_message,
                c.ai_additional_context,
                c.current_scene,
                c.user_name,
                c.user_gender,
                c.greeting,
                c.notes,
                c.ai_avatar_description,
                c.cover_image,
                c.created_at.to_rfc3339(),
                c.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(c)
    }

    async fn delete_character(&self, id: Uuid) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        let n = conn
            .execute(
                "DELETE FROM characters WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if n == 0 {
            return Err(StorageError::NotFound);
        }
        drop(conn);
        let _ = self.delete_character_image_bytes(id).await;
        Ok(())
    }

    async fn list_targets(&self) -> Result<Vec<Target>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt = conn
            .prepare("SELECT id, ai_id, label, created_at FROM targets ORDER BY label ASC")
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_target)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn get_target(&self, id: Uuid) -> Result<Target, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT id, ai_id, label, created_at FROM targets WHERE id = ?1",
            params![id.to_string()],
            row_to_target,
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))?
        .ok_or(StorageError::NotFound)
    }

    async fn upsert_target(&self, mut t: Target) -> Result<Target, StorageError> {
        let conn = lock(&self.conn).await;
        // If a row with the same ai_id exists, merge into it.
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM targets WHERE ai_id = ?1",
                params![t.ai_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if let Some(prev) = existing_id {
            let prev = Uuid::parse_str(&prev).map_err(|e| StorageError::Database(e.to_string()))?;
            if prev != t.id {
                // Update the existing row in place; drop the candidate id.
                conn.execute(
                    "UPDATE targets SET label = ?1 WHERE id = ?2",
                    params![t.label, prev.to_string()],
                )
                .map_err(|e| StorageError::Database(e.to_string()))?;
                t.id = prev;
            }
            let updated = t.clone();
            conn.execute(
                "UPDATE targets SET label = ?1 WHERE id = ?2",
                params![updated.label, updated.id.to_string()],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
            return Ok(updated);
        }
        t.created_at = now();
        conn.execute(
            "INSERT INTO targets (id, ai_id, label, created_at) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET ai_id=excluded.ai_id, label=excluded.label",
            params![
                t.id.to_string(),
                t.ai_id,
                t.label,
                t.created_at.to_rfc3339()
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(t)
    }

    async fn delete_target(&self, id: Uuid) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        let n = conn
            .execute("DELETE FROM targets WHERE id = ?1", params![id.to_string()])
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if n == 0 {
            return Err(StorageError::NotFound);
        }
        Ok(())
    }

    async fn append_push_log(&self, entry: PushLogEntry) -> Result<PushLogEntry, StorageError> {
        let conn = lock(&self.conn).await;
        let fields_json = serde_json::to_string(&entry.fields_sent)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO push_log
             (id, at, character_id, character_name, target_id, target_ai_id, fields_sent,
              did_chat_break, greeting, wipe_cascaded, update_info_status, update_info_body,
              chat_break_status, chat_break_body)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                entry.id.to_string(),
                entry.at.to_rfc3339(),
                entry.character_id.to_string(),
                entry.character_name,
                entry.target_id.to_string(),
                entry.target_ai_id,
                fields_json,
                entry.did_chat_break as i32,
                entry.greeting,
                entry.wipe_cascaded.map(|b| b as i32),
                entry.update_info_status as i64,
                entry.update_info_body,
                entry.chat_break_status.map(|s| s as i64),
                entry.chat_break_body,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(entry)
    }

    async fn list_push_history(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<PushLogEntry>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt = conn
            .prepare(
                "SELECT id, at, character_id, character_name, target_id, target_ai_id,
                        fields_sent, did_chat_break, greeting, wipe_cascaded,
                        update_info_status, update_info_body, chat_break_status,
                        chat_break_body
                 FROM push_log ORDER BY at DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![limit as i64, offset as i64], row_to_push_log)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn get_push_log(&self, id: Uuid) -> Result<PushLogEntry, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT id, at, character_id, character_name, target_id, target_ai_id,
                    fields_sent, did_chat_break, greeting, wipe_cascaded,
                    update_info_status, update_info_body, chat_break_status, chat_break_body
             FROM push_log WHERE id = ?1",
            params![id.to_string()],
            row_to_push_log,
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))?
        .ok_or(StorageError::NotFound)
    }

    async fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))
    }

    async fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn save_character_image_bytes(
        &self,
        character_id: Uuid,
        bytes: &[u8],
    ) -> Result<String, StorageError> {
        let ext = detect_extension(bytes);
        let dir = self.data_dir.join("images");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rel = format!("images/{character_id}.{ext}");
        let path = self.data_dir.join(&rel);
        // Remove any previously stored image files for this character before
        // writing the new one. Otherwise a stale file with a different
        // extension (e.g. the previous PNG after uploading a JPG) stays on
        // disk and would be returned by `read_character_image_bytes`
        // ahead of the newly written file.
        let _ = self.delete_character_image_bytes(character_id).await;
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        // Update the character's cover_image column so the field is
        // consistent with the file on disk.
        let conn = lock(&self.conn).await;
        let updated_at = now();
        conn.execute(
            "UPDATE characters SET cover_image = ?1, updated_at = ?2 WHERE id = ?3",
            params![rel, updated_at.to_rfc3339(), character_id.to_string()],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(rel)
    }

    async fn read_character_image_bytes(
        &self,
        character_id: Uuid,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        // Trust the `cover_image` column rather than guessing the extension
        // from filenames on disk: when a previous image with a different
        // extension was overwritten, a stale file could shadow the current
        // one if we iterated extensions in a fixed order.
        let conn = lock(&self.conn).await;
        let rel: Option<String> = conn
            .query_row(
                "SELECT cover_image FROM characters WHERE id = ?1",
                params![character_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?
            .flatten();
        drop(conn);
        let Some(rel) = rel else {
            return Ok(None);
        };
        let path = self.data_dir.join(&rel);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(Some(bytes))
    }

    async fn delete_character_image_bytes(&self, character_id: Uuid) -> Result<(), StorageError> {
        let dir = self.data_dir.join("images");
        if !dir.exists() {
            return Ok(());
        }
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&character_id.to_string()) {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }
}

fn row_to_character(row: &rusqlite::Row<'_>) -> rusqlite::Result<Character> {
    let id_s: String = row.get(0)?;
    let created_s: String = row.get(16)?;
    let updated_s: String = row.get(17)?;
    Ok(Character {
        id: Uuid::parse_str(&id_s).map_err(|e| id_err(0, e))?,
        name: row.get(1)?,
        ai_name: row.get(2)?,
        ai_gender: row.get(3)?,
        ai_backstory: row.get(4)?,
        ai_memory: row.get(5)?,
        ai_directive: row.get(6)?,
        ai_example_message: row.get(7)?,
        ai_additional_context: row.get(8)?,
        current_scene: row.get(9)?,
        user_name: row.get(10)?,
        user_gender: row.get(11)?,
        greeting: row.get(12)?,
        notes: row.get(13)?,
        ai_avatar_description: row.get(14)?,
        cover_image: row.get(15)?,
        created_at: DateTime::parse_from_rfc3339(&created_s)
            .map_err(|e| id_err(16, e))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_s)
            .map_err(|e| id_err(17, e))?
            .with_timezone(&Utc),
    })
}

fn row_to_target(row: &rusqlite::Row<'_>) -> rusqlite::Result<Target> {
    let id_s: String = row.get(0)?;
    let created_s: String = row.get(3)?;
    Ok(Target {
        id: Uuid::parse_str(&id_s).map_err(|e| id_err(0, e))?,
        ai_id: row.get(1)?,
        label: row.get(2)?,
        created_at: DateTime::parse_from_rfc3339(&created_s)
            .map_err(|e| id_err(3, e))?
            .with_timezone(&Utc),
    })
}

fn row_to_push_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<PushLogEntry> {
    let id_s: String = row.get(0)?;
    let at_s: String = row.get(1)?;
    let cid_s: String = row.get(2)?;
    let tid_s: String = row.get(4)?;
    let fields_s: String = row.get(6)?;
    let did_break: i32 = row.get(7)?;
    let wipe: Option<i32> = row.get(9)?;
    let ui_status: i64 = row.get(10)?;
    let cb_status: Option<i64> = row.get(12)?;
    Ok(PushLogEntry {
        id: Uuid::parse_str(&id_s).map_err(|e| id_err(0, e))?,
        at: DateTime::parse_from_rfc3339(&at_s)
            .map_err(|e| id_err(1, e))?
            .with_timezone(&Utc),
        character_id: Uuid::parse_str(&cid_s).map_err(|e| id_err(2, e))?,
        character_name: row.get(3)?,
        target_id: Uuid::parse_str(&tid_s).map_err(|e| id_err(4, e))?,
        target_ai_id: row.get(5)?,
        fields_sent: serde_json::from_str(&fields_s).map_err(|e| id_err(6, e))?,
        did_chat_break: did_break != 0,
        greeting: row.get(8)?,
        wipe_cascaded: wipe.map(|b| b != 0),
        update_info_status: ui_status as u16,
        update_info_body: row.get(11)?,
        chat_break_status: cb_status.map(|s| s as u16),
        chat_break_body: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::push_log::truncate_body;
    use crate::storage::Repository;

    fn character(name: &str) -> Character {
        Character {
            id: Uuid::new_v4(),
            name: name.into(),
            ai_name: Some("Aria".into()),
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

    fn target(label: &str, ai_id: &str) -> Target {
        Target {
            id: Uuid::new_v4(),
            ai_id: ai_id.into(),
            label: label.into(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn character_crud() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("Test");
        let id = c.id;
        repo.upsert_character(c.clone()).await.unwrap();
        let got = repo.get_character(id).await.unwrap();
        assert_eq!(got.name, "Test");

        let list = repo.list_characters().await.unwrap();
        assert_eq!(list.len(), 1);

        repo.delete_character(id).await.unwrap();
        let err = repo.get_character(id).await.unwrap_err();
        matches!(err, StorageError::NotFound);
    }

    #[tokio::test]
    async fn target_unique_ai_id_upsert_merges() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t1 = target("Original", "ai_123");
        let original_id = t1.id;
        repo.upsert_target(t1).await.unwrap();

        // Try to upsert a *different* row for the same ai_id with a new id.
        let t2 = Target {
            id: Uuid::new_v4(),
            ai_id: "ai_123".into(),
            label: "Renamed".into(),
            created_at: Utc::now(),
        };
        let merged = repo.upsert_target(t2).await.unwrap();
        assert_eq!(merged.id, original_id);
        assert_eq!(merged.label, "Renamed");

        let list = repo.list_targets().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "Renamed");
    }

    #[tokio::test]
    async fn push_log_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        let t = target("T", "ai_x");
        let tid = t.id;
        repo.upsert_character(c).await.unwrap();
        repo.upsert_target(t).await.unwrap();

        let entry = PushLogEntry {
            id: Uuid::new_v4(),
            at: Utc::now(),
            character_id: cid,
            character_name: "C".into(),
            target_id: tid,
            target_ai_id: "ai_x".into(),
            fields_sent: vec!["ai_name".into()],
            did_chat_break: false,
            greeting: None,
            wipe_cascaded: None,
            update_info_status: 200,
            update_info_body: "ok".into(),
            chat_break_status: None,
            chat_break_body: None,
        };
        let entry_id = entry.id;
        repo.append_push_log(entry).await.unwrap();
        let list = repo.list_push_history(10, 0).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, entry_id);
        let got = repo.get_push_log(entry_id).await.unwrap();
        assert_eq!(got.update_info_status, 200);
    }

    #[test]
    fn truncate_body_respects_byte_cap() {
        let big = "a".repeat(8000);
        let t = truncate_body(&big);
        assert!(t.starts_with("aaa"));
        assert!(t.contains("truncated"));
    }

    #[tokio::test]
    async fn settings_get_set() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        assert_eq!(repo.get_setting("k").await.unwrap(), None);
        repo.set_setting("k", "v").await.unwrap();
        assert_eq!(repo.get_setting("k").await.unwrap().as_deref(), Some("v"));
    }
}
