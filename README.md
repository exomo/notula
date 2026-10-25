# Notula

Notula is an open-source desktop music notation editor built with Rust and Tauri, with a Svelte and TypeScript interface. It is an early-stage project and is still under active development.

## What it can do

- Create scores and configure instruments, staves, and score properties.
- Enter notes, rests, chords, and multiple independent voices.
- Edit note durations, dots, accidentals, pitches, and chord tones.
- Add staccato, staccatissimo, tenuto, accent, marcato, and fermata markings.
- Set key and time signatures and create pickup measures.
- Undo and redo score edits.
- Play scores using a selected soundfont.
- Export scores as PDF, PNG, or SVG.

Other notation options, including dynamics and hairpins, are not yet available in the editor.

## Development

You will need a current Rust toolchain and Node.js 22. Tauri also requires platform-specific system dependencies; on Linux, see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your distribution.

```sh
npm ci
npm run tauri dev
```

Run the UI checks and build with:

```sh
npm run check -w ui
npm run build -w ui
```

Run the Rust tests with:

```sh
cargo test --workspace
```

## AI-generated code and license

This codebase is AI-generated with human direction and review. It is provided under the MIT License; see [LICENSE](LICENSE) for the full license text.
