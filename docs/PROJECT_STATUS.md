# Project Status

Last verified against commit `4a20fe1` (`Add score tempo and direction annotations`) plus uncommitted tempo-change and MusicXML tempo work. Update this handoff when a feature is completed or priorities change.

## Product Snapshot

Notula is an early-stage desktop music notation editor built with Rust, Tauri 2, Svelte 5, and TypeScript. The current renderer uses SVG drawing primitives and Bravura/SMuFL notation glyphs.

Implemented capabilities include score setup and properties, instruments and multiple staves, note/rest/chord entry, multiple voices, duration and pitch editing, articulations, ties, ordinary and phrase slurs, key/time signatures and pickup measures, undo/redo, soundfont playback, PDF/PNG/SVG export, and MusicXML import/export.

## Recent Completed Work

- The starting tempo is an ordinary tempo change at measure 1, tick 0 (`Score::initial_tempo()`); without one, playback uses quarter note = 120. There is no separate `Score::tempo` any more: files that stored the old default-tempo field load with it as a printed start mark. The New Score dialog sets the starting tempo and whether it is printed (defaults: printed quarter note = 120; `ScoreSetup` fields default the same way). Score Properties edits the start mark's tempo and whether it is printed, and only creates or changes the mark when one of those differs. Both dialogs share `ui/src/lib/TempoFields.svelte`.
- Tempo directions (`accel.`, `rit.`, `rall.`, `a tempo`, `rubato`) are stored once per score at a measure/tick position. The toolbar can add, replace, and remove a direction at the current cursor. It renders below each instrument part at the shared horizontal position, so changing it from any staff changes the score-wide annotation. Directions are intentionally ignored by playback for now.
- Direction positions follow measure insertions and removals; undo/redo restores edits. Validation rejects positions outside a measure and duplicate directions at one position.
- Tempo changes can be `hidden`: they play but are not printed. The toolbar keeps a mark's hidden flag when editing it and creates printed marks; the start mark's visibility is set in Score Properties.
- Tempo changes (metronome marks) are stored once per score at a measure/tick position, like tempo directions, and **do** affect playback: `notula_playback::TempoMap` converts ticks to seconds across changes. They render as a SMuFL metronome note plus "= N" above each instrument part, lifted clear of notes under the mark. The toolbar sets, edits, and removes the mark at the cursor. A mark at a measure's end and the next measure's start share a tick; the later one wins.
- `Tempo` has an optional `beat_unit_dots` (Serde default 0) so dotted beats such as dotted quarter = 60 are representable; the properties dialog and toolbar offer common dotted beat units.
- Score-wide marks block time-signature or pickup-length changes that would leave a mark past the end of its measure.
- MusicXML round-trips tempo: tempo changes (including the start mark) are exported as metronome marks with `<sound tempo>`, hidden ones with `print-object="no"`; tempo directions as `<words>`. All are written once, in the first part, positioned with `<offset sound="yes">`. Import reads `<metronome>` (including approximate "c. 72" values), direction or measure-level `<sound tempo>`, and recognized direction words from any part, deduplicated by position, preferring a printed metronome over a hidden one over a bare sound tempo. Bare sound tempos and hidden metronomes become hidden tempo changes. Other words such as "Allegro" are still reported as unsupported.

## Suggested Next Work

