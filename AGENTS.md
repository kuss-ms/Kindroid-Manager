# Kindroid Manager — Agent Notes

## Toolchain

- Node + pnpm (install: `npm i -g pnpm`).
- Rust stable, GNU toolchain (`rustup default stable-x86_64-pc-windows-gnu` on Windows).
- Tauri 2 dev dependencies per <https://v2.tauri.app/start/prerequisites/>.
- On Windows, MinGW-w64 (`gcc`/`dlltool`/`ld`/`windres`) must be on `PATH` for the GNU Rust target.
- Linux requires a Secret Service daemon (`gnome-keyring`, `kwalletd`, …) for the keychain crate. **Sandboxed/non-interactive Linux environments without a real login session cannot create the `default` Secret Service collection, which makes the 7 `commands::push::tests::*` tests fail — see the matching Troubleshooting entry, do not waste time chasing it.**

## Common commands

```sh
# Install JS deps
pnpm install

# Dev (Vite + Tauri shell)
pnpm tauri dev

# Release build for current OS
pnpm tauri build

# Frontend
pnpm typecheck          # tsc --noEmit
pnpm lint               # eslint . --max-warnings 0
pnpm format             # prettier --write .
pnpm format:check       # prettier --check .
pnpm test               # vitest run
pnpm build              # tsc --noEmit && vite build (frontend only)

# Rust
cargo fmt --check       # rustfmt
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

## Layout

```
src/                             React + TS frontend
  components/                    AppLayout, Toaster, ConfirmDialog, FieldChecklist, OnboardingBanner
  pages/                         Characters, CharacterEditor, Targets, Push, History, HistoryDetail, ChatHistory, Settings
  lib/                           api.ts (Tauri invoke layer + escapeFtsQuery), schemas.ts (zod), types.ts
  styles/                        global.css (design tokens + components)
