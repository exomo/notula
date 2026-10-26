import { invoke } from "@tauri-apps/api/core";
import type { RenderList } from "./render";

export type Step = "C" | "D" | "E" | "F" | "G" | "A" | "B";
export type NoteValue =
  | "Whole"
  | "Half"
  | "Quarter"
  | "Eighth"
  | "Sixteenth"
  | "ThirtySecond"
  | "SixtyFourth";
export type Articulation =
  | "Staccato"
  | "Staccatissimo"
  | "Tenuto"
  | "Accent"
  | "Marcato"
  | "Fermata";
export type SlurPlacement = "auto" | "above" | "below";
export type Dynamic =
  | "Ppp"
  | "Pp"
  | "P"
  | "Mp"
  | "Mf"
  | "F"
  | "Ff"
  | "Fff"
  | "Fp"
  | "Sf"
  | "Sfz"
  | "Rfz";
export type Hairpin = "Crescendo" | "Diminuendo";
export type TempoDirection = "accelerando" | "ritardando" | "rallentando" | "a_tempo" | "rubato";

/** Metronome setting; `beat_unit_dots` is omitted when the beat is undotted. */
export interface Tempo {
  beat_unit: NoteValue;
  beat_unit_dots?: number;
  units_per_minute: number;
}

export interface TempoChangeMark {
  measure: number;
  tick: number;
  tempo: Tempo;
  /** Plays without being printed; omitted when false. */
  hidden?: boolean;
}

export interface Pitch {
  step: Step;
  alter: number;
  octave: number;
}

export interface Duration {
  value: NoteValue;
  dots: number;
}

export interface EventArticulations {
  articulations: Articulation[];
  is_rest: boolean;
  tied: boolean;
  tie_possible: boolean;
  dynamic: Dynamic | null;
  /** Whether the event starts, ends, or lies inside a hairpin. */
  in_hairpin: boolean;
}

export interface Cursor {
  staff: number;
  measure: number;
  voice: number;
  tick: number;
}

export type TimeSignatureDisplay = "numeric" | "common" | "alla_breve";

export interface Status {
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
  file_name: string | null;
  title: string;
}

/** A null cursor or selection means "unchanged", except after undo, redo, new and open. */
export interface Edited {
  status: Status;
  cursor: Cursor | null;
  selected: number | null;
  inserted: number | null;
  undo_cursor: Cursor | null;
  restored_insert: boolean;
}

export interface OpenScoreResult {
  edited: Edited;
  unsupported: string[];
}

export interface PitchEdited {
  edited: Edited;
  pitch: Pitch;
}

export interface HitInfo {
  cursor: Cursor;
  pitch: Pitch;
  chord_pitch: Pitch | null;
  note: number | null;
}

export interface CursorPosition {
  cursor: Cursor;
  selected: number | null;
}

export interface Instrument {
  id: string;
  name: string;
  short_name: string;
  staves: number;
}

export interface ScoreSetup {
  title: string;
  composer: string;
  instruments: { preset_id: string; staves: number }[];
  time_numerator: number;
  time_denominator: number;
  key_fifths: number;
  measures: number;
  /** Length in ticks of a shortened first measure (anacrusis), or null for none. */
  pickup: number | null;
  /** Tempo at the start of the score, and whether its mark is printed. */
  tempo: Tempo;
  tempo_visible: boolean;
}

export interface ScorePart {
  id: number;
  name: string;
  short_name: string;
  staves: number;
}

export interface ScoreProperties {
  title: string;
  composer: string;
  /** Tempo at the start of the score, and whether its mark is printed. */
  tempo: Tempo;
  tempo_visible: boolean;
  parts: ScorePart[];
}

export interface ScorePropertiesUpdate {
  title: string;
  composer: string;
  tempo: Tempo;
  tempo_visible: boolean;
  remove_part_ids: number[];
  instruments: { part_id: number; name: string; short_name: string; preset_id: string | null; staves: number }[];
  add_instruments: { preset_id: string; name: string; short_name: string; staves: number }[];
}

export interface PlaybackStatus {
  playing: boolean;
  instrument: string | null;
  cursor: Cursor | null;
}

