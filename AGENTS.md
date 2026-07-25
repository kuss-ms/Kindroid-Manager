# Kindroid Manager — Agent Notes

## Toolchain

- Node + pnpm (install: `npm i -g pnpm`).
- Rust stable, GNU toolchain (`rustup default stable-x86_64-pc-windows-gnu` on Windows).
- Tauri 2 dev dependencies per <https://v2.tauri.app/start/prerequisites/>.

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
pnpm lint               # ESLint
pnpm format:check       # Prettier check
pnpm test               # Vitest

# Rust
cargo fmt --check       # rustfmt
cargo clippy -- -D warnings
cargo test
```

## Layout

```
src/                       React + TS frontend
src-tauri/src/
  domain/                  Domain types + share-code
  storage/                 Repository trait + SQLite impl + migrations
  kindroid/                HTTP client trait + reqwest impl
  security/                keyring wrapper
  commands/                Tauri commands (thin wrappers + push flow)
  error.rs                 Top-level AppError + PushResult
```

## Key conventions

- All Tauri `invoke` calls live in `src/lib/api.ts` only — pages/components never import from `@tauri-apps/api/core`.
- Share-code encode/decode is canonical in Rust (`domain::share_code`).
- Token is stored in OS keychain; commands return booleans, never the raw token.
- DB schema uses portable types only (TEXT UUIDs, ISO-8601 datetimes, JSON arrays).
