use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::character::Character;
use crate::domain::chat_automation::{
    AutoJournalEntry, AutoJournalEntryStatus, AutoJournalRun, AutoJournalRunStatus,
    ChatAutomationState, StableMessageCursor, SummaryBackend, SummaryBootstrapMode,
    SummaryCandidate,
};
use crate::domain::chat_message::{ChatMessage, ChatSyncState, SyncStatusKind};
use crate::domain::journal_entry::JournalEntry;
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
    // Migration SQL is embedded at compile time so the binary works on
    // platforms (notably Android) where the build-time `CARGO_MANIFEST_DIR`
    // path does not exist. To add a new migration: drop `NNNN_*.sql` into
    // `src/storage/migrations/`, then append a matching `include_str!` line
    // below in numeric order.
    let bodies: &[(&str, &str)] = &[
        ("0001_init.sql", include_str!("migrations/0001_init.sql")),
        (
            "0002_add_cover_image.sql",
            include_str!("migrations/0002_add_cover_image.sql"),
        ),
        (
            "0003_add_avatar_description.sql",
            include_str!("migrations/0003_add_avatar_description.sql"),
        ),
        (
            "0004_chat_history.sql",
            include_str!("migrations/0004_chat_history.sql"),
        ),
        (
            "0005_chat_favourite.sql",
            include_str!("migrations/0005_chat_favourite.sql"),
        ),
        (
            "0006_character_journal.sql",
            include_str!("migrations/0006_character_journal.sql"),
        ),
        (
            "0007_chat_automation.sql",
            include_str!("migrations/0007_chat_automation.sql"),
        ),
        (
            "0008_chat_automation_response.sql",
            include_str!("migrations/0008_chat_automation_response.sql"),
        ),
        (
            "0009_push_log_create_new_ai.sql",
            include_str!("migrations/0009_push_log_create_new_ai.sql"),
        ),
        (
            "0010_chat_favourite_index.sql",
            include_str!("migrations/0010_chat_favourite_index.sql"),
        ),
        (
            "0011_drop_automation_responses.sql",
            include_str!("migrations/0011_drop_automation_responses.sql"),
        ),
        (
            "0012_drop_sender_type.sql",
            include_str!("migrations/0012_drop_sender_type.sql"),
        ),
    ];
    let mut out = Vec::new();
    for (name, body) in bodies {
        let version: u32 = name
            .split('_')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("bad migration name: {name}"))?;
        out.push((version, (*body).to_string()));
    }
    out.sort_by_key(|(v, _)| *v);
    Ok(out)
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
        let journal_ids_json: Option<String> = match &entry.journal_entry_ids {
            Some(ids) => serde_json::to_string(ids)
                .map(Some)
                .map_err(|e| StorageError::Database(e.to_string()))?,
            None => None,
        };
        conn.execute(
            "INSERT INTO push_log
             (id, at, character_id, character_name, target_id, target_ai_id, fields_sent,
              did_chat_break, greeting, wipe_cascaded, update_info_status, update_info_body,
              chat_break_status, chat_break_body, journal_entry_ids,
              create_new_ai_status, create_new_ai_body)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
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
                journal_ids_json,
                entry.create_new_ai_status.map(|s| s as i64),
                entry.create_new_ai_body,
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
                        chat_break_body, journal_entry_ids,
                        create_new_ai_status, create_new_ai_body
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
                    update_info_status, update_info_body, chat_break_status,
                    chat_break_body, journal_entry_ids,
                    create_new_ai_status, create_new_ai_body
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
        // Write the new bytes to a sibling `*.tmp` first, then rename
        // atomically over the final path. Atomic rename is the POSIX /
        // NTFS guarantee that an interrupted `write` never leaves a
        // half-written file at the canonical path — either the new bytes
        // are fully visible, or the previous file is unchanged. The temp
        // name is unique per call so two concurrent uploads for the same
        // character do not race the rename target.
        let tmp = self.data_dir.join(format!(
            "images/{character_id}.{ext}.{}.tmp",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if let Err(e) = tokio::fs::rename(&tmp, &path).await {
            // Best-effort cleanup of the orphan temp; ignore the error
            // because the rename failure is the one we have to surface.
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(StorageError::Database(format!(
                "atomic image rename failed: {e}"
            )));
        }
        // After the rename succeeds the canonical file on disk matches
        // the new bytes, so it's safe to update the DB column and then
        // delete any leftover files for this character (e.g. a previous
        // upload with a different extension). Hold the DB mutex across
        // the file+UPDATE sequence to serialise concurrent uploads for
        // the same character.
        let conn = lock(&self.conn).await;
        let updated_at = now();
        conn.execute(
            "UPDATE characters SET cover_image = ?1, updated_at = ?2 WHERE id = ?3",
            params![rel, updated_at.to_rfc3339(), character_id.to_string()],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        drop(conn);
        // Best-effort stale-file cleanup, scoped to files OTHER than the
        // one we just wrote (the delete helper matches by character id
        // prefix, so it would otherwise remove the new file too).
        if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(&character_id.to_string())
                    && name != format!("{character_id}.{ext}")
                {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
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
        // Surface the FIRST delete error rather than silently swallowing
        // it (audit M10). A stale file left behind because the OS
        // refused the unlink (Windows file lock, perms) used to be
        // invisible to the caller, which then wrote the new bytes over
        // the canonical path and left the stale file shadowing them.
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&character_id.to_string()) {
                if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                    return Err(StorageError::Database(format!(
                        "failed to remove stale image file {}: {e}",
                        entry.path().display()
                    )));
                }
            }
        }
        Ok(())
    }

    async fn upsert_chat_messages(
        &self,
        ai_id: &str,
        msgs: &[ChatMessage],
    ) -> Result<usize, StorageError> {
        if msgs.is_empty() {
            return Ok(0);
        }
        let conn = lock(&self.conn).await;
        let mut touched = 0usize;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        for m in msgs {
            let image_urls_json = serde_json::to_string(&m.image_urls)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            // Use `ON CONFLICT DO UPDATE` so an edited message (same
            // `kindroid_msg_id`, same `timestamp`, but updated content
            // fields) overwrites the existing row in place. The local
            // `id` (UUID) and `fetched_at` (only meaningful as a "first
            // seen" timestamp) are preserved so FTS5 rowids stay stable.
            //
            // The WHERE clause compares every mutable content field with
            // SQLite's NULL-safe `IS NOT` operator, so a no-op upsert
            // (identical re-fetch) is skipped and doesn't trigger an FTS5
            // delete+insert churn. The execute() return value therefore
            // counts only inserts + rows whose content actually changed.
            let n = tx
                .execute(
                    "INSERT INTO chat_messages
                       (id, ai_id, kindroid_msg_id, sender, display_name,
                        timestamp, message, image_urls, image_description, video_description,
                        internet_response, link_url, link_description, fetched_at, favourite)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
                     ON CONFLICT(ai_id, kindroid_msg_id) DO UPDATE SET
                       display_name      = excluded.display_name,
                       message           = excluded.message,
                       image_urls        = excluded.image_urls,
                       image_description = excluded.image_description,
                       video_description = excluded.video_description,
                       internet_response = excluded.internet_response,
                       link_url          = excluded.link_url,
                       link_description  = excluded.link_description
                     WHERE chat_messages.display_name      IS NOT excluded.display_name
                        OR chat_messages.message           IS NOT excluded.message
                        OR chat_messages.image_urls        IS NOT excluded.image_urls
                        OR chat_messages.image_description IS NOT excluded.image_description
                        OR chat_messages.video_description IS NOT excluded.video_description
                        OR chat_messages.internet_response IS NOT excluded.internet_response
                        OR chat_messages.link_url          IS NOT excluded.link_url
                        OR chat_messages.link_description  IS NOT excluded.link_description",
                    params![
                        m.id.to_string(),
                        ai_id,
                        m.kindroid_msg_id,
                        m.sender,
                        m.display_name,
                        m.timestamp,
                        m.message,
                        image_urls_json,
                        m.image_description,
                        m.video_description,
                        m.internet_response,
                        m.link_url,
                        m.link_description,
                        m.fetched_at.to_rfc3339(),
                        m.favourite as i32,
                    ],
                )
                .map_err(|e| StorageError::Database(e.to_string()))?;
            touched += n;
        }
        tx.commit()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(touched)
    }

    async fn list_chat_messages(
        &self,
        ai_id: &str,
        before_ts: Option<i64>,
        limit: u32,
        favourites_only: bool,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        let conn = lock(&self.conn).await;
        let limit = limit.clamp(1, 500) as i64;
        let fav_filter = if favourites_only {
            " AND favourite = 1"
        } else {
            ""
        };
        let sql = match before_ts {
            Some(_) => format!(
                "SELECT id, ai_id, kindroid_msg_id, sender, display_name,
                        timestamp, message, image_urls, image_description, video_description,
                        internet_response, link_url, link_description, fetched_at, favourite
                 FROM chat_messages
                 WHERE ai_id = ?1 AND timestamp < ?2{fav_filter}
                 ORDER BY timestamp DESC LIMIT ?3"
            ),
            None => format!(
                "SELECT id, ai_id, kindroid_msg_id, sender, display_name,
                        timestamp, message, image_urls, image_description, video_description,
                        internet_response, link_url, link_description, fetched_at, favourite
                 FROM chat_messages
                 WHERE ai_id = ?1{fav_filter}
                 ORDER BY timestamp DESC LIMIT ?2"
            ),
        };
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = match before_ts {
            Some(ts) => stmt
                .query_map(params![ai_id, ts, limit], row_to_chat_message)
                .map_err(|e| StorageError::Database(e.to_string()))?,
            None => stmt
                .query_map(params![ai_id, limit], row_to_chat_message)
                .map_err(|e| StorageError::Database(e.to_string()))?,
        };
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn search_chat(
        &self,
        ai_id: &str,
        query: &str,
        limit: u32,
        offset: u32,
        favourites_only: bool,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        let limit = limit.clamp(1, 500) as i64;
        let offset = offset as i64;
        let conn = lock(&self.conn).await;
        let fav_filter = if favourites_only {
            " AND cm.favourite = 1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT cm.id, cm.ai_id, cm.kindroid_msg_id, cm.sender,
                    cm.display_name, cm.timestamp, cm.message, cm.image_urls,
                    cm.image_description, cm.video_description, cm.internet_response,
                    cm.link_url, cm.link_description, cm.fetched_at, cm.favourite
             FROM chat_messages_fts
             JOIN chat_messages cm ON cm.rowid = chat_messages_fts.rowid
             WHERE chat_messages_fts MATCH ?1 AND cm.ai_id = ?2{fav_filter}
             ORDER BY rank
             LIMIT ?3 OFFSET ?4"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![query, ai_id, limit, offset], row_to_chat_message)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn set_chat_message_favourite(
        &self,
        ai_id: &str,
        kindroid_msg_id: &str,
        favourite: bool,
    ) -> Result<bool, StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute(
            "UPDATE chat_messages SET favourite = ?1
             WHERE ai_id = ?2 AND kindroid_msg_id = ?3",
            params![favourite as i32, ai_id, kindroid_msg_id],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        let current: Option<i32> = conn
            .query_row(
                "SELECT favourite FROM chat_messages
                 WHERE ai_id = ?1 AND kindroid_msg_id = ?2",
                params![ai_id, kindroid_msg_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(current.unwrap_or(0) != 0)
    }

    async fn chat_message_count(&self, ai_id: &str) -> Result<u64, StorageError> {
        let conn = lock(&self.conn).await;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chat_messages WHERE ai_id = ?1",
                params![ai_id],
                |r| r.get(0),
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(n.max(0) as u64)
    }

    async fn get_chat_sync_state(
        &self,
        ai_id: &str,
    ) -> Result<Option<ChatSyncState>, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT ai_id, last_synced_at, last_timestamp, full_sync_done, is_syncing,
                    status_kind, status_message, backoff_until, total
             FROM chat_sync_state WHERE ai_id = ?1",
            params![ai_id],
            row_to_chat_sync_state,
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))
    }

    async fn upsert_chat_sync_state(&self, state: &ChatSyncState) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute(
            "INSERT INTO chat_sync_state
               (ai_id, last_synced_at, last_timestamp, full_sync_done, is_syncing,
                status_kind, status_message, backoff_until, total)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(ai_id) DO UPDATE SET
               last_synced_at=excluded.last_synced_at,
               last_timestamp=excluded.last_timestamp,
               full_sync_done=excluded.full_sync_done,
               is_syncing=excluded.is_syncing,
               status_kind=excluded.status_kind,
               status_message=excluded.status_message,
               backoff_until=excluded.backoff_until,
               total=excluded.total",
            params![
                state.ai_id,
                state.last_synced_at.to_rfc3339(),
                state.last_timestamp,
                state.full_sync_done as i32,
                state.is_syncing as i32,
                state.status_kind.as_str(),
                state.status_message,
                state.backoff_until.map(|d| d.to_rfc3339()),
                state.total as i64,
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn reset_chat_history(&self, ai_id: &str) -> Result<usize, StorageError> {
        let conn = lock(&self.conn).await;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        // Delete messages first. The `chat_messages_ad` trigger on
        // `chat_messages` removes the matching FTS5 rows automatically.
        let deleted = tx
            .execute("DELETE FROM chat_messages WHERE ai_id = ?1", params![ai_id])
            .map_err(|e| StorageError::Database(e.to_string()))?;
        tx.execute(
            "DELETE FROM chat_sync_state WHERE ai_id = ?1",
            params![ai_id],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        tx.commit()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(deleted)
    }

    async fn delete_missing_chat_messages(
        &self,
        ai_id: &str,
        start_after: i64,
        last_timestamp_inclusive: i64,
        keep_ids: &[&str],
    ) -> Result<usize, StorageError> {
        if start_after >= last_timestamp_inclusive {
            // Empty range — nothing to do.
            return Ok(0);
        }
        let conn = lock(&self.conn).await;
        let mut sql = String::from(
            "DELETE FROM chat_messages WHERE ai_id = ?1 AND timestamp > ?2 AND timestamp <= ?3",
        );
        if !keep_ids.is_empty() {
            sql.push_str(" AND kindroid_msg_id NOT IN (");
            let placeholders: Vec<String> =
                (4..=keep_ids.len() + 3).map(|i| format!("?{i}")).collect();
            sql.push_str(&placeholders.join(","));
            sql.push(')');
        }
        let mut params: Vec<&dyn rusqlite::ToSql> =
            vec![&ai_id, &start_after, &last_timestamp_inclusive];
        params.extend(keep_ids.iter().map(|s| s as &dyn rusqlite::ToSql));
        let n = conn
            .execute(&sql, params.as_slice())
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(n)
    }

    async fn list_stable_chat_messages(
        &self,
        ai_id: &str,
        after_cursor: Option<&StableMessageCursor>,
        limit: u32,
        exclude_newest_n: u32,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        let conn = lock(&self.conn).await;
        let (ts, mut id) = after_cursor
            .map(|c| (c.timestamp, c.kindroid_msg_id.as_str()))
            .unwrap_or((i64::MIN, ""));
        if let Some(cursor) = after_cursor {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM chat_messages WHERE ai_id = ?1 AND timestamp = ?2 AND kindroid_msg_id = ?3",
                    params![ai_id, cursor.timestamp, cursor.kindroid_msg_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| StorageError::Database(e.to_string()))?;
            if exists.is_none() {
                id = "";
            }
        }
        let mut stmt = conn
            .prepare(
             "WITH boundary AS (
                SELECT timestamp, kindroid_msg_id FROM chat_messages
                WHERE ai_id = ?1
                ORDER BY timestamp DESC, kindroid_msg_id DESC
                LIMIT 1 OFFSET ?4
              )
              SELECT m.id, m.ai_id, m.kindroid_msg_id, m.sender, m.display_name, m.timestamp,
                     m.message, m.image_urls, m.image_description, m.video_description, m.internet_response,
                     m.link_url, m.link_description, m.fetched_at, m.favourite
              FROM chat_messages m CROSS JOIN boundary b
              WHERE m.ai_id = ?1 AND (m.timestamp > ?2 OR (m.timestamp = ?2 AND m.kindroid_msg_id > ?3))
                AND (m.timestamp < b.timestamp OR (m.timestamp = b.timestamp AND m.kindroid_msg_id <= b.kindroid_msg_id))
              ORDER BY m.timestamp ASC, m.kindroid_msg_id ASC LIMIT ?5",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(
                params![
                    ai_id,
                    ts,
                    id,
                    exclude_newest_n as i64,
                    limit.clamp(1, 10000) as i64
                ],
                row_to_chat_message,
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn latest_stable_cursor(
        &self,
        ai_id: &str,
        exclude_newest_n: u32,
    ) -> Result<Option<StableMessageCursor>, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT timestamp, kindroid_msg_id FROM chat_messages WHERE ai_id = ?1
             ORDER BY timestamp DESC, kindroid_msg_id DESC LIMIT 1 OFFSET ?2",
            params![ai_id, exclude_newest_n as i64],
            |r| {
                Ok(StableMessageCursor {
                    timestamp: r.get(0)?,
                    kindroid_msg_id: r.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| StorageError::Database(e.to_string()))
    }

    async fn get_chat_automation_state(
        &self,
        ai_id: &str,
    ) -> Result<ChatAutomationState, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row(
            "SELECT ai_id, auto_journal_enabled, auto_summary_enabled, interval, journal_cap,
             summary_backend, bootstrap_mode, journal_instructions_override, summary_instructions_override,
             journal_cursor_timestamp, journal_cursor_msg_id, summary_cursor_timestamp, summary_cursor_msg_id,
             journal_initialised, summary, summary_backend_stored, pending_summary_candidate,
             pending_summary_backend, pending_summary_created_at, pending_summary_cursor_timestamp,
             pending_summary_cursor_msg_id, pending_reformat, journal_last_error, summary_last_error,
             journal_last_run_at, summary_last_run_at
             FROM chat_automation_state WHERE ai_id = ?1",
            params![ai_id], row_to_chat_automation_state,
        ).optional().map_err(|e| StorageError::Database(e.to_string()))?.ok_or(StorageError::NotFound)
    }

    async fn upsert_chat_automation_state(
        &self,
        s: &ChatAutomationState,
    ) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute(
            "INSERT INTO chat_automation_state VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)
             ON CONFLICT(ai_id) DO UPDATE SET auto_journal_enabled=excluded.auto_journal_enabled,
             auto_summary_enabled=excluded.auto_summary_enabled, interval=excluded.interval, journal_cap=excluded.journal_cap,
             summary_backend=excluded.summary_backend, bootstrap_mode=excluded.bootstrap_mode,
             journal_instructions_override=excluded.journal_instructions_override, summary_instructions_override=excluded.summary_instructions_override,
             journal_cursor_timestamp=excluded.journal_cursor_timestamp, journal_cursor_msg_id=excluded.journal_cursor_msg_id,
             summary_cursor_timestamp=excluded.summary_cursor_timestamp, summary_cursor_msg_id=excluded.summary_cursor_msg_id,
             journal_initialised=excluded.journal_initialised, summary=excluded.summary, summary_backend_stored=excluded.summary_backend_stored,
             pending_summary_candidate=excluded.pending_summary_candidate, pending_summary_backend=excluded.pending_summary_backend,
             pending_summary_created_at=excluded.pending_summary_created_at,
             pending_summary_cursor_timestamp=excluded.pending_summary_cursor_timestamp,
             pending_summary_cursor_msg_id=excluded.pending_summary_cursor_msg_id,
             pending_reformat=excluded.pending_reformat, journal_last_error=excluded.journal_last_error,
             summary_last_error=excluded.summary_last_error, journal_last_run_at=excluded.journal_last_run_at,
             summary_last_run_at=excluded.summary_last_run_at",
            params![s.ai_id, s.auto_journal_enabled as i32, s.auto_summary_enabled as i32, s.interval, s.journal_cap,
                s.summary_backend.as_str(), s.bootstrap_mode.as_str(), s.journal_instructions_override, s.summary_instructions_override,
                s.journal_cursor.as_ref().map(|c| c.timestamp), s.journal_cursor.as_ref().map(|c| c.kindroid_msg_id.as_str()),
                s.summary_cursor.as_ref().map(|c| c.timestamp), s.summary_cursor.as_ref().map(|c| c.kindroid_msg_id.as_str()),
                s.journal_initialised as i32, s.summary, s.summary_backend_stored.as_str(), s.pending_summary_candidate,
                s.pending_summary_backend.as_ref().map(SummaryBackend::as_str), s.pending_summary_created_at.map(|d| d.to_rfc3339()),
                s.pending_summary_cursor.as_ref().map(|c| c.timestamp), s.pending_summary_cursor.as_ref().map(|c| c.kindroid_msg_id.as_str()),
                s.pending_reformat as i32, s.journal_last_error, s.summary_last_error,
                s.journal_last_run_at.map(|d| d.to_rfc3339()), s.summary_last_run_at.map(|d| d.to_rfc3339())]
        ).map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn create_auto_journal_run(&self, run: &AutoJournalRun) -> Result<(), StorageError> {
        self.update_auto_journal_run(run).await
    }
    async fn get_auto_journal_run(&self, id: &str) -> Result<AutoJournalRun, StorageError> {
        let conn = lock(&self.conn).await;
        conn.query_row("SELECT id, ai_id, start_cursor_timestamp, start_cursor_msg_id, end_cursor_timestamp, end_cursor_msg_id, status, attempts, completed_at, last_error, created_at FROM auto_journal_runs WHERE id=?1", params![id], row_to_auto_journal_run)
            .optional().map_err(|e| StorageError::Database(e.to_string()))?.ok_or(StorageError::NotFound)
    }
    async fn list_pending_auto_journal_runs(
        &self,
        ai_id: &str,
    ) -> Result<Vec<AutoJournalRun>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt = conn.prepare("SELECT id, ai_id, start_cursor_timestamp, start_cursor_msg_id, end_cursor_timestamp, end_cursor_msg_id, status, attempts, completed_at, last_error, created_at FROM auto_journal_runs WHERE ai_id=?1 AND status IN ('pending','running','failed') ORDER BY created_at ASC").map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![ai_id], row_to_auto_journal_run)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }
    async fn update_auto_journal_run(&self, r: &AutoJournalRun) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute("INSERT INTO auto_journal_runs VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET status=excluded.status, attempts=excluded.attempts, completed_at=excluded.completed_at, last_error=excluded.last_error",
            params![r.id,r.ai_id,r.start_cursor.as_ref().map(|c|c.timestamp),r.start_cursor.as_ref().map(|c|c.kindroid_msg_id.as_str()),r.end_cursor.as_ref().map(|c|c.timestamp),r.end_cursor.as_ref().map(|c|c.kindroid_msg_id.as_str()),run_status_str(r.status),r.attempts,r.completed_at.map(|d|d.to_rfc3339()),r.last_error,r.created_at.to_rfc3339()]).map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }
    async fn delete_auto_journal_run(&self, run_id: &str) -> Result<(), StorageError> {
        // The entries FK CASCADE handles the children.
        let conn = lock(&self.conn).await;
        conn.execute(
            "DELETE FROM auto_journal_runs WHERE id = ?1",
            params![run_id],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }
    async fn create_auto_journal_entry(&self, e: &AutoJournalEntry) -> Result<(), StorageError> {
        self.update_auto_journal_entry(e).await
    }
    async fn list_auto_journal_entries(
        &self,
        run_id: &str,
    ) -> Result<Vec<AutoJournalEntry>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt=conn.prepare("SELECT id,run_id,ai_id,entry,keyphrases,source_start_timestamp,source_start_msg_id,source_end_timestamp,source_end_msg_id,status,response_status,response_message,created_at,updated_at FROM auto_journal_entries WHERE run_id=?1 ORDER BY created_at ASC").map_err(|e|StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![run_id], row_to_auto_journal_entry)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }
    async fn update_auto_journal_entry(&self, e: &AutoJournalEntry) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        let kp = serde_json::to_string(&e.keyphrases)
            .map_err(|x| StorageError::Database(x.to_string()))?;
        conn.execute("INSERT INTO auto_journal_entries VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO UPDATE SET status=excluded.status,response_status=excluded.response_status,response_message=excluded.response_message,updated_at=excluded.updated_at",params![e.id,e.run_id,e.ai_id,e.entry,kp,e.source_start.as_ref().map(|c|c.timestamp),e.source_start.as_ref().map(|c|c.kindroid_msg_id.as_str()),e.source_end.as_ref().map(|c|c.timestamp),e.source_end.as_ref().map(|c|c.kindroid_msg_id.as_str()),entry_status_str(e.status),e.response_status,e.response_message,e.created_at.to_rfc3339(),e.updated_at.to_rfc3339()]).map_err(|x|StorageError::Database(x.to_string()))?;
        Ok(())
    }
    async fn commit_summary_candidate(
        &self,
        ai_id: &str,
        c: &SummaryCandidate,
        cursor: Option<&StableMessageCursor>,
    ) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute("UPDATE chat_automation_state SET summary=?2,summary_backend_stored=?3,summary_cursor_timestamp=?4,summary_cursor_msg_id=?5,pending_summary_candidate=NULL,pending_summary_backend=NULL,pending_summary_created_at=NULL,pending_reformat=0 WHERE ai_id=?1",params![ai_id,c.text,c.backend.as_str(),cursor.map(|x|x.timestamp),cursor.map(|x|x.kindroid_msg_id.as_str())]).map_err(|e|StorageError::Database(e.to_string()))?;
        Ok(())
    }
    async fn clear_summary_candidate(&self, ai_id: &str) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute("UPDATE chat_automation_state SET pending_summary_candidate=NULL,pending_summary_backend=NULL,pending_summary_created_at=NULL WHERE ai_id=?1",params![ai_id]).map_err(|e|StorageError::Database(e.to_string()))?;
        Ok(())
    }
    async fn reset_chat_summary(&self, ai_id: &str) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        conn.execute("UPDATE chat_automation_state SET summary=NULL,summary_cursor_timestamp=NULL,summary_cursor_msg_id=NULL,pending_summary_candidate=NULL,pending_summary_backend=NULL,pending_summary_created_at=NULL,pending_reformat=0,summary_last_error=NULL,summary_last_run_at=NULL WHERE ai_id=?1",params![ai_id]).map_err(|e|StorageError::Database(e.to_string()))?;
        Ok(())
    }
    async fn list_recent_successful_auto_journal_entries(
        &self,
        ai_id: &str,
        limit: u32,
    ) -> Result<Vec<AutoJournalEntry>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt=conn.prepare("SELECT id,run_id,ai_id,entry,keyphrases,source_start_timestamp,source_start_msg_id,source_end_timestamp,source_end_msg_id,status,response_status,response_message,created_at,updated_at FROM auto_journal_entries WHERE ai_id=?1 AND status='sent' ORDER BY updated_at DESC LIMIT ?2").map_err(|e|StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(
                params![ai_id, limit.clamp(1, 1000)],
                row_to_auto_journal_entry,
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn list_journal_entries(
        &self,
        character_id: Uuid,
    ) -> Result<Vec<JournalEntry>, StorageError> {
        let conn = lock(&self.conn).await;
        let mut stmt = conn
            .prepare(
                "SELECT id, character_id, entry, keyphrases, created_at, updated_at
                 FROM character_journal_entries
                 WHERE character_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![character_id.to_string()], row_to_journal_entry)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        rows.map(|r| r.map_err(|e| StorageError::Database(e.to_string())))
            .collect()
    }

    async fn upsert_journal_entry(&self, entry: &JournalEntry) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        let keyphrases_json =
            serde_json::to_string(&entry.keyphrases).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO character_journal_entries
               (id, character_id, entry, keyphrases, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET
               entry = excluded.entry,
               keyphrases = excluded.keyphrases,
               updated_at = excluded.updated_at",
            params![
                entry.id,
                entry.character_id.to_string(),
                entry.entry,
                keyphrases_json,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete_journal_entry(
        &self,
        character_id: Uuid,
        entry_id: &str,
    ) -> Result<(), StorageError> {
        let conn = lock(&self.conn).await;
        let n = conn
            .execute(
                "DELETE FROM character_journal_entries
                 WHERE id = ?1 AND character_id = ?2",
                params![entry_id, character_id.to_string()],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;
        if n == 0 {
            return Err(StorageError::NotFound);
        }
        Ok(())
    }
}

fn parse_optional_dt(
    value: Option<String>,
    index: usize,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| id_err(index, e))
        })
        .transpose()
}

fn cursor(timestamp: Option<i64>, id: Option<String>) -> Option<StableMessageCursor> {
    timestamp.map(|timestamp| StableMessageCursor {
        timestamp,
        kindroid_msg_id: id.unwrap_or_default(),
    })
}

fn row_to_chat_automation_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatAutomationState> {
    Ok(ChatAutomationState {
        ai_id: row.get(0)?,
        auto_journal_enabled: row.get::<_, i32>(1)? != 0,
        auto_summary_enabled: row.get::<_, i32>(2)? != 0,
        interval: row.get(3)?,
        journal_cap: row.get(4)?,
        summary_backend: SummaryBackend::parse(&row.get::<_, String>(5)?),
        bootstrap_mode: SummaryBootstrapMode::parse(&row.get::<_, String>(6)?),
        journal_instructions_override: row.get(7)?,
        summary_instructions_override: row.get(8)?,
        journal_cursor: cursor(row.get(9)?, row.get(10)?),
        summary_cursor: cursor(row.get(11)?, row.get(12)?),
        journal_initialised: row.get::<_, i32>(13)? != 0,
        summary: row.get(14)?,
        summary_backend_stored: SummaryBackend::parse(&row.get::<_, String>(15)?),
        pending_summary_candidate: row.get(16)?,
        pending_summary_backend: row
            .get::<_, Option<String>>(17)?
            .map(|s| SummaryBackend::parse(&s)),
        pending_summary_created_at: parse_optional_dt(row.get(18)?, 18)?,
        pending_summary_cursor: cursor(row.get(19)?, row.get(20)?),
        pending_reformat: row.get::<_, i32>(21)? != 0,
        journal_last_error: row.get(22)?,
        summary_last_error: row.get(23)?,
        journal_last_run_at: parse_optional_dt(row.get(24)?, 24)?,
        summary_last_run_at: parse_optional_dt(row.get(25)?, 25)?,
    })
}
fn run_status_str(s: AutoJournalRunStatus) -> &'static str {
    match s {
        AutoJournalRunStatus::Pending => "pending",
        AutoJournalRunStatus::Running => "running",
        AutoJournalRunStatus::Completed => "completed",
        AutoJournalRunStatus::Failed => "failed",
    }
}
fn parse_run_status(s: &str) -> AutoJournalRunStatus {
    match s {
        "running" => AutoJournalRunStatus::Running,
        "completed" => AutoJournalRunStatus::Completed,
        "failed" => AutoJournalRunStatus::Failed,
        _ => AutoJournalRunStatus::Pending,
    }
}
fn entry_status_str(s: AutoJournalEntryStatus) -> &'static str {
    match s {
        AutoJournalEntryStatus::Pending => "pending",
        AutoJournalEntryStatus::Sent => "sent",
        AutoJournalEntryStatus::Error => "error",
    }
}
fn parse_entry_status(s: &str) -> AutoJournalEntryStatus {
    match s {
        "sent" => AutoJournalEntryStatus::Sent,
        "error" => AutoJournalEntryStatus::Error,
        _ => AutoJournalEntryStatus::Pending,
    }
}
fn row_to_auto_journal_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutoJournalRun> {
    let completed: Option<String> = r.get(8)?;
    let created: String = r.get(10)?;
    Ok(AutoJournalRun {
        id: r.get(0)?,
        ai_id: r.get(1)?,
        start_cursor: cursor(r.get(2)?, r.get(3)?),
        end_cursor: cursor(r.get(4)?, r.get(5)?),
        status: parse_run_status(&r.get::<_, String>(6)?),
        attempts: r.get(7)?,
        completed_at: parse_optional_dt(completed, 8)?,
        last_error: r.get(9)?,
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(|e| id_err(10, e))?
            .with_timezone(&Utc),
    })
}
fn row_to_auto_journal_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutoJournalEntry> {
    let kp: String = r.get(4)?;
    let created: String = r.get(12)?;
    let updated: String = r.get(13)?;
    Ok(AutoJournalEntry {
        id: r.get(0)?,
        run_id: r.get(1)?,
        ai_id: r.get(2)?,
        entry: r.get(3)?,
        keyphrases: serde_json::from_str(&kp).map_err(|e| id_err(4, e))?,
        source_start: cursor(r.get(5)?, r.get(6)?),
        source_end: cursor(r.get(7)?, r.get(8)?),
        status: parse_entry_status(&r.get::<_, String>(9)?),
        response_status: r.get(10)?,
        response_message: r.get(11)?,
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(|e| id_err(12, e))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated)
            .map_err(|e| id_err(13, e))?
            .with_timezone(&Utc),
    })
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
    let journal_ids_json: Option<String> = row.get(14)?;
    let create_new_ai_status: Option<i64> = row.get(15)?;
    let journal_entry_ids = match journal_ids_json {
        Some(s) if !s.is_empty() => Some(serde_json::from_str(&s).map_err(|e| id_err(14, e))?),
        _ => None,
    };
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
        create_new_ai_status: create_new_ai_status.map(|s| s as u16),
        create_new_ai_body: row.get(16)?,
        chat_break_status: cb_status.map(|s| s as u16),
        chat_break_body: row.get(13)?,
        journal_entry_ids,
    })
}

