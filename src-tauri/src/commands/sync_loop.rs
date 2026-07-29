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
