/// Escape an arbitrary user query into a safe FTS5 expression.
///
/// Tokens are whitespace-separated. A token wrapped in `"..."` becomes an
/// **exact phrase** match (no wildcard). An unwrapped token becomes a
/// **prefix** match (suffix `*`). All parts are joined with ` AND ` so that
/// every term (or phrase) must be present in a matching message.
///
/// Each raw token / phrase is stripped of FTS5 metacharacters
/// (`*`, `(`, `)`, `:`, `^`), any internal `"` is doubled so it survives
/// FTS5 phrase parsing, and the cleaned text is then re-wrapped. Empty
/// parts after cleaning are dropped. An unmatched opening quote is
/// forgiving: the remainder of the input is treated as a plain unquoted
/// token rather than producing an error.
///
/// FTS5 examples produced by this function:
///
/// * `hello world`         → `"hello"* AND "world"*`
/// * `"hello world"`       → `"hello world"`
/// * `hello "world peace"` → `"hello"* AND "world peace"`
/// * `he*y :foo^bar`       → `"hey"* AND "foobar"*`
/// * empty / whitespace    → `""`
pub fn escape_fts_query(query: &str) -> String {
    const META: &[char] = &['*', '(', ')', ':', '^'];

    let mut parts: Vec<String> = Vec::new();
    let bytes = query.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip whitespace between tokens.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == b'"' {
            // Quoted phrase: read until the next '"' or end-of-input.
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let raw = &query[start..i];
            let closed = i < bytes.len();
            // If we landed on a closing quote, advance past it.
            if closed {
                i += 1;
            }
            // FTS5 tokenisation matches our linear scan: an unmatched
            // opening quote falls back to being treated as a plain token
            // (so the user still gets prefix-matching feedback instead of
            // a silently different search mode).
            if let Some(part) = if closed {
                build_phrase(raw, META)
            } else {
                build_token(raw, META)
            } {
                parts.push(part);
            }
        } else {
            // Plain token: read until whitespace or a `"`.
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'"' {
                i += 1;
            }
            let raw = &query[start..i];
            if let Some(token) = build_token(raw, META) {
                parts.push(token);
            }
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" AND ")
    }
}

fn build_token(raw: &str, meta: &[char]) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| !meta.contains(c)).collect();
    if cleaned.is_empty() {
        return None;
    }
    let escaped = cleaned.replace('"', "\"\"");
    Some(format!("\"{escaped}\"*"))
}

fn build_phrase(raw: &str, meta: &[char]) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| !meta.contains(c)).collect();
    if cleaned.is_empty() {
        return None;
    }
    let escaped = cleaned.replace('"', "\"\"");
    Some(format!("\"{escaped}\""))
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
    fn unquoted_tokens_become_prefix_anded() {
        assert_eq!(escape_fts_query("hello world"), "\"hello\"* AND \"world\"*");
    }

    #[test]
    fn quoted_text_becomes_exact_phrase() {
        assert_eq!(escape_fts_query("\"hello world\""), "\"hello world\"");
    }

    #[test]
    fn mixed_quoted_and_unquoted_are_anded() {
        assert_eq!(
            escape_fts_query("hello \"world peace\""),
            "\"hello\"* AND \"world peace\""
        );
    }

    #[test]
    fn multiple_phrases_are_anded() {
        assert_eq!(
            escape_fts_query("\"foo bar\" \"baz qux\""),
            "\"foo bar\" AND \"baz qux\""
        );
    }

    #[test]
    fn strips_metachars_from_both_tokens_and_phrases() {
        assert_eq!(
            escape_fts_query("he*y (wor)ld :foo^bar"),
            "\"hey\"* AND \"world\"* AND \"foobar\"*"
        );
        assert_eq!(escape_fts_query("\"he*y :foo\""), "\"hey foo\"");
    }

    #[test]
    fn embedded_quote_tokenizes_like_fts5() {
        // FTS5 treats each standalone `"` as a phrase boundary, so an
        // input like `"he said "hi"` parses the same way FTS5 itself
        // would: phrase `he said ` AND token `hi`. This matches the
        // reference behaviour of the upstream `chat_messages_fts`
        // tokenizer; trying to be cleverer would diverge from it.
        assert_eq!(
            escape_fts_query("\"he said \"hi\""),
            "\"he said \" AND \"hi\"*"
        );
    }

    #[test]
    fn preserves_unicode_and_digits_and_hyphens() {
        assert_eq!(
            escape_fts_query("hello-world 2024 café"),
            "\"hello-world\"* AND \"2024\"* AND \"café\"*"
        );
    }

    #[test]
    fn unmatched_open_quote_treats_rest_as_plain_token() {
        assert_eq!(
            escape_fts_query("hello \"world"),
            "\"hello\"* AND \"world\"*"
        );
    }

    #[test]
    fn empty_inputs_return_empty_string() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
        assert_eq!(escape_fts_query("\t\n"), "");
    }

    #[test]
    fn drops_tokens_that_are_only_metacharacters() {
        assert_eq!(escape_fts_query("*** hello"), "\"hello\"*");
        assert_eq!(escape_fts_query("***"), "");
        // A quoted phrase that is only metacharacters is also dropped.
        assert_eq!(escape_fts_query("\"***\" hello"), "\"hello\"*");
    }

    #[test]
    fn preserves_quoted_single_word_phrase() {
        // `"hello"` is a phrase of length 1 — no wildcard, no AND merge.
        assert_eq!(
            escape_fts_query("\"hello\" world"),
            "\"hello\" AND \"world\"*"
        );
    }

    #[test]
    fn empty_phrase_is_dropped() {
        // `""` produces an empty phrase after cleaning, so it's dropped.
        assert_eq!(escape_fts_query("\"\" hello"), "\"hello\"*");
    }

    #[test]
    fn handles_multiple_kinds_of_whitespace() {
        assert_eq!(
            escape_fts_query("hello\tworld\nfoo"),
            "\"hello\"* AND \"world\"* AND \"foo\"*"
        );
    }
}
