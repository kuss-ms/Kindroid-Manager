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
- A **share image** (ComfyUI-style PNG) for exporting / importing a
  character between installs. Drop the PNG (or paste from clipboard)
  on the main window to import; the image itself is saved as the
  character's cover image. Two installs interoperate; Kindroid does
  not consume the metadata.
- A push history with the full request/response body and a one-click
  re-push.
- A **Chat History** viewer that pulls messages from the
  `GET /get-chat-messages` endpoint into a local SQLite + FTS5 cache
  per target. Runs a long-lived background sync, shows a live
  progress indicator (request count + last message timestamp + batch
  size), supports prefix / Porter-stem search, and a click-to-expand
  detail pop-up. A **Reset** button clears the local cache and sync
  cursor so you can do a clean full resync.
- A "Test token" probe that does a best-effort reachability + auth
  check without mutating state.

## Screenshot

_(TODO)_

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
<https://kindroid.ai/home/> → **Profile Settings**. The API key is
shown there; the AI ID is the identifier of the AI you want to push
to.

## Manual end-to-end checklist

1. Launch with no token: first-run banner appears.
2. Open Settings, paste a bogus token, click **Test** → expect
   "Invalid or missing API key". Replace with a real token, **Test** →
   expect "OK". Clear token → Settings shows "not configured" and the
   banner reappears.
3. Create a Character "Test Bot" with a unique backstory.
4. Add a Target with a real `ai_id` from your Kindroid profile.
5. Push (no chat-break) → confirm the change is visible in the
   Kindroid web/app.
6. Edit the backstory, push again with chat-break + greeting →
   confirm both took effect and the History page shows two entries.
7. Export a share image, reset the app's data folder, drop the share
   image on the main window → character reappears with all fields and
   the cover image intact.
8. From the History page, click **Re-push** on the last entry → Push
   page opens with the same character / target / fields / chat-break
   pre-filled. Push again to confirm the round-trip.
9. Disconnect the network, push → confirm a `(network)` error is
   shown and the log records the failure status.
10. Push a character whose `user_name` differs from the AI's existing
    user — confirm the checklist warning is visible and that ticking
    it off leaves the AI's user identity unchanged on the next push.
11. On the Push page, enable chat-break on a character that has a
    stored greeting → confirm the greeting textarea is pre-filled from
    the character and is editable; edit it, push, then confirm the
    History row shows the edited greeting (not the character's
    default) and the Character's stored greeting is unchanged after
    the push. Then enable chat-break on a character with no greeting
    → confirm the textarea starts empty with the hint and the Push
    button stays disabled until a greeting is typed.
12. Open **Chat History** with no targets → confirm the
    "Add a target on the Targets page…" empty state.
13. Add a target → return to Chat History, pick it. Click **Sync** →
    the status flips to "Syncing…", the request counter advances,
    and the message list refreshes as new pages arrive. Cancel the
    sync mid-loop → status becomes `Cancelled`; clicking **Sync**
    again resumes from the saved cursor.
14. Type a word in the search bar → top hits render with a snippet;
    search "run" finds "running" / "runs" (Porter stem). Click any
    row → the detail pop-up shows the full message, image links,
    link, and metadata.
15. Disconnect the network, click **Sync** → status becomes `Error`
    with a message; click **Sync** again to retry once the network is
    back.
16. With target A syncing, open Chat History for target B → the
    **Sync** button on B is disabled with the tooltip
    "Sync in progress for A". Cancel A to free the slot.
17. Click **Reset** on a target that already has cached messages →
    confirm the dialog, then verify the page shows no messages and
    the sync state is gone. Click **Sync** to start a fresh backfill.

## Troubleshooting

- **Linux: "OS keychain is not available"** — install and run
  `gnome-keyring` (or another Secret Service) and retry.
- **Want to reset the local DB** — open Settings → "Open data folder"
  (or navigate to the app's data dir) and delete `kindroid-manager.db`.
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
