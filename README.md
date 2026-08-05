# Kindroid Manager

A Tauri 2 desktop app (Windows / macOS / Linux) for authoring Kindroid
persona "characters" locally and pushing them to one of your Kindroids
via the official `POST /update-info` endpoint (and optionally
`POST /chat-break`). The API token is stored in the OS keychain. Local
storage is SQLite, behind a `storage::Repository` trait so a remote
backend can be added later without touching the UI.

## What it does

- Local authoring of Kindroid persona "characters" (name, ai_name,
  backstory, memory, directive, example message, additional context,
  current scene, greeting, notes).
- Many-to-many push: a single Character can be pushed to any Target
  (an `ai_id` + label) at any time. Pick the fields to send each time.
- Optional chat-break after `update-info`, with editable greeting and
  `wipe_cascaded`.
- A **share image** for exporting / importing a
  character between installs. Drop the PNG (or paste from clipboard)
  on the main window to import; the image itself is saved as the
  character's cover image. Two installs interoperate; Kindroid does
  not consume the metadata. The share code embeds the persona fields
  (name, backstory, memory, directive, example message, additional
  context, current scene, greeting, avatar description) plus the
  per-character **journal entries** (entry text + keyphrases; ids and
  timestamps are local-only and regenerated on import). `notes` is
  still local-only and is not part of the share code. The wire format
  is `CURRENT_VERSION = 2` in `domain::share_code`; v=1 codes still
  decode (without the journal entries they were missing).
- A push history with the full request/response body and a one-click
  re-push.
- A **Chat History** viewer that pulls messages from the
  `GET /get-chat-messages` endpoint into a local SQLite + FTS5 cache
  per target. Runs a long-lived background sync, shows a live
  progress indicator (request count + last message timestamp + batch
  size), supports prefix/Porter-stem search with AND-of-terms
  semantics (wrap a phrase in `"…"` for an exact match), and a
  click-to-expand
  detail pop-up. A **Reset** button clears the local cache and sync
  cursor so you can do a clean full resync.
- A "Test token" probe that does a best-effort reachability + auth
  check without mutating state.

## Screenshot

<img width="899" height="336" alt="image" src="https://github.com/user-attachments/assets/c714ebc3-5edb-48d3-aef6-48749aac29d1" />


## Install

Prebuilt installers will be published on the GitHub Releases page.

Supported targets:

- Windows: MSI / NSIS installer
- macOS: DMG (universal if practical)
- Linux: AppImage + .deb

Tauri mobile builds are not shipped in v1.

## Prerequisites (building from source)

- Node.js + pnpm (`npm i -g pnpm`)
- Rust stable (GNU toolchain on Windows, MSVC on macOS)
- Tauri 2 dev dependencies — see
  <https://v2.tauri.app/start/prerequisites/>

On Linux, a Secret Service provider (e.g. `gnome-keyring`) must be
running for token storage to work.

## Development

```sh
pnpm install
pnpm tauri dev
```

## Build

```sh
pnpm tauri build
```

The installer lands in `src-tauri/target/release/bundle/<platform>/`.

## Where to find your API key and AI ID

Open the Kindroid app or visit
<https://kindroid.ai> → **Settings** → **General** → **API & advanced integrations**. The API key is
shown there; the AI ID is the identifier of the AI you want to push
to. The API key is valid for all AI IDs.

## Troubleshooting

- **Linux: "OS keychain is not available"** — install and run
  `gnome-keyring` (or another Secret Service) and retry.
- **Want to reset the local DB** — navigate to the app's data dir and delete `kindroid-manager.db`.
  Characters and history are gone; the token is untouched (it lives in
  the keychain). To wipe just the chat history for a single target,
  open **Chat History**, pick the target, click **Reset** and confirm.
- **401 / 403** — token is rejected. Re-enter it in Settings.
- **429** — rate-limited. The chat-history sync loop honours the
  `Retry-After` header on its own (status flips to `Paused until …`
  with a countdown); the regular push endpoints do not auto-retry, so
  wait a few seconds and retry manually.
- **400** — Kindroid rejected the request body (often an empty field
  that the API doesn't accept). The full response body is in the
  History detail.
- **Chat history stuck at "Syncing…"** — cancel the sync, check the
  status message. If it reads `Rate-limited`, the loop is waiting for
  the rate-limit window; if it reads `Error`, click **Sync** to retry.
  Use **Reset** to start over from a clean cursor.

## Architecture

Tauri 2 + React + TypeScript + Rust + SQLite. Rust traits separate
persistence (`storage::Repository`) from the Kindroid HTTP client
(`kindroid::KindroidClient`) so a future remote-storage or
webservice-mode can slot in without changing the UI. The frontend's
only Tauri dependency is `src/lib/api.ts` — every page/component talks
to Rust through that one file.

## Acknowledgments

API by [Kindroid](https://kindroid.ai/).
