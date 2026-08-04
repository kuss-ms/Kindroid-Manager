use std::sync::Arc;

use uuid::Uuid;

use crate::storage::Repository;

/// Capture a pre-save snapshot for `character_id`. Called from
/// `save_character`, `save_journal_entry`, and `delete_journal_entry`
/// before the primary mutation. Failure is non-fatal — we log and
/// continue so the user's save still succeeds, losing at most one
/// history entry.
///
/// The repository's `snapshot_character` is the only place that touches
/// the DB mutex; this wrapper exists so the three call sites stay
/// terse and uniform.
pub async fn snapshot_before(repo: &Arc<dyn Repository>, character_id: Uuid) {
    if let Err(e) = repo.snapshot_character(character_id).await {
        eprintln!("character snapshot failed for {character_id}: {e}");
    }
}
