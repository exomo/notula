# Project Status

Last verified against commit `2f0c826` (`Fit slurs, phrase slurs, and ties around their notes`) on `main`, plus an uncommitted hide-name checkbox in Score Properties. Update this handoff when a feature is completed or priorities change.

## Product Snapshot

Notula is an early-stage desktop music notation editor built with Rust, Tauri 2, Svelte 5, and TypeScript. The current renderer uses SVG drawing primitives and Bravura/SMuFL notation glyphs.

Implemented capabilities include score setup and properties, instruments and multiple staves, note/rest/chord entry, multiple voices, duration and pitch editing, articulations, ties, ordinary and phrase slurs, key/time signatures and pickup measures, undo/redo, soundfont playback, PDF/PNG/SVG export, and MusicXML import/export.

## Recent Completed Work

- Slurs, phrase slurs, and ties fit their notes (checked with `files/Deutsche Messe.notula`, Akk. 2 m. 5 and Akk. 1 mm. 21–23):
  - `emit::Arc::clear` bows a slur or phrase slur out until it passes the outer edges (noteheads, stems, beams, rests; `PlacedEvent::{left, right, top, bottom}`) of the events between its ends with `ARC_CLEARANCE`; beyond a maximum depth (3 spaces for slurs, 4 for phrase slurs) both ends move out instead. Slurs avoid their own voice's events, phrase slurs every voice of the staff.
  - Phrase slur ends stay outside the staff or outside a higher (lower) end note.
  - A slur whose end notes both have their stems on its side attaches near the stem ends; with mixed stems both ends stay at the noteheads.
  - A tie on a single note, or on a chord's outermost note on its side, lies just outside the notehead (`TIE_OUTSIDE_HEAD`); ties between inner chord notes stay beside the heads.
  - Staff spacing still estimates slur room (`emit::staff_reach`), so a very deep slur could come close to a neighbouring staff.
