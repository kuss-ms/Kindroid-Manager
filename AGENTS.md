# Kindroid Manager — Agent Notes

## Toolchain

- Node + pnpm (install: `npm i -g pnpm`).
- Rust stable, GNU toolchain (`rustup default stable-x86_64-pc-windows-gnu` on Windows).
- Tauri 2 dev dependencies per <https://v2.tauri.app/start/prerequisites/>.
- On Windows, MinGW-w64 (`gcc`/`dlltool`/`ld`/`windres`) must be on `PATH` for the GNU Rust target.
- Linux requires a Secret Service daemon (`gnome-keyring`, `kwalletd`, …) for the keychain crate. **Sandboxed/non-interactive Linux environments without a real login session cannot create the `default` Secret Service collection, which makes the 7 `commands::push::tests::*` tests fail — see the matching Troubleshooting entry, do not waste time chasing it.**
- **Android** (for personal-sideload APK builds, see `.kilo/plans/1785596514600-android-deployment-plan.md`): JDK 17+, Android SDK (`platforms;android-33`, `build-tools;33.0.2`), NDK `29.0.14206865`, and the `aarch64-linux-android` / `armv7-linux-androideabi` / `x86_64-linux-android` Rust targets. Export `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `ANDROID_NDK_HOME`; prepend `$ANDROID_HOME/platform-tools` to `PATH` (for `adb`). `scripts/setup-android.sh` is an idempotent verifier that prints the export lines; do not run its `sdkmanager` lines on a machine where the toolchain is already installed.

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

# Android (scaffold lives in src-tauri/gen/android/)
pnpm tauri:android dev                                          # dev (needs a connected device)
pnpm exec -- tauri android build --apk                           # release APK, unsigned if no keystore
bash scripts/build-android.sh                                   # frontend build + signed release APK
adb install -r src-tauri/gen/android/app/build/outputs/apk/release/app-release.apk
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
    push_log.rs                  PushLogEntry + MAX_LOG_BODY_BYTES (+ journal_entry_ids, create_new_ai_status/body)
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
    push.rs                      do_push flow (update-info → journal-create → chat-break → log) + do_create_new_kin (create-new-ai → update-info → journal-create → log + target upsert) + FakeRepo test template
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
- Share-image encode/decode is canonical in Rust (`domain::image_share`): a `tEXt` chunk under the keyword `kindroid` containing the JSON payload `{"v":2,"p":{...}}`. `v=2` adds `journal_entries` (each `{ entry, keyphrases }`; ids and timestamps are local-only and regenerated on import). `v=1` codes (without `journal_entries`) still decode thanks to `#[serde(default)]`.
- Cover images are stored as raw bytes (PNG / JPEG / WebP / GIF, detected by magic bytes) under `images/{character_id}.{ext}` in the data dir. `Repository::save_character_image_bytes` also updates the `cover_image` column so the field stays in sync. `duplicate_character` copies the file so each character owns its own.
- Uploading a cover image via the editor strips any `kindroid` `tEXt` chunk from the PNG (`image_share::strip_kindroid_metadata`). The global drag-drop / paste import path keeps the chunk and uses it as the persona payload.
- Token is stored in the OS keychain via the `keyring` crate, service `KindroidManager`, user `api_token`. `#[cfg(test)]` uses a separate `KindroidManager-test` / `api_token-test` entry so `cargo test` never touches the user's real keychain.
- Tauri 2 is configured with `dragDropEnabled: false` on the main window so HTML5 drag/drop events reach the webview. Without that flag, Tauri intercepts native file drops and our `window.addEventListener('drop', …)` never fires.
- DB schema uses portable types only (TEXT UUIDs, ISO-8601 datetimes, JSON arrays). All FKs use `ON DELETE CASCADE` so deleting a target wipes its `chat_messages` + `chat_sync_state` automatically.
- Errors flow through `AppError` (struct variants, `#[serde(tag = "kind", rename_all = "snake_case")]`). The frontend unwraps them via `errorMessage(e)` in `src/lib/api.ts`, which parses `e.message` (Tauri 2 wraps the serialized JSON in there). **Every new `AppError` variant must be appended to the `serializes_all_variants_as_json` test in `error.rs`** or CI breaks.
- FTS5 chat search: `chat_messages_fts` is in external-content mode over `chat_messages.message` with `tokenize='porter unicode61'`. The query produced by `escape_fts_query` (Rust) / `escapeFtsQuery` (TS) is whitespace-tokenised and quote-aware: an unquoted token becomes `"token"*` (prefix match), a `"…"` block becomes `"…"` (exact phrase, no wildcard), and all parts are joined with `AND` so every term (or phrase) must be present. An unmatched opening `"` falls back to prefix-token mode. **The Rust and TS versions must stay in lock-step.** Porter handles inflectional variants ("running"/"runs" → "run"), but NOT irregular forms ("ran" does NOT match "run" — test data uses "runs" not "ran").
- Background-task pattern: any long-running loop is a free `async fn` in a `#[cfg(not(test))]` module. It is spawned via `tauri::async_runtime::spawn` from a thin `#[cfg(not(test))]` entry in `commands::*`. Cancellation goes through a `tokio::sync::watch::Receiver<bool>` paired with a sender held in `SyncRegistry`. The registry is single-slot per the chat-history plan: a second `start()` returns the currently-syncing `ai_id`, and the UI must surface this via `AppError::SyncConflict`. The loop **must** call `SyncRegistry::release()` on every exit path (success, error, cancel, token-cleared) so a future sync can take the slot. Use the `run_loop_inner` + outer `run_sync_loop` wrapper pattern from `commands/sync_loop_impl.rs` so the release is guaranteed even on early return.
- Chat message favourite (pin): `chat_messages.favourite` is the local source of truth — the Kindroid `get-chat-messages` endpoint does not return `isPinned`, so server-side pins set in other clients are invisible until the user re-toggles here. The column **survives** `upsert_chat_messages` because `favourite` is included in the INSERT column list (so the inserted value is recorded) but omitted from both the UPDATE SET list and the `IS NOT` WHERE checks (so subsequent re-fetches do not clobber it). The `commands::chat_history::toggle_chat_message_favourite` command calls `POST /toggle-message-pin` and then reconciles the local row to the server's canonical `isPinned` response — the optimistic UI flip is rolled back on failure. The read API (`list_chat_messages` / `search_chat`) accepts a `favourites_only` flag that appends `AND favourite = 1` to the outer WHERE clause (NOT to the FTS5 MATCH, so Porter stemming is unaffected).
- **Android token storage:** the `keyring` crate v3 has no Android backend and falls back to its in-memory mock store, so the token would be lost on every restart. The `Secrets` impl in `src-tauri/src/security/secrets.rs` is gated `#[cfg(target_os = "android")]` and writes the token to a plaintext file at `<app_data_dir>/token`; `app.rs` calls `Secrets::init(data_dir)` once at setup. Plaintext is acceptable for the personal-sideload threat model (app sandbox + Android file-based encryption at rest). Keystore-backed encryption is a deferred upgrade. The existing 7 callsites of `Secrets::*` are unchanged.
- **Android scaffold (`src-tauri/gen/android/`):** the Tauri 2 scaffold's `cargo tauri android init` has a known bug where the `tauri.settings.gradle` and `app/tauri.build.gradle.kts` files referenced by `settings.gradle` / `app/build.gradle.kts` are not created (the Tauri CLI's `android-studio-script` command panics when run outside a Gradle build with a live dev server). Both are gitignored and committed as empty placeholders — a basic app with no native Rust crate deps needs no actual content in either. If you add a Rust crate that exposes an `android/` build script, populate `tauri.build.gradle.kts` to include it. Also note: the gradle plugin's `BuildTask.kt` calls `node tauri android android-studio-script` from `src-tauri/`, but there is no `tauri` JS file there — a small `src-tauri/tauri` shim (`import '../node_modules/@tauri-apps/cli/tauri.js'`) is required for the rustBuild tasks to invoke the CLI. The release APK output is `gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk` (not `…/release/app-release.apk`).
- **Android signing:** the release `signingConfig` is wired through `keystore.properties` (gitignored, lives at `src-tauri/gen/android/keystore.properties`). Without it, the release build falls back to the auto-generated debug key, so the APK installs but cannot be reinstalled over a real release build (`INSTALL_FAILED_UPDATE_INCOMPATIBLE`). The `signingConfigs { create("release") { … } }` block MUST be declared before `buildTypes { getByName("release") { signingConfig = signingConfigs.getByName("release") } }` in `gen/android/app/build.gradle.kts` — referencing a not-yet-declared config throws `SigningConfig with name 'release' not found` at config time.
- Journal entries are local CRUD (`character_journal_entries` child table, `ON DELETE CASCADE` from `characters`; deleting a character removes its journal rows automatically) and are also embedded in the share image payload as `PartialCharacter.journal_entries`. On import the persona fields, cover image, and journal entries (entry text + keyphrases) are recreated under a new character id with fresh `JournalEntry.id`/`created_at`/`updated_at`. The new `/journal-create` endpoint is called per selected entry from the Push page, sequentially after a successful `/update-info` and before any `/chat-break` call; per-entry failures do not abort the push and each becomes a `JournalEntryStep` in the `PushResult.journal_entries` vector. Validation (length + keyphrase count) runs up-front so an invalid entry never triggers a network call, and is also re-run on share-image import so a hand-crafted code with an over-length entry is rejected. `PushLogEntry.journal_entry_ids` stores the ids of the entries that were actually sent (used by the Re-push button to pre-select them); the field is `#[serde(default)]` so old log rows deserialize as `None`. `notes` is still local-only (see `notes_are_not_in_share_code`).
- Push as new Kin uses `POST /create-new-ai` with `ai_name`, `ai_gender`, `ai_backstory`, `custom_avatar_description`, `custom_greeting`; then a follow-up `POST /update-info` (always called, with at least `ai_id`) for the remaining persona fields; then `POST /journal-create` for each entry. The new `ai_id` is registered as a local target. `custom_avatar_description` is sent on create-new-ai only. The endpoint's plain-text body is the new `ai_id`; it is trimmed and an empty response is treated as `AppError::Invalid`.

