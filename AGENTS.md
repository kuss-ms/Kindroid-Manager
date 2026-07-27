# Kindroid Manager — Agent Notes

## Toolchain

- Node + pnpm (install: `npm i -g pnpm`).
- Rust stable, GNU toolchain (`rustup default stable-x86_64-pc-windows-gnu` on Windows).
- Tauri 2 dev dependencies per <https://v2.tauri.app/start/prerequisites/>.
- On Windows, MinGW-w64 (`gcc`/`dlltool`/`ld`/`windres`) must be on `PATH` for the GNU Rust target.
- Linux requires a Secret Service daemon (`gnome-keyring`, `kwalletd`, …) for the keychain crate.

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
  pages/                         Characters, CharacterEditor, Targets, Push, History, HistoryDetail, Settings
  lib/                           api.ts (Tauri invoke layer), schemas.ts (zod), types.ts
  styles/                        global.css (design tokens + components)
src-tauri/src/
  domain/
    character.rs                 Character struct (incl. cover_image) + PERSONA_FIELDS
    target.rs                    Target struct (ai_id, label)
    push_log.rs                  PushLogEntry + MAX_LOG_BODY_BYTES
    share_code.rs                PartialCharacter + text encode/decode (kept for tests)
    image_share.rs               PNG tEXt share-image encode/decode (ComfyUI-style)
  storage/
    mod.rs                       Repository trait (incl. image ops)
    sqlite.rs                    SqliteRepository impl + data_dir + image file helpers
    migrations/                  0001_init.sql, 0002_add_cover_image.sql, 0003_add_avatar_description.sql
  kindroid/
    http.rs                      HttpKindroidClient (reqwest, 30 s timeout, wiremock tests)
  security/
    secrets.rs                   keyring wrapper (keyring v3)
  commands/
    characters.rs                save / get / list / delete / duplicate logic
    targets.rs                   target CRUD logic
    push.rs                      do_push flow (update-info → chat-break → log)
    share_code.rs                import_share_image / export_share_image / set_character_image
    settings.rs                  base_url + token_status / set_token / clear_token / test_token
    history.rs                   push-log read API
    tauri_wrappers.rs            #[tauri::command] thin wrappers (cfg(not(test)) only)
  error.rs                       AppError (struct variants, tagged enum for serde)
  lib.rs                         re-exports for tests; #[cfg(not(test))] mod app
  app.rs                         Tauri 2 builder, app.manage(repo + client) (#[cfg(not(test))])
  main.rs                        binary entry, calls lib::run
```

## Key conventions

- All Tauri `invoke` calls live in `src/lib/api.ts` only — pages/components never import from `@tauri-apps/api/core`.
- `commands/tauri_wrappers.rs` is the only file that imports `tauri::State` or wraps a function in `#[tauri::command]`. Tests skip it via `#[cfg(not(test))]`.
- The `app::run` entry installs `Arc<dyn Repository>` and `Arc<dyn KindroidClient>` into Tauri state; commands take them out via `State` and call into `commands::*` plain async functions.
- Share-image encode/decode is canonical in Rust (`domain::image_share`): a `tEXt` chunk under the keyword `kindroid` containing the same `{"v":1,"p":{...}}` payload as the old text-based share code.
- Cover images are stored as raw bytes (PNG / JPEG / WebP / GIF, detected by magic bytes) under `images/{character_id}.{ext}` in the data dir. `Repository::save_character_image_bytes` also updates the `cover_image` column so the field stays in sync. `duplicate_character` copies the file so each character owns its own.
- Uploading a cover image via the editor strips any `kindroid` `tEXt` chunk from the PNG (`image_share::strip_kindroid_metadata`). The global drag-drop / paste import path keeps the chunk and uses it as the persona payload.
- Token is stored in the OS keychain via the `keyring` crate, service `KindroidManager`, user `api_token`. `#[cfg(test)]` uses a separate `KindroidManager-test` / `api_token-test` entry so `cargo test` never touches the user's real keychain.
- Tauri 2 is configured with `dragDropEnabled: false` on the main window so HTML5 drag/drop events reach the webview. Without that flag, Tauri intercepts native file drops and our `window.addEventListener('drop', …)` never fires.
- DB schema uses portable types only (TEXT UUIDs, ISO-8601 datetimes, JSON arrays).
- Errors flow through `AppError` (struct variants, `#[serde(tag = "kind", rename_all = "snake_case")]`). The frontend unwraps them via `errorMessage(e)` in `src/lib/api.ts`, which parses `e.message` (Tauri 2 wraps the serialized JSON in there).

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

## Troubleshooting

- "dlltool.exe: program not found" during build → MinGW is missing from PATH (see Toolchain).
- Token "lost" between dev restarts → check the OS keychain via `cmdkey /list:KindroidManager*` (Windows). If the entry is missing, `cargo test` clobbered it — should be impossible since the `cfg(test)` split uses a separate `KindroidManager-test` entry, but you can `cmdkey /delete:KindroidManager.api_token` and re-enter.
- Drag-drop in dev mode does nothing but clipboard paste works → `dragDropEnabled` was reset to `true` somewhere; re-check `src-tauri/tauri.conf.json`.