- Layout fixes found with the MusicXML samples (`files/MusicXML Samples/Binchois.mxl`):
  - Marks follow the voices: when another voice sounds at the same time (`EventPlan::shares_staff`), articulations and fermatas go on the stem side, so the upper voice's marks are above and the lower voice's below, with the inverted fermata glyph. Single-voice staves keep articulations on the notehead side and fermatas above.
  - Automatic staff spacing per system: before a system is drawn, `Layouter::reaches` measures how far each staff's content reaches (notes, stems, beams, articulations, fermatas, slur ends via `emit::staff_reach`, the dynamics line, metronome marks and tempo text), and `Rows::spaced` moves staves apart to keep `STAFF_CLEARANCE` (1.5 spaces) between them; systems likewise. Staves never come closer than the old fixed distances (9 within a part, 10 between parts). Slur bulges are estimated, not measured. Manual spacing belongs to the page layout topic.
  - Horizontal spacing leaves room for an unbeamed up-stem flag (`smufl::FLAG_UP_WIDTH`), so it no longer runs into the next note or the final barline.
  - Ties follow the voices like marks (`PlacedEvent::ties_below`): with two voices the upper voice's ties bend up and the lower voice's down, instead of crossing. A tie leaving a system from a note near the barline keeps a minimum length.
  - `Part::hide_name` (Serde default false): the name is kept but not printed and reserves no indent. MusicXML imports it from `print-object="no"` on `part-name` and writes it back on export; Score Properties has a "Hide name" checkbox per instrument (`Command::SetPartNameHidden`).
  - A rest that is its voice's only event and fills the measure (`EventPlan::fills_measure`) is centred in the measure, like the placeholder rest of an empty measure.
  - Crossing voices: where an up-stem chord reaches down to a second above a down-stem chord of another voice at the same onset, or lower, it moves right (`EventPlan::shift`), so neither stem runs through the other voice's noteheads: by a whole notehead where the heads are a second or less apart, otherwise by 0.6 of one so the chords still read as one beat. Dots of the other voice's chords at that onset move after the moved chord. A unison of two single notes keeps sharing its head.
  - The final measure adds the width of its double barline (`FINAL_BARLINE_ROOM`), so the last note keeps the usual room before it.
  - `src-tauri/examples/render.rs` renders a `.notula`, `.musicxml`, or `.mxl` file to PNG, SVG, or PDF for checking layout: `cargo run --manifest-path src-tauri/Cargo.toml --example render -- <score> <output> [width]` (the app crate's `export` module is public for it).
- Multi-selection, step 4: copy, cut, and paste (`notula_model::clipboard`, Tauri `copy_selection`, `cut_selection`, `paste`), as planned in [Planned: copy and paste](#planned-copy-and-paste). Ctrl+C/X/V or the Selection section's Copy, Cut, and Paste buttons; with only a single note selected, it becomes a selection first. The clipboard lives in the Rust `Session` and survives opening another score. Paste overwrites from the cursor or the selection's start, splits notes at barlines into tied notes, appends measures past the end, and selects what was pasted. The per-measure voice rewriting is shared with shift in `crates/notula-model/src/rewrite.rs`. A missing target voice is added only to the measures pasted into, with the new single-measure `Command::PushVoice`/`PopVoice`, matching how entered voices usually exist in single measures.
- Multi-selection, step 3: shifting a selection in time (`notula_model::shift::shift_selection`, Tauri `shift_selection`). Ctrl+←/→ or the Selection section's "◀ Earlier" / "Later ▶" buttons move the selected notes by the input bar's note value (shown as "by 1/8"); with only a single note selected, it becomes a selection first. Per selected staff and voice the selected events move as one block, only through unmarked rests; the time left behind becomes rest; notes may move into a neighbouring measure but not straddle a barline; ties at the block's edges are removed; if any voice is blocked, nothing moves and the footer says why (footer messages now take priority over key hints). One undo step; the selection and cursor move with the notes. The model rewrites only the affected measures in one batch and keeps a measure that was empty empty when it holds only rests again.
- Selections work like text selections. `Selection` stores two score positions (anchor and focus, in ticks from the start) rather than notes; equal positions mean a cursor that selects nothing. The cursor follows the moving edge (after growing to whole notes): Shift+←/→ from the cursor selects the next or previous note and moves the cursor to the selection's end, Shift+Ctrl+←/→ stops at barlines, and a dragged box, Shift+click, and Ctrl+A put the cursor at their end. Plain ←/→ first collapses a selection to its start or end. Selecting notes turns note input off; turning note input on collapses a selection to its end.
- Shift+↑/↓ without a horizontal selection spans the cursor over several staves without selecting notes. Note input then enters each note or rest into every spanned staff (same letter, octave per staff as usual) as one undo step, via `Document::transaction` and `edit::enter_in_staves`; the entry commands take an `also` list of staves. Moving in time keeps the staves; Alt+↑/↓, Esc, or a click returns to one staff. While several staves are spanned, the cursor's staff is not coloured, and the cursor and input highlight reach across all of them.
- Multi-selection, step 2 (per-note operations). With a range selection, Delete turns the selected notes into rests, ↑/↓ transposes them by a semitone and Ctrl+↑/↓ by an octave (in select mode too), and the articulation buttons add an articulation to every selected note, or remove it when all have it (the button shows active then); a fermata may also go on rests. Each operation is one undoable step and keeps the selection; transposing is all or nothing when a pitch would leave the range. Model: `edit::delete_events`, `transpose_events`, `toggle_articulation_events`, `common_articulations`, `note_count` (one pass over the score); Tauri: `edit_selection`, and `select` now reports the note count and common articulations. Accidentals, tie, and the single-note dynamic control are disabled while a selection is active.
- Multi-selection, step 1 (model, display, controls), as planned in [Planned: multi-selection](#planned-multi-selection). `notula_model::selection` holds the range selection (anchor and focus spans, staves between anchor and focus staff, optional voice filter), keyboard steps, select all, Shift+click extension, and resolving to selected events and per-measure spans after growing to whole notes; `notula_layout::box_selection` maps a dragged box to a selection. One Tauri command, `select`, handles all requests. The UI keeps it in `editor.rangeSelection` (re-resolved after every edit), highlights selected notes, draws one box per measure across the selected staves, shows a Selection section with summary, voice filter, and Clear in the side panel, and lists the keys in the footer. Shift+click extends only in select mode, since in note input it adds a chord tone. No operations use the selection yet.
- Dynamics and hairpins can be edited, are engraved, and round-trip through MusicXML. They stay on the events of one staff (so instruments, and staves of one instrument, can differ), matching MusicXML. Playback ignores them for now.
  - Model: `Markings::dynamic`, plus `hairpin_start: Option<Hairpin>` and `hairpin_stop: bool` like slurs; `Hairpin` is `Crescendo`/`Diminuendo`. `notula_model::edit::hairpins()` pairs them per staff and voice: at an event, a stop closes the open hairpin before a start opens the next; with none open, a start comes first. So one event can hold a whole hairpin (one note) or end one and start the next ("< >"). Files with the older single `hairpin` field (`Crescendo`/`Diminuendo`/`End`) still load; an old `End` becomes a stop on that event, so such a hairpin now runs through that event instead of ending at it.
  - Editing: `set_dynamic`, `toggle_hairpin(start, end, kind)` (same voice, may be one event, overlaps rejected except touching at a shared event, a new hairpin from the same start replaces the old), `remove_hairpin(event)`, `hairpin_at(event)`. Toolbar: a dynamic select for the selected note or rest, Cresc./Dim. modes that work like the slur tool (start note, then end note, Esc cancels), and Remove hairpin.
  - Engraving (`crates/notula-layout/src/volume.rs`): one dynamics line per staff and system below the staff, lowered for low notes and stems but kept above the next staff; dynamics centred under the notehead and given horizontal room; hairpins start after a dynamic at their start, stop before a dynamic at their end, meet in the middle of a shared event, and continue across system breaks with matching openings. Tempo directions move below the bottom staff's dynamics line.
  - MusicXML: dynamics as before; wedges are written with `number` and `<voice>`, the start before the first event and the stop after the last; chained hairpins alternate numbers because they overlap in MusicXML time. Import pairs wedges by staff and number across measures: a start marks the first event at or after it (in its voice if given), a stop marks the last event of the start's voice beginning before it. Unpaired wedges are reported.
- The starting tempo is an ordinary tempo change at measure 1, tick 0 (`Score::initial_tempo()`); without one, playback uses quarter note = 120. There is no separate `Score::tempo` any more: files that stored the old default-tempo field load with it as a printed start mark. The New Score dialog sets the starting tempo and whether it is printed (defaults: printed quarter note = 120; `ScoreSetup` fields default the same way). Score Properties edits the start mark's tempo and whether it is printed, and only creates or changes the mark when one of those differs. Both dialogs share `ui/src/lib/TempoFields.svelte`.
- Tempo directions (`accel.`, `rit.`, `rall.`, `a tempo`, `rubato`) are stored once per score at a measure/tick position. The toolbar can add, replace, and remove a direction at the current cursor. It renders below each instrument part at the shared horizontal position, so changing it from any staff changes the score-wide annotation. Directions are intentionally ignored by playback for now.
- Direction positions follow measure insertions and removals; undo/redo restores edits. Validation rejects positions outside a measure and duplicate directions at one position.
- Tempo changes can be `hidden`: they play but are not printed. The toolbar keeps a mark's hidden flag when editing it and creates printed marks; the start mark's visibility is set in Score Properties.
- Tempo changes (metronome marks) are stored once per score at a measure/tick position, like tempo directions, and **do** affect playback: `notula_playback::TempoMap` converts ticks to seconds across changes. They render as a SMuFL metronome note plus "= N" above each instrument part, lifted clear of notes under the mark. The toolbar sets, edits, and removes the mark at the cursor. A mark at a measure's end and the next measure's start share a tick; the later one wins.
- `Tempo` has an optional `beat_unit_dots` (Serde default 0) so dotted beats such as dotted quarter = 60 are representable; the properties dialog and toolbar offer common dotted beat units.
- Score-wide marks block time-signature or pickup-length changes that would leave a mark past the end of its measure.
- MusicXML round-trips tempo: tempo changes (including the start mark) are exported as metronome marks with `<sound tempo>`, hidden ones with `print-object="no"`; tempo directions as `<words>`. All are written once, in the first part, positioned with `<offset sound="yes">`. Import reads `<metronome>` (including approximate "c. 72" values), direction or measure-level `<sound tempo>`, and recognized direction words from any part, deduplicated by position, preferring a printed metronome over a hidden one over a bare sound tempo. Bare sound tempos and hidden metronomes become hidden tempo changes. Other words such as "Allegro" are still reported as unsupported.

## Suggested Next Work

1. **Next: extended annotations** that are still missing: instrument-specific marks such as accordion registration symbols and piano sustain pedal marks (Ped. and release, or pedal lines), and free text annotations such as "div." or playing and character directions ("vivace", "dolce"). Also horizontal brackets over or under a group of notes, as MusicXML `<bracket>` directions (start/stop, `line-end` hooks, solid or dashed, numbered for overlaps); early-music editions such as the Binchois sample use them to mark ligatures of the original source, and import currently reports "direction: bracket" as unsupported. This includes tempo text such as "Allegro" (with an optional metronome) as a score-wide mark, which MusicXML import currently drops. Decide per kind whether it belongs to an event, a staff, or the score (see `AGENTS.md`), and which ones playback should honour (pedal likely, text not).
2. Repeats and other barline types (repeat barlines, possibly endings and double barlines). They affect playback order as well as notation and MusicXML; MusicXML import currently reports barline styles and repeats as unsupported.
3. Polish volume marks: automatic staff spacing now leaves room for the dynamics line between staves; check grand staves for remaining near-collisions. Also consider text dynamics and hairpin variants ("cresc.", "dim.", niente circles), dynamics shared by both staves of one instrument, and keyboard shortcuts.
4. Make rit./accel./rall. affect playback; see [Planned: tempo transitions](#planned-tempo-transitions).
5. Playback, to be designed as one larger topic later: dynamics and hairpins (velocity and gradual volume), tempo transitions, and an option to start playback at the current cursor position instead of the beginning of the score. None of this is implemented yet; the first dynamics change is notation, editing, and MusicXML only.
6. PNG/PDF export loads only Bravura into usvg, so plain text items (part names, tempo text) likely fall back to an unavailable font there. Verify and load or embed a text font if needed.
7. Explicit stem directions, MusicXML-compatible: an optional stem direction per event (automatic, up, down), imported from and exported to `<stem>`, with a command to flip stems. With it, two voices carrying identical notes and the same stem direction overlap exactly as written, as in the Binchois sample (lower staff, mm. 2–8: one voice per instrument, Cantus 2 and Tenor, with identical notes, stems, beams, and positions). Notula currently forces opposite stems on simultaneous voices and draws both, so unisons show two stems; import reports explicit stem directions as unsupported.

## Future Topics

Larger features the user wants eventually, roughly in order of interest. None is designed yet; each needs its own design pass before implementation.

- **Page layout**, especially for PDF export: additional freely positioned text, staff and system layout hints (system and page breaks, spacing), and whatever else comes up when the feature is designed.
- **Single-instrument parts:** print a sheet for just one instrument of a multi-instrument score. Whether this is a filter or layout option on the same score or an export to a new score is open.
- **More import and export formats**, mainly as interchange to bring music in from other sources: LilyPond and MIDI.
- **Drum and percussion notation:** percussion staves, notehead shapes, and the mapping from staff positions to instruments (and to playback sounds).
- **Accordion bass notation:** enter bass patterns so that one key press is one chord, written as letters (as is common for accordion) rather than full chord notes. The input idea may help other instruments too.
- **Guitar tablature** (possible idea, not certain): tab staves, possibly linked to a normal staff. How this fits meaningfully into a notation editor still needs thought.
- **Score scanning (optical music recognition):** import scores from printed paper or images. By far the most complex topic and likely needs external tools or libraries; a large usability gain if it works.

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

## Planned: Multi-Selection

Agreed model, informed by MuseScore's range and list selections, Sibelius's passage selection, and Dorico's simpler selected-or-not model. None of these handles ranges that cut through notes cleanly; Dorico's developers call partial-note copying "complex".

- **A selection is staves × time × voices:** a contiguous range of staves, a time interval between two rhythmic positions, and a voice filter. One voice is one staff with a voice filter; one instrument is its staves; the whole score is all staves. Score-wide marks (tempo) are not part of a selection yet.
- **Membership:** in each selected staff and voice, an event belongs to the selection when it starts inside the interval. With notes of different lengths in different staves, a long note starting inside is included.
- **Edges cut through notes:** the selection grows so its end covers every included note. The highlighted box therefore always contains whole notes, and can become wider than the position extended to. (Splitting with ties at the edge was considered and left out.)
- **Voice filter:** selections include all voices by default; an explicit filter in the side panel (All / Voice 1 / Voice 2 …) narrows them, like MuseScore's selection filter.
- **Keyboard:** Shift+←/→ moves the selection end by one note in the staff and voice where the selection started; Shift+Ctrl+←/→ by a measure; Shift+↑/↓ adds or removes staves; Ctrl+A selects everything; Esc clears it. Starting from a selected note or the cursor, the first Shift+arrow creates the selection.
- **Mouse:** Shift+click extends the selection to the clicked note; dragging on empty score space draws a selection box that snaps to staves and rhythmic positions. Dragging on notes (to move them) is left for later.
- **Operations:**
  - Per-note operations use the events that start inside: transpose, delete (notes become rests), articulations, dynamics.
  - Time operations use the grown extent with whole notes: shift, and later copy and paste.
  - A list selection of scattered notes (Ctrl+click) may come later, for per-note operations only.
- **Display:** a translucent box over the selected staves and time span, with selected notes highlighted as the single selection is today.
- **Status:** step 1 (model, display, keyboard and mouse controls, voice filter), step 2 (delete, transpose, articulations on selections), and step 3 (shift) are done. Step 4 (copy and paste) is done too; a list selection of scattered notes and mouse dragging remain possible later additions.

## Planned: Copy and Paste

Agreed behaviour, completing multi-selection (the time operations use the grown span with whole notes). Implemented; see Recent Completed Work.

- **Copy (Ctrl+C):** the clipboard takes the selection's grown span: the selected staves in score order, the voices the voice filter allows, and in each the events that start inside, with their marks and their offsets from the span start. Slurs, phrase slurs, and hairpins with only one end inside the span lose that end in the copy, and a tie leaving the span is removed. Score-wide marks (tempo) are not copied.
- **Cut (Ctrl+X):** copy, then the existing selection delete (notes become rests), as one undo step.
- **Clipboard:** kept in the Rust `Session` for the app run, so it can be pasted into another opened score. The system clipboard (for example as MusicXML) may come later.
- **Paste target (Ctrl+V):** the cursor, or the start of a range selection; the clipboard's length decides how far the paste reaches, not the selection's. The number of target staves must match: a clipboard with several staves pastes only when the cursor or selection spans exactly as many staves (Shift+↑/↓), otherwise nothing happens and the footer says why. A single-staff clipboard pastes into the cursor's staff, or into every staff of a spanned cursor or selection, as note entry does.
- **Voices:** clipboard voice n goes into voice n; a clipboard holding one voice (voice filter) goes into the cursor's voice. Voices the clipboard lacks stay unchanged in the target; a missing target voice is added in the pasted measures only.
- **Overwriting:** in each target staff and voice, the paste range is replaced. A note cut at the start of the range keeps its beginning, shortened; what sticks out at its end becomes rest. Slurs, phrase slurs, hairpins, and ties left with one end by the overwrite are removed.
- **Barlines:** pasted notes that would cross a barline (other beat position or time signature) are split there into tied notes; rests are split plainly. Remaining time is written with the largest rest values that fit, as when entering notes.
- **Score end:** a paste that runs past the last measure appends measures with the last measure's time and key signature, in the same undo step.
- **Result:** one undoable step; afterwards the pasted range is selected, so it can be transposed or shifted right away (→ continues after it).
- **Controls:** Ctrl+C/X/V, and Copy/Cut/Paste buttons in the side panel's Selection section.
- **Implementation sketch:** a `notula_model::clipboard` module with a `Clip` (staff count, length, per staff and voice the placed events) and `copy`/`paste` functions; extract the per-measure voice rewriting of `shift.rs` (placed events → measure contents with rest filling, keeping empty measures empty) into a shared helper used by both, extended with splitting into tied notes at barlines. Tauri commands `copy_selection`, `cut_selection`, `paste`, with model tests for each rule above.

## Planned: Horizontal Note Shifting

Agreed behaviour for moving the selected note or chord earlier or later in time. Implemented as multi-selection step 3 (see Recent Completed Work); mouse dragging is still to come. It builds on multi-selection: a selection moves as a block per staff and voice by the rules below, all selected voices together, and if any of them is blocked, nothing moves.

- **What moves:** the selected event in its own voice, with everything attached to it (articulations, dynamic, slur, phrase slur, and hairpin starts and ends). Other voices and staves stay unchanged. The time it leaves becomes rest. Rests themselves are not moved.
- **Step:** the note value selected in the input bar, dot included. Choosing a smaller value gives finer steps.
- **Only through rests:** the time the note moves into must be rest in its voice. A neighbouring note blocks the move with a footer message; there is no partial move. Because notes never pass each other, slurs, hairpins, and ties keep their order.
- **Barlines:** the note may move into the previous or next measure when it fits completely into that measure's rests. A move that would make the note straddle a barline is blocked for now; splitting it into tied notes is a possible later extension.
- **Edge cases:**
  - Ties are removed when the moved note no longer touches its partner (its own tie to the next note, and a tie into it from the previous note).
  - A rest that would be overwritten while carrying marks (dynamic, hairpin start or end, fermata) blocks the move, so marks are never lost silently.
  - An additional voice that does not exist or has ended (explicit voice end) in the target measure blocks the move.
  - Pickup measures use their shorter length.
  - Remaining rest time is rewritten with the largest rest values that fit, as when entering notes.
- **Result:** one undoable step; the selection and cursor follow the moved note.
- **Controls:** Ctrl+←/→ in select and note input mode (Shift+←/→ stays reserved for multi-selection), and "◀ Earlier" / "Later ▶" buttons in the side panel showing the step (for example "by 1/8"), disabled when nothing is selected. Mouse dragging comes later, designed together with drag selection.
- **Later extension to multi-selection:** a selection within one voice moves as a block by the same rules, applied at its outer edges.
- **Implementation sketch:** a model edit such as `edit::shift_event(doc, event, forward, step)` that works on the voice's events across the current and neighbouring measures and applies all changes as one `Command::Batch`, with model tests for each rule above; a Tauri command and UI wiring follow the existing pattern.

## Useful Code Map

- Score model, initial tempo, and score-wide tempo directions and changes: `crates/notula-model/src/score.rs`
- Reversible score mutations and measure position maintenance: `crates/notula-model/src/command.rs`
- Range selections: `crates/notula-model/src/selection.rs`; time operations: `shift.rs` and `clipboard.rs`, sharing the voice rewriting in `rewrite.rs`
- Event-local dynamics, hairpins, and other markings: `crates/notula-model/src/markings.rs`; edit helpers and hairpin pairing: `crates/notula-model/src/edit.rs`
- Dynamics and hairpin engraving: `crates/notula-layout/src/volume.rs`
- Shared staff layout, measure columns, and tempo-direction engraving: `crates/notula-layout/src/measure.rs`, `crates/notula-layout/src/lib.rs`
- Drawing primitives and SVG frontend rendering: `crates/notula-layout/src/render.rs`, `ui/src/lib/ScoreView.svelte`
- Tauri API and command registration: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Frontend bridge and editor state: `ui/src/lib/api.ts`, `ui/src/lib/editor.svelte.ts`
- New score and score properties dialogs, shared starting-tempo fields: `ui/src/lib/NewScoreDialog.svelte`, `ui/src/lib/ScorePropertiesDialog.svelte`, `ui/src/lib/TempoFields.svelte`; backend: `src-tauri/src/setup.rs`
- Toolbar (top bar with File/playback menus, undo/redo, note input, zoom, panel toggle; input bar with note values, accidentals, articulations, tie right above the score): `ui/src/lib/Toolbar.svelte`; side panel with sections for dynamics and hairpins, lines (slurs and phrase slurs), tempo (metronome mark and tempo direction), voice, and measure: `ui/src/lib/Sidebar.svelte`. A new hairpin, slur, or phrase slur can start where the selected note has none or where one starts (it then replaces that one); inside or at the end of one, only Remove is offered. The selection info from `event_articulations` reports these positions (`edit::SpanPosition`); shared pieces: `Icon.svelte`, `MenuButton.svelte`, `focus.ts` (hands keyboard focus back to the score after button clicks and dropdown choices)
- MusicXML import/export: `crates/notula-format/src/musicxml.rs`
- Playback event generation, tempo map, and audio scheduling: `crates/notula-playback/src/lib.rs`, `src-tauri/src/playback.rs`

## Working In A New Session

Start with a concrete task and ask the agent to read `AGENTS.md` and this file, inspect the worktree, and continue from the current code rather than assuming previous-session context. State the desired behavior, scope boundaries, and acceptance criteria. For example:

> Read `AGENTS.md` and `docs/PROJECT_STATUS.md`, inspect the current worktree, and implement [specific behavior]. Keep [scope boundary] unchanged. Acceptance criteria: [observable outcomes]. Add focused tests, run the relevant checks, and update the project status if the milestone changes.

For a larger task, split independent research from implementation, but keep one session responsible for integrating model, UI, layout, and validation. Ask for a review or commit separately; commits should only be made when explicitly requested.