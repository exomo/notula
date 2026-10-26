import {
  api,
  type Cursor,
  type Duration,
  type Articulation,
  type Dynamic,
  type Hairpin,
  type Edited,
  type EventArticulations,
  type NoteValue,
  type Pitch,
  type PlaybackStatus,
  type SlurPlacement,
  type Status,
  type Step,
  type TimeSignatureDisplay,
  type Tempo,
  type TempoChangeMark,
  type TempoDirection,
} from "./api";
import type { MeasureArea } from "./render";

/** Index + 1 is the number key that selects the value. */
export const NOTE_VALUES: NoteValue[] = [
  "SixtyFourth",
  "ThirtySecond",
  "Sixteenth",
  "Eighth",
  "Quarter",
  "Half",
  "Whole",
];

/** Beat units offered for metronome marks, keyed as "value:dots" for select controls. */
export const TEMPO_BEATS: { key: string; label: string }[] = [
  { key: "Whole:0", label: "1/1 note" },
  { key: "Half:1", label: "Dotted 1/2 note" },
  { key: "Half:0", label: "1/2 note" },
  { key: "Quarter:1", label: "Dotted 1/4 note" },
  { key: "Quarter:0", label: "1/4 note" },
  { key: "Eighth:1", label: "Dotted 1/8 note" },
  { key: "Eighth:0", label: "1/8 note" },
  { key: "Sixteenth:0", label: "1/16 note" },
  { key: "ThirtySecond:0", label: "1/32 note" },
  { key: "SixtyFourth:0", label: "1/64 note" },
];

export function tempoBeatKey(tempo: Tempo): string {
  return `${tempo.beat_unit}:${tempo.beat_unit_dots ?? 0}`;
}

export function tempoFromBeatKey(key: string, unitsPerMinute: number): Tempo {
  const [beatUnit, dots] = key.split(":");
  return {
    beat_unit: beatUnit as NoteValue,
    beat_unit_dots: Number(dots),
    units_per_minute: unitsPerMinute,
  };
}

const SIDEBAR_KEY = "notula.sidebarOpen";

/** Remembered per browser profile; storage can be unavailable, so it is only a preference. */
function loadSidebarOpen(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_KEY) !== "false";
  } catch {
    return true;
  }
}

const MIN_ZOOM = 4;
const MAX_ZOOM = 24;

interface InsertMarker {
  event: number;
  cursor: Cursor;
}

class Editor {
  status = $state<Status>({
    can_undo: false,
    can_redo: false,
    dirty: false,
    file_name: null,
    title: "",
  });
  cursor = $state<Cursor | null>(null);
  selected = $state<number | null>(null);
  eventArticulations = $state<EventArticulations | null>(null);
  selectedTone = $state<{ event: number; pitch: Pitch; y: number } | null>(null);
  slurMode = $state(false);
  slurStart = $state<number | null>(null);
  phraseSlurMode = $state(false);
  phraseSlurStart = $state<number | null>(null);
  hairpinMode = $state<Hairpin | null>(null);
  hairpinStart = $state<number | null>(null);
  slurPlacement = $state<SlurPlacement>("auto");
  tempoDirectionAt = $state<TempoDirection | null>(null);
  tempoChangeAt = $state<TempoChangeMark | null>(null);
  inputMode = $state(false);
  entryAlter = $state<number | null>(null);
  duration = $state<Duration>({ value: "Quarter", dots: 0 });
  zoom = $state(8);
  /** Width of the score view in staff spaces. */
  layoutWidth = $state(0);
  /** Incremented whenever the score changed, to trigger a new layout. */
  revision = $state(0);
  message = $state<string | null>(null);
  newScoreOpen = $state(false);
  scorePropertiesOpen = $state(false);
  timeSignatureOpen = $state(false);
  keySignatureOpen = $state(false);
  sidebarOpen = $state(loadSidebarOpen());
  /** Where measures are on screen, as of the latest layout. */
  areas = $state<MeasureArea[]>([]);
  playbackPlaying = $state(false);
  playbackInstrument = $state<string | null>(null);
  playbackCursor = $state<Cursor | null>(null);
  private noteEditor: SVGSVGElement | null = null;
  private playbackPoll: ReturnType<typeof setInterval> | null = null;
  private insertHistory: InsertMarker[] = [];
  private articulationRequest = 0;
  private tempoDirectionRequest = 0;
  private tempoChangeRequest = 0;

