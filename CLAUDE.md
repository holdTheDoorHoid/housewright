# Working on Housewright

Read `docs/DECISIONS.md` first; it is the source of truth. `docs/DESIGN-PRINCIPLES.md`
governs every user-facing choice. Entries marked *Proposed* may change; entries
marked *Owner* may not be changed without the owner.

## Rules

- Rust for the core, server, CLI and the Home Assistant bridge. The interface
  is Svelte (TypeScript) inside Tauri 2. The one exception (D-117, Owner): the
  Home Assistant integration in `integrations/home-assistant/` is Python, and
  only Home Assistant glue: every Housewright rule lives in Rust (the bridge,
  `crates/bridge`), and the integration only calls the bridge's local API.
  Do not add any other language without asking.
- Do not re-enable incremental compilation; `sccache` is the shared cache.
  Do not point worktrees at a shared `CARGO_TARGET_DIR`.
- One task, one branch, one git worktree:
  `git -C ~/Desktop/housewright worktree add ~/Desktop/hw-<task> -b <branch>`.
- The schedule engine stays pure (no clock, no I/O): "now" is a parameter.
- Completions are append-only. Never write code that edits or deletes one in place.
- Every field that could be sensitive (codes, account details, valuables'
  locations) must carry a sensitivity flag; the binder relies on it (D-41).
- Tests: `cargo test --workspace`; `cargo clippy --workspace -- -D warnings`;
  `cargo fmt --check`.
- No manufacturer manuals or scraped content in the repository (D-21).
- Never push to `main` without the owner's go-ahead for that change; work on
  branches.
