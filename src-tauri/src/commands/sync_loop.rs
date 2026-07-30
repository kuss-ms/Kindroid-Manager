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

/// Overlap window applied to the cursor after the first full sync. Each
/// subsequent poll re-fetches and re-upserts the last few minutes of
/// messages; the server replies with current content (an edited message
/// keeps its `kindroid_msg_id` / `timestamp`) and the upsert picks up
/// the change.
pub const OVERLAP_MINUTES: i64 = 3;
pub const OVERLAP_MS: i64 = OVERLAP_MINUTES * 60 * 1000;

/// Compute the `start_after_timestamp` to send to the API.
///
///   * `last_timestamp == 0`: first call — return `None`.
///   * During the initial backfill (`full_sync_done == false`):
///     pass the cursor through unchanged so the drain walks forward
///     through the history without re-fetching.
///   * After the first full sync (`full_sync_done == true`): rewind
///     the cursor by `OVERLAP_MS` so each poll re-confirms the last
///     few minutes and catches edits. If the rewind underflows
///     (very first poll after a brand-new sync) fall back to the
///     original cursor.
pub fn compute_start_after_timestamp(
    last_timestamp: i64,
    full_sync_done: bool,
) -> Option<i64> {
    if last_timestamp <= 0 {
        return None;
    }
    if !full_sync_done {
        return Some(last_timestamp);
    }
    let rewound = last_timestamp.saturating_sub(OVERLAP_MS);
    if rewound > 0 {
        Some(rewound)
    } else {
        Some(last_timestamp)
    }
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

    #[test]
    fn compute_cursor_returns_none_for_zero_cursor() {
        assert_eq!(compute_start_after_timestamp(0, false), None);
        assert_eq!(compute_start_after_timestamp(0, true), None);
    }

    #[test]
    fn compute_cursor_passes_through_during_initial_backfill() {
        // During the drain we walk forward without rewinding; we send
        // the cursor straight through.
        assert_eq!(
            compute_start_after_timestamp(1_700_000_000_000, false),
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn compute_cursor_rewinds_after_full_sync() {
        // After the first sync, rewind by 3 min (180_000 ms).
        let cursor = 1_700_000_180_000;
        assert_eq!(
            compute_start_after_timestamp(cursor, true),
            Some(cursor - OVERLAP_MS)
        );
    }

    #[test]
    fn compute_cursor_falls_back_when_rewind_underflows() {
        // If the cursor is within OVERLAP_MS of zero (very first poll on
        // a brand-new sync), fall back to the original cursor rather
        // than returning 0 / None.
        assert_eq!(
            compute_start_after_timestamp(60_000, true),
            Some(60_000)
        );
        assert_eq!(
            compute_start_after_timestamp(OVERLAP_MS - 1, true),
            Some(OVERLAP_MS - 1)
        );
    }
}
