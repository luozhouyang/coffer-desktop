# Agent rules

## Commits

- Never add `Co-Authored-By` trailers (including model names) to commit messages.
- Use Conventional Commit subjects: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `ci:`.

## Repository

- `docs/` is local-only and must not be committed unless explicitly requested.
- Before committing code, run `pnpm typecheck`, `pnpm test`, and `cargo test --manifest-path src-tauri/Cargo.toml`.
