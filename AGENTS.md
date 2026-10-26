# Agent Guide

Before starting work:

1. Read `docs/PROJECT_STATUS.md` for the current product state and active follow-ups.
2. Check `git status --short` and preserve existing user or generated changes. Do not overwrite or remove files under `/files/`; this directory is ignored and may contain local scores.
3. Trace the owning model, command, UI, and layout path before editing. Follow existing patterns and keep changes focused.

## Architecture

- Rust workspace, edition 2024: `notula-model` owns score data, validation, and undoable commands; `notula-layout` turns scores into serializable drawing primitives; `notula-format` handles native files and MusicXML; `notula-playback` generates note events; `src-tauri` exposes app commands and audio; `ui/` is Svelte 5 + TypeScript.
- Score changes should go through `Document::execute(Command::...)` so undo/redo and dirty-state tracking work. Persisted fields need Serde defaults when older score files should continue to load.
- Model annotations at the scope where they apply: event/staff markings live with their owning events or staves, while score-wide markings belong on `Score` rather than being duplicated across instruments. The layout already shares measure columns across staves.
- Playback events use ticks with 960 ticks per quarter note. Tempo directions are currently notation-only; do not make them affect playback unless that is part of the task.
- UI-to-Rust calls use Tauri commands in `src-tauri/src/commands.rs`, registered in `src-tauri/src/lib.rs`, and mirrored by `ui/src/lib/api.ts`.

## Validation

Common checks:

```sh
cargo fmt --all --check
cargo test --workspace
cargo check --manifest-path src-tauri/Cargo.toml
npm run check -w ui
npm run build -w ui
git diff --check
```

CI additionally runs:

```sh
cargo clippy --all-targets -- -D warnings
cargo clippy -p notula-app --all-targets -- -D warnings
```

Tauri development requires Node.js 22, a current Rust toolchain, and platform-specific Tauri dependencies.

Keep `docs/PROJECT_STATUS.md` current when completing a milestone or changing a product decision. Do not commit unless the user asks.