fn row_to_chat_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    let id_s: String = row.get(0)?;
    let image_urls_s: String = row.get(7)?;
    let fetched_s: String = row.get(13)?;
    let fav: i32 = row.get(14)?;
    Ok(ChatMessage {
        id: Uuid::parse_str(&id_s).map_err(|e| id_err(0, e))?,
        ai_id: row.get(1)?,
        kindroid_msg_id: row.get(2)?,
        sender: row.get(3)?,
        display_name: row.get(4)?,
        timestamp: row.get(5)?,
        message: row.get(6)?,
        image_urls: serde_json::from_str(&image_urls_s).map_err(|e| id_err(7, e))?,
        image_description: row.get(8)?,
        video_description: row.get(9)?,
        internet_response: row.get(10)?,
        link_url: row.get(11)?,
        link_description: row.get(12)?,
        fetched_at: DateTime::parse_from_rfc3339(&fetched_s)
            .map_err(|e| id_err(13, e))?
            .with_timezone(&Utc),
        favourite: fav != 0,
    })
}

fn row_to_journal_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<JournalEntry> {
    let id: String = row.get(0)?;
    let cid_s: String = row.get(1)?;
    let entry: String = row.get(2)?;
    let keyphrases_s: String = row.get(3)?;
    let created_s: String = row.get(4)?;
    let updated_s: String = row.get(5)?;
    let keyphrases = if keyphrases_s.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&keyphrases_s).map_err(|e| id_err(3, e))?
    };
    Ok(JournalEntry {
        id,
        character_id: Uuid::parse_str(&cid_s).map_err(|e| id_err(1, e))?,
        entry,
        keyphrases,
        created_at: DateTime::parse_from_rfc3339(&created_s)
            .map_err(|e| id_err(4, e))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_s)
            .map_err(|e| id_err(5, e))?
            .with_timezone(&Utc),
    })
}