1. **Next: volume annotations (dynamics and hairpins).** The model already defines `Dynamic` and `Hairpin` on event markings, and MusicXML import/export already understands them; however, the editor has no controls for them, the layout does not render them, and playback ignores them. Decide whether these marks remain event/staff-local or should be score-wide before exposing editing. The current request described score-wide behavior for speed directions specifically; do not assume volume marks share that scope.
2. Make rit./accel./rall. affect playback; see [Planned: tempo transitions](#planned-tempo-transitions).
3. Consider tempo text such as "Allegro" (with an optional metronome) as a score-wide mark; MusicXML import currently drops it.
4. PNG/PDF export loads only Bravura into usvg, so plain text items (part names, tempo text) likely fall back to an unavailable font there. Verify and load or embed a text font if needed.

These are recommendations based on current code, not committed feature promises. Follow the user's latest request if priorities differ.

## Planned: Tempo Transitions

Agreed direction for making `rit.`, `rall.`, and `accel.` change playback speed, split into steps that can land separately. Background: MusicXML has no gradual-tempo concept. `<sound tempo>` is a single quarter-notes-per-minute value; other programs print `<words>` (sometimes with a `<dashes>` extent) and approximate the change with stepped `<sound tempo>` values (MuseScore uses four steps).

**Step A: model and playback (no UI).**
- Add an optional, hidden playback setting to the direction mark, with a Serde default so older files load:
  `TempoDirectionMark { measure, tick, direction, #[serde(default)] playback: Option<TempoTransition> }`, where `TempoTransition { end: (measure, tick), target: TransitionTarget }` and `TransitionTarget` is either an absolute `Tempo` ("slow to quarter = 70") or a percentage of the tempo in effect at the start ("to 80 %"). Percent targets keep working when an earlier tempo changes.
- Prefer a fixed end position over a length: it follows measure insertions and removals the same way marks already do. This is still open for confirmation.
- Without explicit settings, `rit.`/`rall.` use a default (for example 80 % over one measure) and `accel.` the inverse (for example 125 %). `a tempo` returns to the last tempo change before the transition and needs no settings. `rubato` stays notation-only.
- After a transition ends, its target tempo holds until the next tempo change or `a tempo`. A tempo change inside a transition cuts it off at that point.
- Extend `notula_playback::TempoMap` with a linear ramp segment (tempo linear in ticks). Seconds across a ramp of L ticks from q0 to q1 quarter notes per minute are `60 / 960 · L / (q1 − q0) · ln(q1 / q0)` (plain division when q0 = q1); `tick_at` inverts it with `exp`. The audio thread and playback cursor only call `seconds_at` and `tick_at`, so they need no changes.
- Model maintenance mirrors tempo changes: measure insertion and removal move or drop the end, shortening a measure must not strand it, and validation rejects an end before the start.

**Step B: editing UI and engraving.**
- Selecting or placing a `rit.`/`accel.`/`rall.` at the cursor opens a small popover: target ("slow to [70] BPM" or "[80] %") and length ("over [1] measure", or until a chosen position). Clearing the settings returns to the defaults.
- When a transition is set, draw dashes after the text up to the end ("rit. - - - -"), which makes the hidden extent visible and matches MusicXML `<dashes>`.

**Step C: MusicXML.**
- Export `<words>` plus `<dashes type="start">`/`"stop"` for the extent, and stepped `<sound tempo>` values (for example one per beat) so other programs play an approximation.
- On import, fold `<sound tempo>` steps that fall inside a recognized `rit.`/`accel.` extent into one transition (end of the extent, last value as target) instead of creating many tempo changes, so a Notula round trip returns the same single transition.

A dedicated playback annotation mode (timeline view, curve shapes, per-note timing) is a later topic; `TempoTransition` can gain a curve field without a format break.

## Useful Code Map

- Score model, initial tempo, and score-wide tempo directions and changes: `crates/notula-model/src/score.rs`
- Reversible score mutations and measure position maintenance: `crates/notula-model/src/command.rs`
- Event-local dynamics, hairpins, and other markings: `crates/notula-model/src/markings.rs`; edit helpers: `crates/notula-model/src/edit.rs`
- Shared staff layout, measure columns, and tempo-direction engraving: `crates/notula-layout/src/measure.rs`, `crates/notula-layout/src/lib.rs`
- Drawing primitives and SVG frontend rendering: `crates/notula-layout/src/render.rs`, `ui/src/lib/ScoreView.svelte`
- Tauri API and command registration: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Frontend bridge and editor state: `ui/src/lib/api.ts`, `ui/src/lib/editor.svelte.ts`
- New score and score properties dialogs, shared starting-tempo fields: `ui/src/lib/NewScoreDialog.svelte`, `ui/src/lib/ScorePropertiesDialog.svelte`, `ui/src/lib/TempoFields.svelte`; backend: `src-tauri/src/setup.rs`
- Toolbar controls: `ui/src/lib/Toolbar.svelte`
- MusicXML import/export: `crates/notula-format/src/musicxml.rs`
- Playback event generation, tempo map, and audio scheduling: `crates/notula-playback/src/lib.rs`, `src-tauri/src/playback.rs`

## Working In A New Session

Start with a concrete task and ask the agent to read `AGENTS.md` and this file, inspect the worktree, and continue from the current code rather than assuming previous-session context. State the desired behavior, scope boundaries, and acceptance criteria. For example:

> Read `AGENTS.md` and `docs/PROJECT_STATUS.md`, inspect the current worktree, and implement [specific behavior]. Keep [scope boundary] unchanged. Acceptance criteria: [observable outcomes]. Add focused tests, run the relevant checks, and update the project status if the milestone changes.

For a larger task, split independent research from implementation, but keep one session responsible for integrating model, UI, layout, and validation. Ask for a review or commit separately; commits should only be made when explicitly requested.