## Manual end-to-end checklist (matches the README)

1. Launch with no token → first-run banner appears.
2. Settings → paste a bogus token → **Test** → "Invalid or missing API key". Replace with a real token → "OK". Clear token → banner reappears.
3. Create a Character "Test Bot" with a unique backstory.
4. Add a Target with a real `ai_id`.
5. Push (no chat-break) → verify in Kindroid.
6. Edit the backstory, push with chat-break + greeting → verify both.
7. Export a share image (with journal entries embedded), reset the app's data folder, drop the share image on the main window → character reappears with all fields, journal entries, and the cover image intact.
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
31. Export a character with 5 journal entries as a share image → reset app data → drop the image → character reappears with 5 journal entries (entry text + keyphrases preserved; ids and timestamps are new).
32. Delete a character with journal entries → entries are gone (FK CASCADE).
33. From Characters overview, click **Push as new Kin** on a character with `ai_name`, no journal entries → confirm; toast shows `New Kin created with ai_id …`; Push History detail lists `create-new-ai response` (status 200) and `update-info response` (status 200); Targets list now contains a row with the new ai_id and the AI name as label.
35. Sync a target with fewer than 10 messages → automation does not process them; add stable messages and confirm the newest 10 remain excluded.
36. Enable auto-journal after a completed sync → no historical backfill occurs; after the configured interval of stable messages, generated entries are sent to Kindroid.
37. Force a partial `journal-create` failure → successful entries remain sent, the failed entry is retried on the next completed sync, and no successful entry is regenerated.
38. Enable auto-summary with **Bootstrap from existing history** → the next completed sync summarizes all stable history; switch to **Incremental only** and confirm no initial AI call occurs.
39. Add enough new stable messages for an incremental summary → the selected Kindroid field is updated; switch backend with an over-limit summary and confirm the reformat path runs before the remote update.
40. Click **Reset summary** → local summary, candidate, and cursor clear while auto-journal settings and audit entries remain; **Run summary now** respects incremental-only no-op behavior.
41. Set global automation instructions, then set a target override → prompts use override first, global second, and hard-coded defaults when both are empty; restore and clear each override.
42. Configure an authless AI endpoint and an authenticated endpoint → automation sends an explicit empty AI bearer for the former and the stored bearer for the latter; no token appears in logs.
43. Delete a target with automation enabled → automation state, pending runs, and generated audit entries are removed by cascade.