fn row_to_chat_sync_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSyncState> {
    let ai_id: String = row.get(0)?;
    let last_synced_s: String = row.get(1)?;
    let last_timestamp: i64 = row.get(2)?;
    let full_done: i32 = row.get(3)?;
    let is_syncing: i32 = row.get(4)?;
    let status_kind_s: String = row.get(5)?;
    let status_message: Option<String> = row.get(6)?;
    let backoff_s: Option<String> = row.get(7)?;
    let total: i64 = row.get(8)?;
    Ok(ChatSyncState {
        ai_id,
        last_synced_at: DateTime::parse_from_rfc3339(&last_synced_s)
            .map_err(|e| id_err(1, e))?
            .with_timezone(&Utc),
        last_timestamp,
        full_sync_done: full_done != 0,
        is_syncing: is_syncing != 0,
        status_kind: SyncStatusKind::parse(&status_kind_s),
        status_message,
        backoff_until: match backoff_s {
            Some(s) => Some(
                DateTime::parse_from_rfc3339(&s)
                    .map_err(|e| id_err(7, e))?
                    .with_timezone(&Utc),
            ),
            None => None,
        },
        total: total.max(0) as u64,
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
            create_new_ai_status: None,
            create_new_ai_body: None,
            chat_break_status: None,
            chat_break_body: None,
            journal_entry_ids: None,
        };
        let entry_id = entry.id;
        repo.append_push_log(entry).await.unwrap();
        let list = repo.list_push_history(10, 0).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, entry_id);
        let got = repo.get_push_log(entry_id).await.unwrap();
        assert_eq!(got.update_info_status, 200);
    }

    #[tokio::test]
    async fn push_log_create_new_ai_fields_round_trip() {
        // Regression test for the C1 audit finding: real SQLite-backed
        // push_log rows must persist the create-new-ai step result so the
        // History detail page shows it for "Push as new Kin" pushes.
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        let t = target("T", "ai_NEW_OK");
        let tid = t.id;
        repo.upsert_character(c).await.unwrap();
        repo.upsert_target(t).await.unwrap();

        let entry = PushLogEntry {
            id: Uuid::new_v4(),
            at: Utc::now(),
            character_id: cid,
            character_name: "C".into(),
            target_id: tid,
            target_ai_id: "ai_NEW_OK".into(),
            fields_sent: vec!["ai_name".into(), "ai_backstory".into()],
            did_chat_break: false,
            greeting: None,
            wipe_cascaded: None,
            update_info_status: 200,
            update_info_body: "ok".into(),
            create_new_ai_status: Some(200),
            create_new_ai_body: Some("ai_NEW_OK".into()),
            chat_break_status: None,
            chat_break_body: None,
            journal_entry_ids: None,
        };
        let entry_id = entry.id;
        repo.append_push_log(entry).await.unwrap();

        let got = repo.get_push_log(entry_id).await.unwrap();
        assert_eq!(got.create_new_ai_status, Some(200));
        assert_eq!(got.create_new_ai_body.as_deref(), Some("ai_NEW_OK"));

        let list = repo.list_push_history(10, 0).await.unwrap();
        assert_eq!(list[0].create_new_ai_status, Some(200));
        assert_eq!(list[0].create_new_ai_body.as_deref(), Some("ai_NEW_OK"));
    }

    #[test]
    fn migration_0009_persists_create_new_ai_columns() {
        // The legacy simulate-and-rerun pattern from the 0008 test: confirm
        // 0009's recreate-table migration lands the new columns on a DB
        // that started at v8 and is rolled back.
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 8;").unwrap();
        run_migrations(&mut conn).unwrap();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('push_log')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            cols.iter().any(|c| c == "create_new_ai_status"),
            "0009 should add create_new_ai_status, columns: {cols:?}"
        );
        assert!(
            cols.iter().any(|c| c == "create_new_ai_body"),
            "0009 should add create_new_ai_body, columns: {cols:?}"
        );
    }

    #[tokio::test]
    async fn save_character_image_atomic_rename_leaves_no_partial_file() {
        // Regression test for the H2 audit finding: image save must
        // write to a sibling temp file and rename atomically so an
        // interrupted write never leaves a half-written file at the
        // canonical path, and the cover_image column never points at a
        // file that doesn't yet exist on disk.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("kindroid.sqlite");
        let repo = SqliteRepository::open(&db_path).unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        // Minimal valid PNG bytes (1×1 transparent).
        let png = [
            0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];

        let rel = repo.save_character_image_bytes(cid, &png).await.unwrap();
        assert_eq!(rel, format!("images/{cid}.png"));

        // Final file is exactly the bytes we wrote.
        let on_disk = tokio::fs::read(dir.path().join(&rel)).await.unwrap();
        assert_eq!(on_disk, png);

        // No leftover `*.tmp` files in the images dir after a successful
        // save.
        let mut tmp_count = 0usize;
        let mut entries = tokio::fs::read_dir(dir.path().join("images"))
            .await
            .unwrap();
        while let Some(e) = entries.next_entry().await.unwrap() {
            let n = e.file_name();
            if n.to_string_lossy().ends_with(".tmp") {
                tmp_count += 1;
            }
        }
        assert_eq!(tmp_count, 0, "atomic save must not leave temp files");

        // Column and disk agree.
        let after = repo.get_character(cid).await.unwrap();
        assert_eq!(after.cover_image.as_deref(), Some(rel.as_str()));

        // Re-saving with different content (different extension this
        // time, a JPEG) replaces the old file and the column updates.
        let jpg = [
            0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 0, 0, 0, 0, 0,
        ];
        let rel2 = repo.save_character_image_bytes(cid, &jpg).await.unwrap();
        assert_eq!(rel2, format!("images/{cid}.jpg"));
        let on_disk2 = tokio::fs::read(dir.path().join(&rel2)).await.unwrap();
        assert_eq!(on_disk2, jpg);
        // Stale PNG is best-effort cleaned up.
        let stale_exists = tokio::fs::try_exists(dir.path().join(format!("images/{cid}.png")))
            .await
            .unwrap();
        assert!(
            !stale_exists,
            "stale PNG from previous upload should be removed"
        );
    }

    #[test]
    fn migration_0010_creates_partial_favourite_index() {
        // Regression test for the H3 audit finding: the favourites_only
        // filter path needs an index on chat_messages.favourite or it
        // degrades to a full table scan as the table grows.
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        run_migrations(&mut conn).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, sql FROM sqlite_master
                 WHERE type = 'index' AND tbl_name = 'chat_messages'",
            )
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut found = false;
        while let Some(r) = rows.next().unwrap() {
            let name: String = r.get(0).unwrap();
            let sql: Option<String> = r.get(1).unwrap();
            if name == "idx_chat_messages_favourite" {
                let sql = sql.unwrap_or_default();
                assert!(
                    sql.contains("WHERE favourite = 1"),
                    "index should be partial, got: {sql}"
                );
                found = true;
            }
        }
        assert!(found, "0010 should create idx_chat_messages_favourite");
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

    fn chat_msg(ai_id: &str, kindroid_msg_id: &str, ts: i64, text: &str) -> ChatMessage {
        ChatMessage {
            id: Uuid::new_v4(),
            ai_id: ai_id.into(),
            kindroid_msg_id: kindroid_msg_id.into(),
            sender: "user".into(),
            display_name: None,
            timestamp: ts,
            message: text.into(),
            image_urls: Vec::new(),
            image_description: None,
            video_description: None,
            internet_response: None,
            link_url: None,
            link_description: None,
            fetched_at: Utc::now(),
            favourite: false,
        }
    }

    fn chat_msg_fav(
        ai_id: &str,
        kindroid_msg_id: &str,
        ts: i64,
        text: &str,
        favourite: bool,
    ) -> ChatMessage {
        ChatMessage {
            favourite,
            ..chat_msg(ai_id, kindroid_msg_id, ts, text)
        }
    }

    #[tokio::test]
    async fn chat_messages_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "hello");
        let m2 = chat_msg("ai_x", "k2", 200, "world");
        let inserted = repo
            .upsert_chat_messages("ai_x", &[m1.clone(), m2.clone()])
            .await
            .unwrap();
        assert_eq!(inserted, 2);
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 2);

        // Listing is DESC by timestamp.
        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].timestamp, 200);
        assert_eq!(list[1].timestamp, 100);

        // Pagination with before_ts.
        let older = repo
            .list_chat_messages("ai_x", Some(200), 50, false)
            .await
            .unwrap();
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].kindroid_msg_id, "k1");
    }

    #[tokio::test]
    async fn chat_messages_dedup_on_kindroid_msg_id() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "first");
        let inserted = repo
            .upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();
        assert_eq!(inserted, 1);

        // Re-insert the same (ai_id, kindroid_msg_id) with DIFFERENT
        // content — the upsert should update the row in place and
        // report 1 (insert + actual update = 1 touch).
        let m1_edited = chat_msg("ai_x", "k1", 100, "different text");
        let touched = repo
            .upsert_chat_messages("ai_x", std::slice::from_ref(&m1_edited))
            .await
            .unwrap();
        assert_eq!(touched, 1);

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "still one row");
        assert_eq!(
            list[0].message, "different text",
            "content should be updated in place"
        );
        // Local UUID survives the update so the FTS5 rowid stays stable.
        assert_eq!(list[0].id, m1.id);
    }

    #[tokio::test]
    async fn chat_messages_idempotent_on_same_content() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "stable");
        let touched = repo
            .upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();
        assert_eq!(touched, 1);

        // Re-insert with IDENTICAL content — the WHERE clause should
        // skip the update entirely (no-op upsert).
        let m1_again = chat_msg("ai_x", "k1", 100, "stable");
        let touched_again = repo
            .upsert_chat_messages("ai_x", std::slice::from_ref(&m1_again))
            .await
            .unwrap();
        assert_eq!(touched_again, 0, "no-op upsert should not be counted");
    }

    #[tokio::test]
    async fn chat_messages_partial_edit_updates_field() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let mut m1 = chat_msg("ai_x", "k1", 100, "old body");
        m1.image_urls = vec!["https://x/a.png".into()];
        m1.link_url = Some("https://example.com".into());
        repo.upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();

        // Edit only the message body, leave the rest unchanged.
        let mut m1_edited = m1.clone();
        m1_edited.message = "new body".into();
        let touched = repo
            .upsert_chat_messages("ai_x", std::slice::from_ref(&m1_edited))
            .await
            .unwrap();
        assert_eq!(touched, 1, "single field change should still be detected");

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list[0].message, "new body");
        // Unchanged fields survive.
        assert_eq!(list[0].image_urls, vec!["https://x/a.png".to_string()]);
        assert_eq!(list[0].link_url.as_deref(), Some("https://example.com"));
        // Sender / timestamp are part of the message identity, so they
        // were never updatable.
        assert_eq!(list[0].sender, "user");
        assert_eq!(list[0].timestamp, 100);
    }

    #[tokio::test]
    async fn chat_messages_falls_back_to_max_timestamp_after_edit() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "old");
        let m2 = chat_msg("ai_x", "k2", 200, "another");
        repo.upsert_chat_messages("ai_x", &[m1.clone(), m2.clone()])
            .await
            .unwrap();

        // Edit m1 in place; m2 stays the same. Cursor logic computes
        // max(timestamps) of the response — for an overlap re-fetch the
        // newest of the returned set is `m2`'s timestamp (200).
        let mut m1_edited = m1.clone();
        m1_edited.message = "edited".into();
        repo.upsert_chat_messages("ai_x", &[m1_edited])
            .await
            .unwrap();

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        // Edited message is in place with new content.
        let mut by_id = list
            .iter()
            .map(|m| (m.kindroid_msg_id.as_str(), m))
            .collect::<Vec<_>>();
        by_id.sort_by(|a, b| a.0.cmp(b.0));
        let (k1_msg, _k2_msg) = (by_id[0].1, by_id[1].1);
        assert_eq!(k1_msg.message, "edited");
        // m1's UUID is preserved across the update.
        assert_eq!(k1_msg.id, m1.id);
    }

    #[tokio::test]
    async fn delete_missing_removes_messages_not_in_keep_ids() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "a");
        let m2 = chat_msg("ai_x", "k2", 200, "b");
        let m3 = chat_msg("ai_x", "k3", 300, "c");
        repo.upsert_chat_messages("ai_x", &[m1.clone(), m2.clone(), m3.clone()])
            .await
            .unwrap();

        // Range (100, 300]: k2 (ts 200) is in the range but not in
        // keep_ids; k1 (ts 100) is at the start_after boundary and
        // excluded by `>`; k3 (ts 300) is at the upper bound and
        // included but in keep_ids.
        let keep: Vec<&str> = vec!["k3"];
        let deleted = repo
            .delete_missing_chat_messages("ai_x", 100, 300, &keep)
            .await
            .unwrap();
        assert_eq!(deleted, 1, "only k2 should be deleted");

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        let ids: Vec<&str> = list.iter().map(|m| m.kindroid_msg_id.as_str()).collect();
        assert!(ids.contains(&"k1"));
        assert!(ids.contains(&"k3"));
        assert!(!ids.contains(&"k2"));
    }

    #[tokio::test]
    async fn delete_missing_with_empty_keep_ids_removes_everything_in_range() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "a");
        let m2 = chat_msg("ai_x", "k2", 200, "b");
        let m3 = chat_msg("ai_x", "k3", 300, "c");
        repo.upsert_chat_messages("ai_x", &[m1.clone(), m2.clone(), m3.clone()])
            .await
            .unwrap();

        // Empty keep_ids (e.g. the API returned an empty page on the
        // final has_more = false poll) — delete every row in (100, 300].
        let deleted = repo
            .delete_missing_chat_messages("ai_x", 100, 300, &[])
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "k1 (ts 100) is at the start_after boundary");
        assert_eq!(list[0].kindroid_msg_id, "k1");
    }

    #[tokio::test]
    async fn delete_missing_with_empty_range_is_a_noop() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();
        let m1 = chat_msg("ai_x", "k1", 100, "a");
        repo.upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();

        let deleted = repo
            .delete_missing_chat_messages("ai_x", 200, 200, &["k1"])
            .await
            .unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn delete_missing_only_affects_targeted_ai_id() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t_x = target("X", "ai_x");
        let t_y = target("Y", "ai_y");
        repo.upsert_target(t_x).await.unwrap();
        repo.upsert_target(t_y).await.unwrap();

        repo.upsert_chat_messages("ai_x", &[chat_msg("ai_x", "x1", 100, "x-msg")])
            .await
            .unwrap();
        repo.upsert_chat_messages("ai_y", &[chat_msg("ai_y", "y1", 100, "y-msg")])
            .await
            .unwrap();

        // Delete against ai_x — ai_y's row must survive.
        let deleted = repo
            .delete_missing_chat_messages("ai_x", 50, 200, &[])
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 0);
        assert_eq!(repo.chat_message_count("ai_y").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn delete_missing_updates_fts5_index() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let m1 = chat_msg("ai_x", "k1", 100, "searchable-text");
        let m2 = chat_msg("ai_x", "k2", 200, "another-text");
        repo.upsert_chat_messages("ai_x", &[m1, m2]).await.unwrap();

        // Before deletion the FTS5 index has both messages.
        let hits = repo
            .search_chat("ai_x", "\"searchable\"*", 50, 0, false)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);

        // Delete k1 — the FTS5 index entry must go too (via the
        // chat_messages_ad trigger).
        let deleted = repo
            .delete_missing_chat_messages("ai_x", 50, 200, &["k2"])
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let hits = repo
            .search_chat("ai_x", "\"searchable\"*", 50, 0, false)
            .await
            .unwrap();
        assert!(
            hits.is_empty(),
            "FTS5 index should be wiped for the deleted row"
        );
    }

    #[tokio::test]
    async fn chat_messages_fts_search_finds_stems() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let msgs = vec![
            chat_msg("ai_x", "k1", 100, "running through the forest"),
            chat_msg("ai_x", "k2", 200, "she runs fast"),
            chat_msg("ai_x", "k3", 300, "completely unrelated"),
        ];
        repo.upsert_chat_messages("ai_x", &msgs).await.unwrap();

        // Porter stemmer turns "running"/"runs" into "run".
        let q = "\"run\"*";
        let hits = repo.search_chat("ai_x", q, 50, 0, false).await.unwrap();
        assert!(
            hits.len() >= 2,
            "expected at least 2 hits, got {}",
            hits.len()
        );

        let q2 = "\"unrelated\"*";
        let hits2 = repo.search_chat("ai_x", q2, 50, 0, false).await.unwrap();
        assert_eq!(hits2.len(), 1);
    }

    #[tokio::test]
    async fn chat_messages_target_cascade_delete() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        let tid = t.id;
        repo.upsert_target(t).await.unwrap();
        repo.upsert_chat_messages("ai_x", &[chat_msg("ai_x", "k1", 100, "hello")])
            .await
            .unwrap();
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 1);

        repo.delete_target(tid).await.unwrap();
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 0);
        assert!(repo.get_chat_sync_state("ai_x").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn chat_sync_state_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let state = ChatSyncState {
            ai_id: "ai_x".into(),
            last_synced_at: Utc::now(),
            last_timestamp: 12345,
            full_sync_done: true,
            is_syncing: false,
            status_kind: SyncStatusKind::Cancelled,
            status_message: Some("stopped".into()),
            backoff_until: None,
            total: 42,
        };
        repo.upsert_chat_sync_state(&state).await.unwrap();
        let got = repo.get_chat_sync_state("ai_x").await.unwrap().unwrap();
        assert_eq!(got.ai_id, "ai_x");
        assert_eq!(got.last_timestamp, 12345);
        assert!(got.full_sync_done);
        assert!(!got.is_syncing);
        assert_eq!(got.status_kind, SyncStatusKind::Cancelled);
        assert_eq!(got.status_message.as_deref(), Some("stopped"));
        assert_eq!(got.total, 42);

        // Re-upsert updates in place.
        let updated = ChatSyncState {
            status_kind: SyncStatusKind::Idle,
            total: 99,
            ..state.clone()
        };
        repo.upsert_chat_sync_state(&updated).await.unwrap();
        let got2 = repo.get_chat_sync_state("ai_x").await.unwrap().unwrap();
        assert_eq!(got2.total, 99);
        assert_eq!(got2.status_kind, SyncStatusKind::Idle);
    }

    #[tokio::test]
    async fn reset_chat_history_wipes_messages_and_state() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t_x = target("X", "ai_x");
        let t_y = target("Y", "ai_y");
        repo.upsert_target(t_x).await.unwrap();
        repo.upsert_target(t_y).await.unwrap();

        // Seed messages for both targets. Seed a sync state for ai_x only.
        repo.upsert_chat_messages(
            "ai_x",
            std::slice::from_ref(&chat_msg("ai_x", "x1", 100, "hello")),
        )
        .await
        .unwrap();
        repo.upsert_chat_messages(
            "ai_x",
            std::slice::from_ref(&chat_msg("ai_x", "x2", 200, "world")),
        )
        .await
        .unwrap();
        repo.upsert_chat_messages(
            "ai_y",
            std::slice::from_ref(&chat_msg("ai_y", "y1", 100, "unrelated")),
        )
        .await
        .unwrap();
        repo.upsert_chat_sync_state(&ChatSyncState {
            ai_id: "ai_x".into(),
            last_synced_at: Utc::now(),
            last_timestamp: 200,
            full_sync_done: true,
            is_syncing: false,
            status_kind: SyncStatusKind::Idle,
            status_message: None,
            backoff_until: None,
            total: 2,
        })
        .await
        .unwrap();

        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 2);
        assert_eq!(repo.chat_message_count("ai_y").await.unwrap(), 1);

        let deleted = repo.reset_chat_history("ai_x").await.unwrap();
        assert_eq!(deleted, 2, "two messages for ai_x were deleted");

        // ai_x is fully wiped.
        assert_eq!(repo.chat_message_count("ai_x").await.unwrap(), 0);
        assert!(repo.get_chat_sync_state("ai_x").await.unwrap().is_none());

        // FTS5 index was also wiped (the trigger fires on DELETE).
        let fts_hits = repo
            .search_chat("ai_x", "\"hello\"*", 50, 0, false)
            .await
            .unwrap();
        assert!(fts_hits.is_empty(), "FTS5 entries for ai_x should be gone");

        // ai_y is untouched.
        assert_eq!(repo.chat_message_count("ai_y").await.unwrap(), 1);
        let y_hits = repo
            .search_chat("ai_y", "\"unrelated\"*", 50, 0, false)
            .await
            .unwrap();
        assert_eq!(y_hits.len(), 1);

        // Idempotent: calling reset again is a no-op.
        let deleted_again = repo.reset_chat_history("ai_x").await.unwrap();
        assert_eq!(deleted_again, 0);
    }

    #[tokio::test]
    async fn chat_message_favourite_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let mut m1 = chat_msg("ai_x", "k1", 100, "fav-target");
        m1.favourite = true;
        repo.upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert!(list[0].favourite);

        let stored = repo
            .set_chat_message_favourite("ai_x", "k1", false)
            .await
            .unwrap();
        assert!(!stored);

        let list2 = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert!(!list2[0].favourite);

        // Toggling a non-existent row leaves state untouched and returns false.
        let missing = repo
            .set_chat_message_favourite("ai_x", "missing", true)
            .await
            .unwrap();
        assert!(!missing);
    }

    #[tokio::test]
    async fn chat_message_favourite_survives_upsert() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        let mut m1 = chat_msg("ai_x", "k1", 100, "first edition");
        m1.favourite = true;
        repo.upsert_chat_messages("ai_x", std::slice::from_ref(&m1))
            .await
            .unwrap();

        // Subsequent sync that updates the content — favourite must survive.
        let m1_edited = ChatMessage {
            message: "second edition".into(),
            ..chat_msg("ai_x", "k1", 100, "placeholder")
        };
        repo.upsert_chat_messages("ai_x", std::slice::from_ref(&m1_edited))
            .await
            .unwrap();

        let list = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message, "second edition");
        assert!(
            list[0].favourite,
            "local favourite flag should survive content upsert"
        );
    }

    #[tokio::test]
    async fn chat_message_favourites_only_filter_browse() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        repo.upsert_chat_messages(
            "ai_x",
            &[
                chat_msg_fav("ai_x", "k1", 100, "pinned-a", true),
                chat_msg_fav("ai_x", "k2", 200, "unpinned", false),
                chat_msg_fav("ai_x", "k3", 300, "pinned-b", true),
            ],
        )
        .await
        .unwrap();

        let unfiltered = repo
            .list_chat_messages("ai_x", None, 50, false)
            .await
            .unwrap();
        assert_eq!(unfiltered.len(), 3);

        let only_favs = repo
            .list_chat_messages("ai_x", None, 50, true)
            .await
            .unwrap();
        assert_eq!(only_favs.len(), 2);
        let ids: Vec<&str> = only_favs
            .iter()
            .map(|m| m.kindroid_msg_id.as_str())
            .collect();
        assert!(ids.contains(&"k1"));
        assert!(ids.contains(&"k3"));
        assert!(!ids.contains(&"k2"));
        // Every returned row really is favourited.
        assert!(only_favs.iter().all(|m| m.favourite));

        // Filter also applies to the paginated path (`before_ts`).
        let older_favs = repo
            .list_chat_messages("ai_x", Some(300), 50, true)
            .await
            .unwrap();
        assert_eq!(older_favs.len(), 1);
        assert_eq!(older_favs[0].kindroid_msg_id, "k1");
    }

    #[tokio::test]
    async fn chat_message_favourites_only_filter_search() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let t = target("T", "ai_x");
        repo.upsert_target(t).await.unwrap();

        repo.upsert_chat_messages(
            "ai_x",
            &[
                chat_msg_fav("ai_x", "k1", 100, "searchable pinned", true),
                chat_msg_fav("ai_x", "k2", 200, "searchable plain", false),
            ],
        )
        .await
        .unwrap();

        let unfiltered = repo
            .search_chat("ai_x", "\"searchable\"*", 50, 0, false)
            .await
            .unwrap();
        assert_eq!(unfiltered.len(), 2);

        let only_favs = repo
            .search_chat("ai_x", "\"searchable\"*", 50, 0, true)
            .await
            .unwrap();
        assert_eq!(only_favs.len(), 1);
        assert_eq!(only_favs[0].kindroid_msg_id, "k1");
    }

    fn make_journal(character_id: Uuid, id: &str, entry: &str, kp: &[&str]) -> JournalEntry {
        let now = Utc::now();
        JournalEntry {
            id: id.to_string(),
            character_id,
            entry: entry.to_string(),
            keyphrases: kp.iter().map(|s| s.to_string()).collect(),
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn journal_crud_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let e1 = make_journal(cid, "je-1", "one", &["a"]);
        let e2 = make_journal(cid, "je-2", "two", &["b", "c"]);
        let e3 = make_journal(cid, "je-3", "three", &[]);
        repo.upsert_journal_entry(&e1).await.unwrap();
        repo.upsert_journal_entry(&e2).await.unwrap();
        repo.upsert_journal_entry(&e3).await.unwrap();

        let got = repo.list_journal_entries(cid).await.unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].entry, "one");
        assert_eq!(got[1].keyphrases, vec!["b", "c"]);
        assert!(got[2].keyphrases.is_empty());
    }

    #[tokio::test]
    async fn journal_update_preserves_created_at() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let mut e = make_journal(cid, "je-1", "first", &[]);
        repo.upsert_journal_entry(&e).await.unwrap();
        let original_created = e.created_at;

        e.updated_at = e.created_at + chrono::Duration::seconds(5);
        e.entry = "second".to_string();
        repo.upsert_journal_entry(&e).await.unwrap();

        let got = repo.list_journal_entries(cid).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].entry, "second");
        assert_eq!(got[0].created_at, original_created);
        assert!(got[0].updated_at > original_created);
    }

    #[tokio::test]
    async fn journal_delete_removes_one() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let e1 = make_journal(cid, "je-1", "one", &[]);
        let e2 = make_journal(cid, "je-2", "two", &[]);
        repo.upsert_journal_entry(&e1).await.unwrap();
        repo.upsert_journal_entry(&e2).await.unwrap();
        repo.delete_journal_entry(cid, "je-1").await.unwrap();

        let got = repo.list_journal_entries(cid).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "je-2");
    }

    #[tokio::test]
    async fn journal_delete_wrong_character_errors() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let e = make_journal(cid, "je-1", "x", &[]);
        repo.upsert_journal_entry(&e).await.unwrap();

        let other = Uuid::new_v4();
        let err = repo.delete_journal_entry(other, "je-1").await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
    }

    #[tokio::test]
    async fn automation_stable_messages_exclude_newest_and_order_ties() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.upsert_target(target("T", "ai_auto")).await.unwrap();
        let messages = (0..13)
            .map(|i| chat_msg("ai_auto", &format!("m{i:02}"), 100, &format!("m{i}")))
            .collect::<Vec<_>>();
        repo.upsert_chat_messages("ai_auto", &messages)
            .await
            .unwrap();
        let stable = repo
            .list_stable_chat_messages("ai_auto", None, 100, 10)
            .await
            .unwrap();
        assert_eq!(stable.len(), 3);
        assert_eq!(stable[0].kindroid_msg_id, "m00");
        assert_eq!(stable[2].kindroid_msg_id, "m02");
        let after = StableMessageCursor {
            timestamp: 100,
            kindroid_msg_id: "m00".into(),
        };
        let next = repo
            .list_stable_chat_messages("ai_auto", Some(&after), 100, 10)
            .await
            .unwrap();
        assert_eq!(next[0].kindroid_msg_id, "m01");
        assert_eq!(
            repo.latest_stable_cursor("ai_auto", 10)
                .await
                .unwrap()
                .unwrap()
                .kindroid_msg_id,
            "m02"
        );
    }

    #[tokio::test]
    async fn automation_state_and_audit_cascade_round_trip() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let target = target("T", "ai_auto");
        let target_id = target.id;
        repo.upsert_target(target).await.unwrap();
        let state = ChatAutomationState {
            ai_id: "ai_auto".into(),
            auto_journal_enabled: true,
            pending_summary_cursor: Some(StableMessageCursor {
                timestamp: 4,
                kindroid_msg_id: "m4".into(),
            }),
            ..Default::default()
        };
        repo.upsert_chat_automation_state(&state).await.unwrap();
        assert_eq!(
            repo.get_chat_automation_state("ai_auto").await.unwrap(),
            state
        );
        let now = Utc::now();
        let run = AutoJournalRun {
            id: "run-1".into(),
            ai_id: "ai_auto".into(),
            start_cursor: None,
            end_cursor: None,
            status: AutoJournalRunStatus::Pending,
            attempts: 0,
            completed_at: None,
            last_error: None,
            created_at: now,
        };
        repo.create_auto_journal_run(&run).await.unwrap();
        repo.create_auto_journal_entry(&AutoJournalEntry {
            id: "entry-1".into(),
            run_id: run.id.clone(),
            ai_id: run.ai_id.clone(),
            entry: "remember this".into(),
            keyphrases: vec!["test".into()],
            source_start: None,
            source_end: None,
            status: AutoJournalEntryStatus::Sent,
            response_status: Some(200),
            response_message: Some("OK".into()),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
        assert_eq!(
            repo.list_recent_successful_auto_journal_entries("ai_auto", 5)
                .await
                .unwrap()
                .len(),
            1
        );
        repo.delete_target(target_id).await.unwrap();
        assert!(matches!(
            repo.get_chat_automation_state("ai_auto").await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            repo.get_auto_journal_run("run-1").await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn journal_entries_cascade_on_character_delete() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let c = character("C");
        let cid = c.id;
        repo.upsert_character(c).await.unwrap();

        let e1 = make_journal(cid, "je-1", "x", &[]);
        let e2 = make_journal(cid, "je-2", "y", &[]);
        repo.upsert_journal_entry(&e1).await.unwrap();
        repo.upsert_journal_entry(&e2).await.unwrap();
        assert_eq!(repo.list_journal_entries(cid).await.unwrap().len(), 2);

        repo.delete_character(cid).await.unwrap();
        assert!(repo.list_journal_entries(cid).await.unwrap().is_empty());
    }

    #[test]
    fn migration_0011_drops_automation_response_columns() {
        // Regression test for the privacy cleanup: 0011 drops
        // journal_last_response / summary_last_response from
        // chat_automation_state. We use the same "run migrations, roll
        // back, run again" pattern as the 0008 / 0009 tests so the
        // migration runs once against a v10 schema (where the columns
        // are still present) and asserts the v10→v11 transition drops
        // them. After the second run, the columns are gone.
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        run_migrations(&mut conn).unwrap();

        // Re-add the columns (0011 just dropped them on the first run)
        // and roll the schema back to v10 so the second `run_migrations`
        // applies only 0011. This isolates the drop to 0011 and proves
        // the migration lands the schema change without help from any
        // other migration.
        conn.execute_batch(
            "ALTER TABLE chat_automation_state ADD COLUMN journal_last_response TEXT;
             ALTER TABLE chat_automation_state ADD COLUMN summary_last_response TEXT;",
        )
        .unwrap();
        conn.execute_batch("PRAGMA user_version = 10;").unwrap();

        let cols_before: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('chat_automation_state')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            cols_before.iter().any(|c| c == "journal_last_response"),
            "pre-0011 schema should still have journal_last_response, columns: {cols_before:?}"
        );

        run_migrations(&mut conn).unwrap();
        let cols_after: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('chat_automation_state')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            !cols_after.iter().any(|c| c == "journal_last_response"),
            "0011 must drop journal_last_response, columns: {cols_after:?}"
        );
        assert!(
            !cols_after.iter().any(|c| c == "summary_last_response"),
            "0011 must drop summary_last_response, columns: {cols_after:?}"
        );
    }

    #[tokio::test]
    async fn chat_automation_state_round_trip_without_response_columns() {
        // After 0011 the schema no longer carries the response fields.
        // Confirm a full upsert → read round-trip works against the
        // post-migration schema (would have hit a column-count mismatch
        // if the SELECT or INSERT were still sized for 28 columns).
        let repo = SqliteRepository::open_in_memory().unwrap();
        let target = target("T", "ai_x");
        repo.upsert_target(target).await.unwrap();

        let state = ChatAutomationState {
            ai_id: "ai_x".into(),
            journal_last_error: Some("boom".into()),
            summary_last_error: Some("kaboom".into()),
            ..Default::default()
        };
        repo.upsert_chat_automation_state(&state).await.unwrap();
        let got = repo.get_chat_automation_state("ai_x").await.unwrap();
        assert_eq!(got.ai_id, "ai_x");
        assert_eq!(got.journal_last_error.as_deref(), Some("boom"));
        assert_eq!(got.summary_last_error.as_deref(), Some("kaboom"));
    }

    #[tokio::test]
    async fn delete_auto_journal_run_removes_run_and_cascades_entries() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let target = target("T", "ai_del");
        repo.upsert_target(target).await.unwrap();
        let now = Utc::now();
        let run = AutoJournalRun {
            id: "del-run".into(),
            ai_id: "ai_del".into(),
            start_cursor: None,
            end_cursor: None,
            status: AutoJournalRunStatus::Failed,
            attempts: 3,
            completed_at: None,
            last_error: Some("bad".into()),
            created_at: now,
        };
        repo.create_auto_journal_run(&run).await.unwrap();
        repo.create_auto_journal_entry(&AutoJournalEntry {
            id: "del-entry".into(),
            run_id: run.id.clone(),
            ai_id: run.ai_id.clone(),
            entry: "remember this".into(),
            keyphrases: vec!["test".into()],
            source_start: None,
            source_end: None,
            status: AutoJournalEntryStatus::Error,
            response_status: Some(400),
            response_message: Some("bad".into()),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
        repo.delete_auto_journal_run(&run.id).await.unwrap();
        assert!(matches!(
            repo.get_auto_journal_run(&run.id).await,
            Err(StorageError::NotFound)
        ));
        assert!(repo
            .list_auto_journal_entries(&run.id)
            .await
            .unwrap()
            .is_empty());
    }
}