src-tauri/src/
  domain/
    character.rs                 Character struct (incl. cover_image) + PERSONA_FIELDS
    target.rs                    Target struct (ai_id, label)
    push_log.rs                  PushLogEntry + MAX_LOG_BODY_BYTES (+ journal_entry_ids)
    share_code.rs                PartialCharacter + text encode/decode (kept for tests)
    image_share.rs               PNG tEXt share-image encode/decode (ComfyUI-style)
    chat_message.rs              ChatMessage + ChatSyncState + SyncStatusKind
    journal_entry.rs             JournalEntry + JournalEntryInput (MAX_ENTRY_CHARS, MAX_KEYPHRASES, MAX_KEYPHRASE_CHARS)
  storage/
    mod.rs                       Repository trait (incl. image + chat_history ops)
    sqlite.rs                    SqliteRepository impl + data_dir + image file helpers
    migrations/                  0001_init, 0002_add_cover_image, 0003_add_avatar_description, 0004_chat_history
  kindroid/
    http.rs                      HttpKindroidClient (reqwest, 30 s timeout, wiremock tests) incl. list_chat_messages
  security/
    secrets.rs                   keyring wrapper (keyring v3)
  commands/
    characters.rs                save / get / list / delete / duplicate logic
    targets.rs                   target CRUD logic
    push.rs                      do_push flow (update-info → journal-create → chat-break → log) + FakeRepo test template
    share_code.rs                import_share_image / export_share_image / set_character_image
    settings.rs                  base_url + token_status / set_token / clear_token / test_token
    history.rs                   push-log read API
    chat_history.rs              chat-history read API + start/cancel sync entry points (start_chat_sync is cfg(not(test)))
    journal.rs                   list / save / delete journal entries (local CRUD)
    sync_registry.rs             single-slot background-task registry (Arc<Mutex<Option<SyncEntry>>> + watch::Sender<bool))
    sync_loop.rs                 escape_fts_query helper (always compiled) + tests
    sync_loop_impl.rs            the actual background loop (#[cfg(not(test))] — uses tauri::AppHandle / Emitter)
    tauri_wrappers.rs            #[tauri::command] thin wrappers (cfg(not(test)) only)
  error.rs                       AppError (struct variants, tagged enum for serde)
  lib.rs                         re-exports for tests; #[cfg(not(test))] mod app
  app.rs                         Tauri 2 builder, app.manage(repo + client + SyncRegistry) (#[cfg(not(test))])
  main.rs                        binary entry, calls lib::run
```

## Key conventions

- **Git commit messages**: subject line only by default, imperative mood, ≤ 60 characters, no body. Match the existing log style (e.g. `Cursor-based chat history pagination`, `Add chat message favourite (pin) feature`, `Rewind chat sync by recent local messages`, `Version bumped to 0.2.1`). The diff and PR description carry the details; the commit subject is for `git log --oneline` scanning. Add a short body only when the subject alone is insufficient (e.g. a non-obvious bug fix or breaking refactor whose rationale isn't visible from the diff). No multi-paragraph explanations, no `Co-Authored-By` trailers, no "this commit…" preambles.
- All Tauri `invoke` calls live in `src/lib/api.ts` only — pages/components never import from `@tauri-apps/api/core`.
- `commands/tauri_wrappers.rs` is the only file that imports `tauri::State` or wraps a function in `#[tauri::command]`. Tests skip it via `#[cfg(not(test))]`.
- The `app::run` entry installs `Arc<dyn Repository>`, `Arc<dyn KindroidClient>`, and `Arc<SyncRegistry>` into Tauri state; commands take them out via `State` and call into `commands::*` plain async functions.
- **Never reference `tauri::AppHandle`, `tauri::Emitter`, or `tauri::async_runtime` from non-`#[cfg(not(test))]` code.** On Windows, doing so pulls `WebView2Loader.dll` into the test binary, which then fails to load with `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) and the entire `cargo test --lib` exits silently. The pattern used by `start_chat_sync` / `run_sync_loop` is: put the Tauri-dependent code in its own file (`sync_loop_impl.rs`) gated with `#[cfg(not(test))]`, expose it through a thin entry in a non-gated file (`chat_history.rs`) that is itself also gated where it touches `tauri::AppHandle`. See "Tauri/test build" in Troubleshooting for the diagnostic.
- Share-image encode/decode is canonical in Rust (`domain::image_share`): a `tEXt` chunk under the keyword `kindroid` containing the same `{"v":1,"p":{...}}` payload as the old text-based share code.
- Cover images are stored as raw bytes (PNG / JPEG / WebP / GIF, detected by magic bytes) under `images/{character_id}.{ext}` in the data dir. `Repository::save_character_image_bytes` also updates the `cover_image` column so the field stays in sync. `duplicate_character` copies the file so each character owns its own.
- Uploading a cover image via the editor strips any `kindroid` `tEXt` chunk from the PNG (`image_share::strip_kindroid_metadata`). The global drag-drop / paste import path keeps the chunk and uses it as the persona payload.
- Token is stored in the OS keychain via the `keyring` crate, service `KindroidManager`, user `api_token`. `#[cfg(test)]` uses a separate `KindroidManager-test` / `api_token-test` entry so `cargo test` never touches the user's real keychain.
- Tauri 2 is configured with `dragDropEnabled: false` on the main window so HTML5 drag/drop events reach the webview. Without that flag, Tauri intercepts native file drops and our `window.addEventListener('drop', …)` never fires.
- DB schema uses portable types only (TEXT UUIDs, ISO-8601 datetimes, JSON arrays). All FKs use `ON DELETE CASCADE` so deleting a target wipes its `chat_messages` + `chat_sync_state` automatically.
- Errors flow through `AppError` (struct variants, `#[serde(tag = "kind", rename_all = "snake_case")]`). The frontend unwraps them via `errorMessage(e)` in `src/lib/api.ts`, which parses `e.message` (Tauri 2 wraps the serialized JSON in there). **Every new `AppError` variant must be appended to the `serializes_all_variants_as_json` test in `error.rs`** or CI breaks.
- FTS5 chat search: `chat_messages_fts` is in external-content mode over `chat_messages.message` with `tokenize='porter unicode61'`. The query produced by `escape_fts_query` (Rust) / `escapeFtsQuery` (TS) is whitespace-tokenised and quote-aware: an unquoted token becomes `"token"*` (prefix match), a `"…"` block becomes `"…"` (exact phrase, no wildcard), and all parts are joined with `AND` so every term (or phrase) must be present. An unmatched opening `"` falls back to prefix-token mode. **The Rust and TS versions must stay in lock-step.** Porter handles inflectional variants ("running"/"runs" → "run"), but NOT irregular forms ("ran" does NOT match "run" — test data uses "runs" not "ran").
- Background-task pattern: any long-running loop is a free `async fn` in a `#[cfg(not(test))]` module. It is spawned via `tauri::async_runtime::spawn` from a thin `#[cfg(not(test))]` entry in `commands::*`. Cancellation goes through a `tokio::sync::watch::Receiver<bool>` paired with a sender held in `SyncRegistry`. The registry is single-slot per the chat-history plan: a second `start()` returns the currently-syncing `ai_id`, and the UI must surface this via `AppError::SyncConflict`. The loop **must** call `SyncRegistry::release()` on every exit path (success, error, cancel, token-cleared) so a future sync can take the slot. Use the `run_loop_inner` + outer `run_sync_loop` wrapper pattern from `commands/sync_loop_impl.rs` so the release is guaranteed even on early return.
- Chat message favourite (pin): `chat_messages.favourite` is the local source of truth — the Kindroid `get-chat-messages` endpoint does not return `isPinned`, so server-side pins set in other clients are invisible until the user re-toggles here. The column **survives** `upsert_chat_messages` because `favourite` is included in the INSERT column list (so the inserted value is recorded) but omitted from both the UPDATE SET list and the `IS NOT` WHERE checks (so subsequent re-fetches do not clobber it). The `commands::chat_history::toggle_chat_message_favourite` command calls `POST /toggle-message-pin` and then reconciles the local row to the server's canonical `isPinned` response — the optimistic UI flip is rolled back on failure. The read API (`list_chat_messages` / `search_chat`) accepts a `favourites_only` flag that appends `AND favourite = 1` to the outer WHERE clause (NOT to the FTS5 MATCH, so Porter stemming is unaffected).
- Journal entries are local-only. `character_journal_entries` is a child table with `ON DELETE CASCADE` from `characters`; deleting a character removes its journal rows automatically. The new `/journal-create` endpoint is called per selected entry from the Push page, sequentially after a successful `/update-info` and before any `/chat-break` call; per-entry failures do not abort the push and each becomes a `JournalEntryStep` in the `PushResult.journal_entries` vector. Validation (length + keyphrase count) runs up-front so an invalid entry never triggers a network call. `PushLogEntry.journal_entry_ids` stores the ids of the entries that were actually sent (used by the Re-push button to pre-select them); the field is `#[serde(default)]` so old log rows deserialize as `None`. Share images never include journal entries (`notes_are_not_in_share_code` is the precedent for this exclusion).

## Manual end-to-end checklist (matches the README)

1. Launch with no token → first-run banner appears.
2. Settings → paste a bogus token → **Test** → "Invalid or missing API key". Replace with a real token → "OK". Clear token → banner reappears.
3. Create a Character "Test Bot" with a unique backstory.
4. Add a Target with a real `ai_id`.
5. Push (no chat-break) → verify in Kindroid.
6. Edit the backstory, push with chat-break + greeting → verify both.
7. Export a share image, reset the app's data folder, drop the share image on the main window → character reappears with all fields and the cover image intact.
8. From History, click **Re-push** on the last entry → Push page pre-filled.
9. Disconnect the network, push → `(network)` error toast; log records the failure.
10. Push a character whose `user_name` differs from the AI's existing user → confirm the checklist warning is visible (still supported as a read-only field on existing data).
11. On the Push page, enable chat-break on a character that has a stored greeting → confirm the textarea is pre-filled and the Push button is enabled.
12. From the Characters overview, click **Share** on a character with a cover image → confirm the PNG is on the system clipboard.
13. Toggle "Reset Cascaded Memory" → confirm the warning callout appears explaining the data loss risk.
14. Click **Duplicate** on a character with a cover image → confirm the duplicate also shows the image in the edit screen.
15. Open **Chat History** with no targets → "Add a target on the Targets page…" empty state.
16. Add a target → return to Chat History, select it. Click **Sync** → status flips to "Syncing…", counter advances. Cancel mid-loop → state becomes `Cancelled`; clicking Sync resumes from the saved cursor.
17. Search for a word in the search bar → top hits render with a snippet; search "run" finds "running"/"runs" (Porter stem). Two words (`hello world`) require both terms; `"hello world"` (quoted) is an exact phrase match.
18. Disconnect the network, click Sync → state becomes `Error` with a message; click Sync again to retry.
19. With target A syncing, open Chat History for target B → action disabled, "Sync in progress for A". Cancel A to sync B.
20. Click the heart on a row → row updates instantly; the same message in the Kindroid web UI shows pinned.
21. With network disabled → click heart → row updates → ~1 s later reverts and error toast appears.
22. Toggle "Favourites only" → only pinned rows render in both browse and search modes.
23. Pin a message, run Sync → pin state survives.
24. Delete the target → pinned messages are gone (FK CASCADE applies).
25. Open a character with 3 journal entries → Push page lists them with checkboxes. Select 2, push → `/journal-create` is called twice in id order; both 200 → result shows two green `journal:` rows.
26. Re-push a log entry that originally pushed 2 journal entries → Push page pre-selects those 2 ids via the URL param.
27. Push a character with chat-break enabled and journal entries selected → order on the server is `update-info` → `journal-create ×N` → `chat-break`; all visible in the result block.
28. Push with `update-info` failing (network off) → no journal calls fire; only `update-info` is shown in the result, error toast appears.
29. Editor: try to save an entry with 9 keyphrases → error toast "at most 8 keyphrases"; counter shows `8/8` and the 9th is rejected client-side as well.
30. Editor: save an entry with 501 characters → error toast "entry must be 500 characters or fewer".
31. Export a character with 5 journal entries as a share image → reset app data → drop the image → character reappears with 0 journal entries (documented local-only behavior).
32. Delete a character with journal entries → entries are gone (FK CASCADE).

## Troubleshooting

- "dlltool.exe: program not found" during build → MinGW is missing from PATH (see Toolchain).
- Token "lost" between dev restarts → check the OS keychain via `cmdkey /list:KindroidManager*` (Windows). If the entry is missing, `cargo test` clobbered it — should be impossible since the `cfg(test)` split uses a separate `KindroidManager-test` entry, but you can `cmdkey /delete:KindroidManager.api_token` and re-enter.
- Drag-drop in dev mode does nothing but clipboard paste works → `dragDropEnabled` was reset to `true` somewhere; re-check `src-tauri/tauri.conf.json`.
- **`cargo test --lib` exits silently with `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139)** on Windows → the test binary is being linked against `WebView2Loader.dll`. A new file is referencing `tauri::AppHandle`, `tauri::Emitter`, or `tauri::async_runtime` from outside a `#[cfg(not(test))]` block. Grep for those symbols and either gate the function/file with `#[cfg(not(test))]` or follow the `sync_loop` / `sync_loop_impl` split pattern. Confirm by running the binary directly: `target\debug\deps\kindroid_manager_lib-*.exe --list` will fail with exit code `0xC0000139` if WebView2 was pulled in. Use `objdump -p <exe>` (mingw) to confirm `WebView2Loader.dll` is in the import table. The baseline `cargo test --lib` should report `94 passed; 0 failed` after the FTS-search change (or `92 passed; 0 failed` post-chat-favourite / `71` pre-favourite) — except in sandboxed Linux, where 7 `commands::push::tests::*` always fail with `keyring::Error::Unavailable` (see the entry below); ignore those.
- **`cargo clippy --all-targets -- -D warnings` complains about `clippy::await_holding_lock`** in a background loop → the loop is holding a `tokio::sync::Mutex` (e.g. `SqliteRepository`'s connection) across a `client.*` await. Restructure so the `repo.*` call is its own short-lived scope and the lock is released before the await on `client.*`.
- **`cargo test --lib` reports `92 passed; 7 failed` (or `85 + 7` after the FTS-search change) with all 7 failures in `commands::push::tests::*`** → every failing test panics at `Secrets::set("test-token").unwrap()` in `src/commands/push.rs:472` with `Err(keyring::Error::Unavailable)`. This is an **environment problem, not a code regression**. `keyring` v3's `sync-secret-service` backend on Linux looks up the Secret Service's `default` collection by alias; in a sandbox/SSH session without a real login, `gnome-keyring-daemon` may be running but the alias is unmapped (verify: `gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.Secret.Service.ReadAlias "default"` returns `(objectpath '/',)`), and `Service.CreateCollection` refuses to create one without a PAM auth flow (`gdbus ... CreateCollection "{...Label: <default>}" ""` errors with `Only the 'default' alias is supported`, the Prompt object disconnects without replying). These 7 tests pass on the user's actual dev machine (Windows/macOS/Linux-with-login) and reproduce with `git stash` on master, so they are safe to ignore in this sandbox. Do not change `secrets.rs`, `keyring` features, or `push.rs` to try to fix them.
