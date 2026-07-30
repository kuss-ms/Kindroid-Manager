-- v5: chat message favourite (pin) toggle
ALTER TABLE chat_messages ADD COLUMN favourite INTEGER NOT NULL DEFAULT 0;