## Troubleshooting

- "dlltool.exe: program not found" during build → MinGW is missing from PATH (see Toolchain).
- Token "lost" between dev restarts → check the OS keychain via `cmdkey /list:KindroidManager*` (Windows). If the entry is missing, `cargo test` clobbered it — should be impossible since the `cfg(test)` split uses a separate `KindroidManager-test` entry, but you can `cmdkey /delete:KindroidManager.api_token` and re-enter.
- Drag-drop in dev mode does nothing but clipboard paste works → `dragDropEnabled` was reset to `true` somewhere; re-check `src-tauri/tauri.conf.json`.
- **`cargo test --lib` exits silently with `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139)** on Windows → the test binary is being linked against `WebView2Loader.dll`. A new file is referencing `tauri::AppHandle`, `tauri::Emitter`, or `tauri::async_runtime` from outside a `#[cfg(not(test))]` block. Grep for those symbols and either gate the function/file with `#[cfg(not(test))]` or follow the `sync_loop` / `sync_loop_impl` split pattern. Confirm by running the binary directly: `target\debug\deps\kindroid_manager_lib-*.exe --list` will fail with exit code `0xC0000139` if WebView2 was pulled in. Use `objdump -p <exe>` (mingw) to confirm `WebView2Loader.dll` is in the import table. The baseline `cargo test --lib` should report `94 passed; 0 failed` after the FTS-search change (or `92 passed; 0 failed` post-chat-favourite / `71` pre-favourite) — except in sandboxed Linux, where 7 `commands::push::tests::*` always fail with `keyring::Error::Unavailable` (see the entry below); ignore those.
- **`cargo clippy --all-targets -- -D warnings` complains about `clippy::await_holding_lock`** in a background loop → the loop is holding a `tokio::sync::Mutex` (e.g. `SqliteRepository`'s connection) across a `client.*` await. Restructure so the `repo.*` call is its own short-lived scope and the lock is released before the await on `client.*`.
- **`cargo test --lib` reports `92 passed; 7 failed` (or `85 + 7` after the FTS-search change) with all 7 failures in `commands::push::tests::*`** → every failing test panics at `Secrets::set("test-token").unwrap()` in `src/commands/push.rs:472` with `Err(keyring::Error::Unavailable)`. This is an **environment problem, not a code regression**. `keyring` v3's `sync-secret-service` backend on Linux looks up the Secret Service's `default` collection by alias; in a sandbox/SSH session without a real login, `gnome-keyring-daemon` may be running but the alias is unmapped (verify: `gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.Secret.Service.ReadAlias "default"` returns `(objectpath '/',)`), and `Service.CreateCollection` refuses to create one without a PAM auth flow (`gdbus ... CreateCollection "{...Label: <default>}" ""` errors with `Only the 'default' alias is supported`, the Prompt object disconnects without replying). These 7 tests pass on the user's actual dev machine (Windows/macOS/Linux-with-login) and reproduce with `git stash` on master, so they are safe to ignore in this sandbox. Do not change `secrets.rs`, `keyring` features, or `push.rs` to try to fix them.