  zoomIn() {
    this.zoom = Math.min(MAX_ZOOM, this.zoom + 1);
  }

  zoomOut() {
    this.zoom = Math.max(MIN_ZOOM, this.zoom - 1);
  }

  get canZoomIn() {
    return this.zoom < MAX_ZOOM;
  }

  get canZoomOut() {
    return this.zoom > MIN_ZOOM;
  }

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    try {
      localStorage.setItem(SIDEBAR_KEY, String(this.sidebarOpen));
    } catch {
      // The panel still toggles; it just will not be remembered.
    }
  }

  setNoteEditor(element: SVGSVGElement | null) {
    this.noteEditor = element;
  }

  focusNoteEditor() {
    this.noteEditor?.focus({ preventScroll: true });
  }

  get canDeleteMeasure() {
    return this.cursor !== null && new Set(this.areas.map((area) => area.measure)).size > 1;
  }

  get voiceCount() {
    const cursor = this.cursor;
    return cursor
      ? this.areas.find((area) => area.staff === cursor.staff && area.measure === cursor.measure)?.voice_count ?? 1
      : 0;
  }

  get canEndVoice() {
    const cursor = this.cursor;
    if (!cursor || cursor.voice === 0) return false;
    return this.voiceCount > cursor.voice;
  }

  private apply(edited: Edited | null, exactSelection = false) {
    if (!edited) return;
    this.status = edited.status;
    if (edited.cursor) this.cursor = edited.cursor;
    if (exactSelection || edited.selected !== null) {
      if (this.selected !== edited.selected) this.selectedTone = null;
      this.selected = edited.selected;
    }
    this.revision++;
  }

  private async run<T>(action: () => Promise<T>): Promise<T | undefined> {
    this.message = null;
    try {
      return await action();
    } catch (error) {
      this.message = String(error);
      return undefined;
    }
  }

  async init() {
    this.apply((await this.run(() => api.currentState())) ?? null, true);
    const instrument = await this.run(() => api.restoreSoundfont());
    if (instrument) this.playbackInstrument = instrument;
    await this.refreshPlaybackStatus();
  }

  private applyPlaybackStatus(status: PlaybackStatus) {
    this.playbackPlaying = status.playing;
    this.playbackInstrument = status.instrument;
    this.playbackCursor = status.cursor;
    if (!status.playing && this.playbackPoll !== null) {
      clearInterval(this.playbackPoll);
      this.playbackPoll = null;
    }
  }

  private async refreshPlaybackStatus() {
    const status = await this.run(() => api.playbackStatus());
    if (status) this.applyPlaybackStatus(status);
  }

  async selectSoundfont() {
    const name = await this.run(() => api.selectSoundfont());
    if (name) {
      this.playbackInstrument = name;
      this.message = null;
    }
  }

  async playScore() {
    await this.run(() => api.playScore());
    if (this.message) return;
    await this.refreshPlaybackStatus();
    if (this.playbackPoll === null) {
      this.playbackPoll = setInterval(() => void this.refreshPlaybackStatus(), 50);
    }
  }

  async stopPlayback() {
    await this.run(() => api.stopPlayback());
    if (this.message) return;
    await this.refreshPlaybackStatus();
  }

  async exportScore(format: "pdf" | "png" | "svg" | "musicxml") {
    const unsupported = await this.run(() => api.exportScore(format));
    if (unsupported?.length) {
      this.message = `Export omitted unsupported features: ${unsupported.join(", ")}`;
    }
  }

  async selectEvent(id: number) {
    this.selected = id;
    const cursor = await this.run(() => api.selectEvent(id));
    if (cursor) this.cursor = cursor;
  }

  private samePosition(a: Cursor | null, b: Cursor) {
    return a !== null && a.staff === b.staff && a.measure === b.measure && a.voice === b.voice && a.tick === b.tick;
  }

  private targetTone(event: number, pitch: Pitch, clickedY: number, cursor: Cursor) {
    const area = this.areas.find(
      (item) => item.staff === cursor.staff && item.measure === cursor.measure,
    );
    if (!area) return;
    const snappedY = area.y_top + Math.round((clickedY - area.y_top) * 2) / 2;
    this.selectedTone = { event, pitch, y: snappedY };
  }

  private pitchDiatonicIndex(pitch: Pitch) {
    return pitch.octave * 7 + ("CDEFGAB".indexOf(pitch.step));
  }

  private entryPitch(pitch: Pitch) {
    return this.entryAlter === null ? pitch : { ...pitch, alter: this.entryAlter };
  }

  private async enterAtCursor(action: () => Promise<Edited>) {
    const edited = await this.run(action);
    if (!edited) return;
    if (edited.inserted !== null && edited.undo_cursor !== null) {
      this.insertHistory.push({ event: edited.inserted, cursor: edited.undo_cursor });
    }
    this.apply(edited, true);
  }

  async clickStaff(x: number, y: number, shift = false) {
    const hit = await this.run(() => api.hitTest(this.layoutWidth, x, y, this.cursor?.voice ?? 0));
    if (!hit) {
      this.selected = null;
      return;
    }
    if (this.hairpinMode) {
      if (hit.note === null) {
        this.message = "Choose a note or rest as the endpoint.";
        return;
      }
      await this.chooseHairpinEvent(hit.note, hit.cursor);
      return;
    }
    if (this.slurMode || this.phraseSlurMode) {
      if (hit.note === null) {
        this.message = "Choose a note as the endpoint.";
        return;
      }
      await this.chooseSlurEvent(hit.note, hit.cursor, this.phraseSlurMode);
      return;
    }
    if (this.inputMode) {
      const cursor = this.cursor;
      const note = hit.note;
      const selectedAtCursor =
        note !== null && note === this.selected && this.samePosition(cursor, hit.cursor);
      if (selectedAtCursor) {
        if (shift) {
          this.apply((await this.run(() => api.togglePitchInChord(note, this.entryPitch(hit.pitch)))) ?? null, true);
          this.selectedTone = null;
        } else if (hit.chord_pitch) {
          this.targetTone(note, hit.chord_pitch, y, hit.cursor);
        }
        return;
      }
      if (note !== null) {
        this.cursor = hit.cursor;
        this.selected = note;
        this.selectedTone = null;
        return;
      }
      if (!cursor || !this.samePosition(cursor, hit.cursor)) {
        this.cursor = hit.cursor;
        this.selected = null;
        this.selectedTone = null;
        return;
      }
      if (shift) await this.enterAtCursor(() => api.enterNote(cursor, this.entryPitch(hit.pitch), this.duration, true));
    } else {
      this.cursor = hit.cursor;
      this.selected = hit.note;
      this.selectedTone = null;
    }
  }

  async clickEvent(id: number, x: number, y: number, shift = false) {
    const eventCursor = await this.run(() => api.selectEvent(id));
    if (!eventCursor) return;
    if (this.hairpinMode) {
      await this.chooseHairpinEvent(id, eventCursor);
      return;
    }
    if (this.slurMode || this.phraseSlurMode) {
      await this.chooseSlurEvent(id, eventCursor, this.phraseSlurMode);
      return;
    }
    if (!this.inputMode) {
      this.cursor = eventCursor;
      this.selected = id;
      this.selectedTone = null;
      return;
    }
    const eventArea = this.areas.find(
      (area) => area.staff === eventCursor.staff && area.measure === eventCursor.measure,
    );
    const eventX = eventArea?.columns.find(([tick]) => tick === eventCursor.tick)?.[1] ?? x;
    const hit = await this.run(() => api.hitTest(this.layoutWidth, eventX, y, eventCursor.voice));
    const isSelectedAtCursor = this.selected === id && this.samePosition(this.cursor, eventCursor);
    if (!isSelectedAtCursor) {
      this.cursor = eventCursor;
      this.selected = id;
      this.selectedTone = null;
      return;
    }
    if (shift && hit?.note === id) {
      this.apply((await this.run(() => api.togglePitchInChord(id, this.entryPitch(hit.pitch)))) ?? null, true);
      this.selectedTone = null;
    } else if (hit?.note === id && hit.chord_pitch) {
      this.targetTone(id, hit.chord_pitch, y, eventCursor);
    } else {
      this.selectedTone = null;
    }
  }

  private requireCursor(): Cursor | null {
    if (!this.cursor) this.message = "Click on a staff first.";
    return this.cursor;
  }

  async enterStep(step: Step) {
    const cursor = this.requireCursor();
    if (!cursor) return;
    await this.enterAtCursor(() => api.enterStep(cursor, step, this.duration, false, this.entryAlter));
  }

  async enterStepHeld(step: Step) {
    const cursor = this.requireCursor();
    if (!cursor) return;
    await this.enterAtCursor(() => api.enterStep(cursor, step, this.duration, true, this.entryAlter));
  }

  async enterRest() {
    const cursor = this.requireCursor();
    if (!cursor) return;
    await this.enterAtCursor(() => api.enterRest(cursor, this.duration));
  }

  async addNoteAbove(step: Step) {
    const event = this.selected;
    if (event === null) return;
    this.apply((await this.run(() => api.addNoteAbove(event, step, this.entryAlter))) ?? null);
  }

  async deleteSelected() {
    const event = this.selected;
    if (event === null) return;
    this.apply((await this.run(() => api.deleteEvent(event))) ?? null);
  }

  hasArticulation(articulation: Articulation) {
    return this.eventArticulations?.articulations.includes(articulation) ?? false;
  }

  canToggleArticulation(articulation: Articulation) {
    const state = this.eventArticulations;
    return state !== null && (!state.is_rest || articulation === "Fermata");
  }

  hasTie() {
    return this.eventArticulations?.tied ?? false;
  }

  canToggleTie() {
    const state = this.eventArticulations;
    return state !== null && !state.is_rest && (state.tied || state.tie_possible);
  }

  /** Picks a start and an end event; the same event twice gives a hairpin on one note. */
  toggleHairpinMode(kind: Hairpin) {
    if (this.hairpinMode === kind) {
      this.cancelHairpinMode();
      return;
    }
    this.cancelSlurMode();
    this.hairpinMode = kind;
    this.hairpinStart = this.selected;
    this.message = this.hairpinStart === null ? null : "Hairpin start selected. Click the ending note.";
  }

  cancelHairpinMode() {
    this.hairpinMode = null;
    this.hairpinStart = null;
    this.message = null;
  }

  private async chooseHairpinEvent(event: number, cursor: Cursor) {
    const kind = this.hairpinMode;
    if (!kind) return;
    this.cursor = cursor;
    this.selected = event;
    this.selectedTone = null;
    const start = this.hairpinStart;
    if (start === null) {
      this.hairpinStart = event;
      this.message = "Hairpin start selected. Click the ending note.";
      return;
    }
    this.cancelHairpinMode();
    const edited = await this.run(() => api.toggleHairpin(start, event, kind));
    if (edited) {
      this.apply(edited);
      await this.refreshArticulations(event);
    }
  }

  async setDynamic(dynamic: Dynamic | null) {
    const event = this.selected;
    if (event === null) return;
    const edited = await this.run(() => api.setDynamic(event, dynamic));
    if (!edited) return;
    this.apply(edited);
    await this.refreshArticulations(event);
  }

  async removeHairpin() {
    const event = this.selected;
    if (event === null || !this.eventArticulations?.hairpin) return;
    const edited = await this.run(() => api.removeHairpin(event));
    if (!edited) return;
    this.apply(edited);
    await this.refreshArticulations(event);
  }

  async removeSlur(phrase: boolean) {
    const event = this.selected;
    const position = phrase ? this.eventArticulations?.phrase_slur : this.eventArticulations?.slur;
    if (event === null || !position) return;
    const edited = await this.run(() => api.removeSlur(event, phrase));
    if (!edited) return;
    this.apply(edited);
    await this.refreshArticulations(event);
  }

  toggleSlurMode() {
    if (this.slurMode) {
      this.cancelSlurMode();
      return;
    }
    this.cancelHairpinMode();
    this.phraseSlurMode = false;
    this.phraseSlurStart = null;
    this.slurMode = true;
    this.slurStart = this.selected;
    this.message = this.slurStart === null ? null : "Slur start selected. Click the ending note.";
  }

  togglePhraseSlurMode() {
    if (this.phraseSlurMode) {
      this.cancelSlurMode();
      return;
    }
    this.cancelHairpinMode();
    this.slurMode = false;
    this.slurStart = null;
    this.phraseSlurMode = true;
    this.phraseSlurStart = this.selected;
    this.message = this.phraseSlurStart === null
      ? null
      : "Phrase start selected. Click the ending note.";
  }

  cancelSlurMode() {
    this.slurMode = false;
    this.slurStart = null;
    this.phraseSlurMode = false;
    this.phraseSlurStart = null;
    this.message = null;
  }

  private async chooseSlurEvent(event: number, cursor: Cursor, phrase: boolean) {
    this.cursor = cursor;
    this.selected = event;
    this.selectedTone = null;
    const start = phrase ? this.phraseSlurStart : this.slurStart;
    if (start === null) {
      if (phrase) this.phraseSlurStart = event;
      else this.slurStart = event;
      this.message = phrase
        ? "Phrase start selected. Click the ending note."
        : "Slur start selected. Click the ending note.";
      return;
    }
    this.cancelSlurMode();
    if (event === start) {
      this.message = "Slur endpoints must be different notes.";
      return;
    }
    const edited = await this.run(() => phrase
      ? api.togglePhraseSlur(start, event, this.slurPlacement)
      : api.toggleSlur(start, event, this.slurPlacement));
    if (edited) {
      this.apply(edited);
      await this.refreshArticulations(event);
    }
  }

  async refreshArticulations(event: number | null = this.selected) {
    const request = ++this.articulationRequest;
    if (event === null) {
      this.eventArticulations = null;
      return;
    }
    this.eventArticulations = null;
    const articulations = await this.run(() => api.eventArticulations(event));
    if (
      request === this.articulationRequest &&
      this.selected === event &&
      articulations !== undefined
    ) {
      this.eventArticulations = articulations;
    }
  }

  async toggleArticulation(articulation: Articulation) {
    const event = this.selected;
    if (event === null || !this.canToggleArticulation(articulation)) return;
    const edited = await this.run(() => api.toggleArticulation(event, articulation));
    if (!edited) return;
    this.apply(edited);
    await this.refreshArticulations(event);
  }

  async toggleTie() {
    const event = this.selected;
    if (event === null || !this.canToggleTie()) return;
    const edited = await this.run(() => api.toggleTie(event));
    if (!edited) return;
    this.apply(edited);
    await this.refreshArticulations(event);
  }

  async refreshTempoDirection(cursor = this.cursor) {
    const request = ++this.tempoDirectionRequest;
    if (!cursor) {
      this.tempoDirectionAt = null;
      return;
    }
    const direction = await this.run(() => api.tempoDirectionAt(cursor.measure, cursor.tick));
    if (request === this.tempoDirectionRequest && direction !== undefined) {
      this.tempoDirectionAt = direction;
    }
  }

  async setTempoDirection(direction: TempoDirection | null) {
    const cursor = this.cursor;
    if (!cursor) return;
    const edited = await this.run(() =>
      api.setTempoDirection(cursor.measure, cursor.tick, direction),
    );
    if (!edited) return;
    this.apply(edited);
    await this.refreshTempoDirection(cursor);
  }

  async refreshTempoChange(cursor = this.cursor) {
    const request = ++this.tempoChangeRequest;
    if (!cursor) {
      this.tempoChangeAt = null;
      return;
    }
    const mark = await this.run(() => api.tempoChangeAt(cursor.measure, cursor.tick));
    if (request === this.tempoChangeRequest && mark !== undefined) {
      this.tempoChangeAt = mark;
    }
  }

  /** Editing keeps whether an existing mark is printed; new marks are printed. */
  async setTempoChange(tempo: Tempo | null) {
    const cursor = this.cursor;
    if (!cursor) return;
    const hidden = this.tempoChangeAt?.hidden ?? false;
    const edited = await this.run(() =>
      api.setTempoChange(cursor.measure, cursor.tick, tempo, hidden),
    );
    if (!edited) return;
    this.apply(edited);
    await this.refreshTempoChange(cursor);
  }

  async transpose(semitones: number) {
    if (!this.inputMode) return;
    const event = this.selected;
    if (event === null) return;
    this.selectedTone = null;
    this.apply((await this.run(() => api.transposeSemitones(event, semitones))) ?? null);
  }

  async transposeChordTone(semitones: number) {
    if (!this.inputMode) return;
    const tone = this.selectedTone;
    if (!tone || tone.event !== this.selected) {
      await this.transpose(semitones);
      return;
    }
    const changed = await this.run(() =>
      api.transposeChordPitch(tone.event, tone.pitch, semitones),
    );
    if (!changed) return;
    this.apply(changed.edited);
    const targetY = tone.y - (this.pitchDiatonicIndex(changed.pitch) - this.pitchDiatonicIndex(tone.pitch)) * 0.5;
    this.selectedTone = { event: tone.event, pitch: changed.pitch, y: targetY };
  }

  async transposeOctave(octaves: number) {
    if (!this.inputMode) return;
    const event = this.selected;
    if (event === null) return;
    this.selectedTone = null;
    this.apply((await this.run(() => api.transposeOctaves(event, octaves))) ?? null);
  }

  async setAlter(alter: number) {
    if (this.inputMode) {
      this.entryAlter = this.entryAlter === alter ? null : alter;
      return;
    }
    const event = this.selected;
    if (event === null) return;
    this.selectedTone = null;
    this.apply((await this.run(() => api.setAlter(event, alter))) ?? null);
  }

  async setNoteValue(value: NoteValue) {
    this.duration = { value, dots: 0 };
    await this.applyDurationToSelection();
  }

  async toggleDot() {
    this.duration = { ...this.duration, dots: (this.duration.dots + 1) % 3 };
    await this.applyDurationToSelection();
  }

  /** In input mode the duration only applies to what is entered next. */
  private async applyDurationToSelection() {
    const event = this.selected;
    if (this.inputMode || event === null) return;
    const duration = this.duration;
    this.apply((await this.run(() => api.setDuration(event, duration))) ?? null);
  }

  async moveCursor(forward: boolean, movement = "step") {
    const cursor = this.cursor;
    if (cursor === null) return;
    this.selectedTone = null;
    const next = await this.run(() => api.moveCursor(cursor, this.selected, forward, movement));
    if (next) {
      this.cursor = next.cursor;
      this.selected = next.selected;
    }
  }

  async moveDown() {
    const cursor = this.cursor;
    if (!cursor) return;
    if (cursor.voice + 1 < this.voiceCount) {
      await this.selectVoiceAtCursor({ ...cursor, voice: cursor.voice + 1 });
      this.selectedTone = null;
      return;
    }
    await this.moveCursor(true, "staff");
  }

  async moveUp() {
    const cursor = this.cursor;
    if (!cursor) return;
    if (cursor.voice > 0) {
      await this.selectVoiceAtCursor({ ...cursor, voice: cursor.voice - 1 });
      this.selectedTone = null;
      return;
    }
    await this.moveCursor(false, "staff");
  }

  private async selectVoiceAtCursor(cursor: Cursor) {
    const area = this.areas.find(
      (item) => item.staff === cursor.staff && item.measure === cursor.measure,
    );
    const column = area?.columns.reduce<[number, number] | null>((nearest, current) => {
      if (nearest === null || Math.abs(current[0] - cursor.tick) < Math.abs(nearest[0] - cursor.tick)) {
        return current;
      }
      return nearest;
    }, null);
    const x = column?.[1] ?? area?.x_start ?? 0;
    const y = area ? area.y_top + 2 : 0;
    const hit = area
      ? await this.run(() => api.hitTest(this.layoutWidth, x, y, cursor.voice))
      : null;
    this.cursor = cursor;
    this.selected = hit?.cursor.measure === cursor.measure && hit.cursor.tick === cursor.tick
      ? hit.note
      : null;
  }

  async insertMeasure(before: boolean) {
    const cursor = this.cursor;
    if (!cursor) {
      this.message = "Click on a measure first.";
      return;
    }
    this.apply((await this.run(() => api.insertMeasure(cursor.measure, before, cursor))) ?? null, true);
  }

  async removeMeasure(measure: number) {
    const cursor = this.cursor;
    if (!cursor) return;
    this.apply((await this.run(() => api.removeMeasure(measure, cursor))) ?? null, true);
  }

  async addVoice() {
    const cursor = this.requireCursor();
    if (!cursor) return;
    const edited = await this.run(() => api.addVoice(cursor.staff, cursor.measure));
    this.apply(edited ?? null, true);
  }

  async removeVoice() {
    const cursor = this.requireCursor();
    if (!cursor || cursor.voice === 0) return;
    const edited = await this.run(() => api.removeVoice(cursor.staff, cursor.measure, cursor.voice));
    this.apply(edited ?? null, true);
  }

  async endVoice() {
    const cursor = this.requireCursor();
    if (!cursor || !this.canEndVoice) return;
    const edited = await this.run(() =>
      api.endVoice(cursor.staff, cursor.measure, cursor.voice, cursor.tick),
    );
    this.apply(edited ?? null, true);
  }

  selectVoice(voice: number) {
    const cursor = this.cursor;
    if (!cursor || voice < 0 || voice >= this.voiceCount) return;
    this.cursor = { ...cursor, voice, tick: 0 };
    this.selected = null;
    this.selectedTone = null;
    requestAnimationFrame(() => this.focusNoteEditor());
  }

  /** The time signature is applied from the cursor's measure up to the next change. */
  async setTimeSignature(
    numerator: number,
    denominator: number,
    display: TimeSignatureDisplay,
  ): Promise<boolean> {
    const cursor = this.requireCursor();
    if (!cursor) return false;
    const edited = await this.run(() =>
      api.setTimeSignature(cursor.measure, numerator, denominator, display),
    );
    this.apply(edited ?? null);
    return edited !== undefined;
  }

  async setKeySignature(fifths: number): Promise<boolean> {
    const cursor = this.requireCursor();
    if (!cursor) return false;
    const edited = await this.run(() => api.setKeySignature(cursor.measure, fifths));
    this.apply(edited ?? null);
    return edited !== undefined;
  }

  async togglePickup() {
    const cursor = this.requireCursor();
    if (!cursor) return;
    this.apply((await this.run(() => api.togglePickup(cursor.measure, cursor))) ?? null);
  }

  async undo() {
    const marker = this.insertHistory.at(-1);
    const edited = await this.run(() =>
      api.undo(this.cursor, this.selected, marker?.cursor ?? null, marker?.event ?? null),
    );
    if (edited?.restored_insert) this.insertHistory.pop();
    if (edited) this.selectedTone = null;
    this.apply(edited ?? null, true);
  }

  async redo() {
    const edited = await this.run(() => api.redo(this.cursor, this.selected));
    if (edited) this.selectedTone = null;
    this.apply(edited ?? null, true);
  }

  toggleInputMode() {
    if (this.inputMode) {
      this.disableInputMode();
    } else {
      this.inputMode = true;
    }
    this.message = null;
  }

  disableInputMode() {
    this.inputMode = false;
    this.selectedTone = null;
  }

  async newScore(setup: Parameters<typeof api.newScore>[0]): Promise<boolean> {
    const edited = await this.run(() => api.newScore(setup));
    if (edited) this.insertHistory = [];
    this.apply(edited ?? null, true);
    return edited !== undefined && edited !== null;
  }

  async updateScoreProperties(update: Parameters<typeof api.updateScoreProperties>[0]): Promise<boolean> {
    const edited = await this.run(() => api.updateScoreProperties(update));
    this.apply(edited ?? null);
    return edited !== undefined;
  }

  async open() {
    const result = await this.run(() => api.openScore());
    if (!result) return;
    this.insertHistory = [];
    this.apply(result.edited, true);
    if (result.unsupported.length) {
      this.message = `Imported with unsupported features: ${result.unsupported.join(", ")}`;
    }
  }

  async save(saveAs = false) {
    const status = await this.run(() => api.saveScore(saveAs));
    if (status) this.status = status;
  }

  /** Returns true when the key was used. */
  handleKey(event: KeyboardEvent): boolean {
    const key = event.key;
    if (event.ctrlKey || event.metaKey) {
      switch (key.toLowerCase()) {
        case "z":
          void (event.shiftKey ? this.redo() : this.undo());
          return true;
        case "y":
          void this.redo();
          return true;
        case "s":
          void this.save(event.shiftKey);
          return true;
        case "o":
          void this.open();
          return true;
        case "n":
          this.newScoreOpen = true;
          return true;
        case "arrowup":
          if (this.selectedTone !== null) void this.transposeChordTone(12);
          else void this.transposeOctave(1);
          return true;
        case "arrowdown":
          if (this.selectedTone !== null) void this.transposeChordTone(-12);
          else void this.transposeOctave(-1);
          return true;
        default:
          return false;
      }
    }
    if (event.altKey && !key.startsWith("Arrow")) return false;

    if (/^[a-g]$/i.test(key)) {
      const step = key.toUpperCase() as Step;
      if (event.shiftKey && this.selected !== null) void this.addNoteAbove(step);
      else if (event.shiftKey && this.inputMode) void this.enterStepHeld(step);
      else if (this.inputMode) void this.enterStep(step);
      else return false;
      return true;
    }
    if (/^[1-7]$/.test(key)) {
      void this.setNoteValue(NOTE_VALUES[Number(key) - 1]);
      return true;
    }

    switch (key) {
      case "n":
      case "N":
        this.toggleInputMode();
        return true;
      case "t":
      case "T":
        if (!this.requireCursor()) return true;
        this.timeSignatureOpen = true;
        return true;
      case "0":
      case "r":
      case "R":
        if (!this.inputMode) return false;
        void this.enterRest();
        return true;
      case " ":
        if (!this.inputMode) return false;
        void this.enterRest();
        return true;
      case ".":
        void this.toggleDot();
        return true;
      case "Escape":
        if (this.hairpinMode) this.cancelHairpinMode();
        else if (this.slurMode || this.phraseSlurMode) this.cancelSlurMode();
        else if (this.inputMode) this.disableInputMode();
        else this.selected = null;
        return true;
      case "Delete":
      case "Backspace":
        void this.deleteSelected();
        return true;
      case "ArrowLeft":
        void this.moveCursor(false, event.altKey ? "measure" : "step");
        return true;
      case "ArrowRight":
        void this.moveCursor(true, event.altKey ? "measure" : "step");
        return true;
      case "ArrowUp":
        if (event.altKey) void this.moveUp();
        else if (this.selectedTone !== null) void this.transposeChordTone(1);
        else void this.transpose(1);
        return true;
      case "ArrowDown":
        if (event.altKey) void this.moveDown();
        else if (this.selectedTone !== null) void this.transposeChordTone(-1);
        else void this.transpose(-1);
        return true;
      default:
        return false;
    }
  }
}

export const editor = new Editor();
