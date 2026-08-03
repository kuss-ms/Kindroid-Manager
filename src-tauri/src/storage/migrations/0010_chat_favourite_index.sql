-- v10: partial index on chat_messages.favourite for the "favourites
-- only" browse/search paths. Partial because the vast majority of
-- messages are not pinned; an index that covers every row would be
-- wasted space and slow INSERT/UPDATE.
--
-- Exists for audit H3 (M3 in the audit findings) — `favourites_only`
-- used to force a full table scan.
CREATE INDEX IF NOT EXISTS idx_chat_messages_favourite
    ON chat_messages(ai_id, favourite) WHERE favourite = 1;