export const api = {
  currentState: () => invoke<Edited>("current_state"),
  renderScore: (width: number) => invoke<RenderList>("render_score", { width }),
  hitTest: (width: number, x: number, y: number, voice: number) =>
    invoke<HitInfo | null>("hit_test", { width, x, y, voice }),
  selectEvent: (event: number) => invoke<Cursor | null>("select_event", { event }),
  moveCursor: (cursor: Cursor, selected: number | null, forward: boolean, movement = "step") =>
    invoke<CursorPosition | null>("move_cursor", { cursor, selected, forward, movement }),
  enterNote: (cursor: Cursor, pitch: Pitch, duration: Duration, holdCursor = false) =>
    invoke<Edited>("enter_note", { cursor, pitch, duration, holdCursor }),
  enterStep: (
    cursor: Cursor,
    step: Step,
    duration: Duration,
    holdCursor = false,
    alter: number | null = null,
  ) => invoke<Edited>("enter_step", { cursor, step, duration, holdCursor, alter }),
  enterRest: (cursor: Cursor, duration: Duration) =>
    invoke<Edited>("enter_rest", { cursor, duration }),
  setDuration: (event: number, duration: Duration) =>
    invoke<Edited>("set_duration", { event, duration }),
  toggleArticulation: (event: number, articulation: Articulation) =>
    invoke<Edited>("toggle_articulation", { event, articulation }),
  toggleTie: (event: number) => invoke<Edited>("toggle_tie", { event }),
  toggleSlur: (start: number, end: number, placement: SlurPlacement) =>
    invoke<Edited>("toggle_slur", { start, end, placement }),
  togglePhraseSlur: (start: number, end: number, placement: SlurPlacement) =>
    invoke<Edited>("toggle_phrase_slur", { start, end, placement }),
  setDynamic: (event: number, dynamic: Dynamic | null) =>
    invoke<Edited>("set_dynamic", { event, dynamic }),
  toggleHairpin: (start: number, end: number, kind: Hairpin) =>
    invoke<Edited>("toggle_hairpin", { start, end, kind }),
  removeHairpin: (event: number) => invoke<Edited>("remove_hairpin", { event }),
  tempoDirectionAt: (measure: number, tick: number) =>
    invoke<TempoDirection | null>("tempo_direction_at", { measure, tick }),
  setTempoDirection: (measure: number, tick: number, direction: TempoDirection | null) =>
    invoke<Edited>("set_tempo_direction", { measure, tick, direction }),
  tempoChangeAt: (measure: number, tick: number) =>
    invoke<TempoChangeMark | null>("tempo_change_at", { measure, tick }),
  setTempoChange: (measure: number, tick: number, tempo: Tempo | null, hidden: boolean) =>
    invoke<Edited>("set_tempo_change", { measure, tick, tempo, hidden }),
  eventArticulations: (event: number) =>
    invoke<EventArticulations | null>("event_articulations", { event }),
  deleteEvent: (event: number) => invoke<Edited>("delete_event", { event }),
  transposeSemitones: (event: number, semitones: number) =>
    invoke<Edited>("transpose_semitones", { event, semitones }),
  transposeChordPitch: (event: number, pitch: Pitch, semitones: number) =>
    invoke<PitchEdited>("transpose_chord_pitch", { event, pitch, semitones }),
  transposeOctaves: (event: number, octaves: number) =>
    invoke<Edited>("transpose_octaves", { event, octaves }),
  setAlter: (event: number, alter: number) => invoke<Edited>("set_alter", { event, alter }),
  addNoteAbove: (event: number, step: Step, alter: number | null = null) =>
    invoke<Edited>("add_note_above", { event, step, alter }),
  addPitchToChord: (event: number, pitch: Pitch) =>
    invoke<Edited>("add_pitch_to_chord", { event, pitch }),
  removePitchFromChord: (event: number, pitch: Pitch) =>
    invoke<Edited>("remove_pitch_from_chord", { event, pitch }),
  togglePitchInChord: (event: number, pitch: Pitch) =>
    invoke<Edited>("toggle_pitch_in_chord", { event, pitch }),
  insertMeasure: (measure: number, before: boolean, cursor: Cursor) =>
    invoke<Edited>("insert_measure", { measure, before, cursor }),
  removeMeasure: (measure: number, cursor: Cursor) =>
    invoke<Edited>("remove_measure", { measure, cursor }),
  addVoice: (staff: number, measure: number) =>
    invoke<Edited>("add_voice", { staff, measure }),
  removeVoice: (staff: number, measure: number, voice: number) =>
    invoke<Edited>("remove_voice", { staff, measure, voice }),
  endVoice: (staff: number, measure: number, voice: number, tick: number) =>
    invoke<Edited>("end_voice", { staff, measure, voice, tick }),
  setTimeSignature: (
    measure: number,
    numerator: number,
    denominator: number,
    display: TimeSignatureDisplay,
  ) => invoke<Edited>("set_time_signature", { measure, numerator, denominator, display }),
  setKeySignature: (measure: number, fifths: number) =>
    invoke<Edited>("set_key_signature", { measure, fifths }),
  togglePickup: (measure: number, cursor: Cursor | null) =>
    invoke<Edited>("toggle_pickup", { measure, cursor }),
  undo: (
    cursor: Cursor | null,
    selected: number | null,
    restoreCursor: Cursor | null,
    insertedEvent: number | null,
  ) => invoke<Edited>("undo", { cursor, selected, restoreCursor, insertedEvent }),
  redo: (cursor: Cursor | null, selected: number | null) =>
    invoke<Edited>("redo", { cursor, selected }),
  instrumentPresets: () => invoke<Instrument[]>("instrument_presets"),
  scoreProperties: () => invoke<ScoreProperties>("score_properties"),
  updateScoreProperties: (update: ScorePropertiesUpdate) =>
    invoke<Edited>("update_score_properties", { update }),
  newScore: (setup: ScoreSetup) => invoke<Edited | null>("new_score", { setup }),
  openScore: () => invoke<OpenScoreResult | null>("open_score"),
  saveScore: (saveAs: boolean) => invoke<Status | null>("save_score", { saveAs }),
  exportScore: (format: "pdf" | "png" | "svg" | "musicxml") =>
    invoke<string[] | null>("export_score", { format }),
  selectSoundfont: () => invoke<string | null>("select_soundfont"),
  restoreSoundfont: () => invoke<string | null>("restore_soundfont"),
  playScore: () => invoke<void>("play_score"),
  stopPlayback: () => invoke<void>("stop_playback"),
  playbackStatus: () => invoke<PlaybackStatus>("playback_status"),
};
