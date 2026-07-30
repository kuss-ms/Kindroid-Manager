/// Escape an arbitrary user query into a safe FTS5 prefix-match expression.
///
/// Each whitespace-separated token is double-quoted (so FTS5 treats it as
/// a literal phrase), stripped of FTS5 metacharacters, with internal `"`
/// doubled, and suffixed with `*` for prefix matching. The result is
/// `token1* OR token2* OR token3*`.
pub fn escape_fts_query(query: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for token in query.split_whitespace() {
        let stripped: String = token
            .chars()
            .filter(|c| !matches!(c, '*' | '(' | ')' | ':' | '^'))
            .collect();
        if stripped.is_empty() {
            continue;
        }
        let escaped = stripped.replace('"', "\"\"");
        out.push(format!("\"{escaped}\"*"));
    }
    if out.is_empty() {
        String::new()
    } else {
        out.join(" OR ")
    }
}

/// How many of the most recent local messages to re-check on every
/// post-full-sync poll. Kindroid allows editing and deleting only the
/// last 10 messages; the +2 margin covers the strict-`>` API
/// boundary plus one position just outside the edit window.
pub const REWIND_MESSAGE_COUNT: u32 = 12;

/// Compute the `start_after_timestamp` for an outgoing chat-history
/// request based on the **local DB** rather than the cursor or
/// wall-clock time. The cursor can drift arbitrarily far past the
/// user's editable messages (the AI keeps responding and the cursor
/// advances with every new message), so anchoring the rewind to
/// `cursor − X` or `now() − X` either misses older messages or wastes
/// bandwidth. Anchoring to the local 12 newest messages keeps the
/// rewind window exactly aligned with the user's edit window.
///
///   * `last_timestamp == 0` and `!full_sync_done`: first call —
///     return `None`.
///   * During the initial backfill (`full_sync_done == false`): pass
///     the cursor through unchanged so the drain walks forward
///     without re-fetching.
///   * After the first full sync: rewind to the timestamp of the
///     oldest of the 12 most recent local messages (or the oldest
///     message if we have < 12), minus 1 so the boundary message is
///     strictly inside the response window.
pub async fn compute_local_rewind(
    repo: &dyn crate::storage::Repository,
    ai_id: &str,
    full_sync_done: bool,
    last_timestamp: i64,
) -> Result<Option<i64>, crate::error::AppError> {
    if last_timestamp <= 0 && !full_sync_done {
        return Ok(None);
    }
    if !full_sync_done {
        return Ok(Some(last_timestamp));
    }
    let msgs = repo
        .list_chat_messages(ai_id, None, REWIND_MESSAGE_COUNT, false)
        .await?;
    if msgs.is_empty() {
        return Ok(None);
    }
    // `msgs` is sorted DESC by timestamp. The oldest of the returned
    // list is either the 12th-most-recent (if we have >= 12) or the
    // actual oldest (if < 12). Subtracting 1 puts the boundary message
    // strictly inside the response window (the API uses `>`).
    let boundary = msgs.last().unwrap().timestamp - 1;
    Ok(Some(boundary.max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_fts_query_basic() {
        let q = escape_fts_query("hello world");
        assert_eq!(q, "\"hello\"* OR \"world\"*");
    }

    #[test]
    fn escape_fts_query_strips_metachars_and_doubles_quotes() {
        let q = escape_fts_query("he*y (wor)ld :foo^bar \"quoted\"");
        // The metachar strip keeps `"` (it's a phrase delimiter, not a
        // content character), then `replace('"', "\"\"")` doubles it.
        // Wrapping then adds one extra pair, so the inner `"quoted"`
        // becomes `"""quoted"""*` (1 wrap-quote + 2 escaped + 2 escaped +
        // 1 wrap-quote on each side).
        assert_eq!(
            q,
            "\"hey\"* OR \"world\"* OR \"foobar\"* OR \"\"\"quoted\"\"\"*"
        );
    }

    #[test]
    fn escape_fts_query_preserves_unicode_and_digits() {
        let q = escape_fts_query("hello-world 2024 café");
        assert_eq!(q, "\"hello-world\"* OR \"2024\"* OR \"café\"*");
    }

    #[test]
    fn escape_fts_query_empty() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
        assert_eq!(escape_fts_query("***"), "");
    }
}
