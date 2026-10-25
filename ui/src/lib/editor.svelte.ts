import {
  api,
  type Cursor,
  type Duration,
  type Edited,
  type NoteValue,
  type Pitch,
  type PlaybackStatus,
  type Status,
  type Step,
  type TimeSignatureDisplay,
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
  selectedTone = $state<{ event: number; pitch: Pitch; y: number } | null>(null);
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
  /** Where measures are on screen, as of the latest layout. */
  areas = $state<MeasureArea[]>([]);
  playbackPlaying = $state(false);
  playbackInstrument = $state<string | null>(null);
  playbackCursor = $state<Cursor | null>(null);
  private noteEditor: SVGSVGElement | null = null;
  private playbackPoll: ReturnType<typeof setInterval> | null = null;
  private insertHistory: InsertMarker[] = [];

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

  async exportScore(format: "pdf" | "png" | "svg") {
    await this.run(() => api.exportScore(format));
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
    if (this.inputMode) {
      const cursor = this.cursor;
      const note = hit.note;
      if (note !== null && note === this.selected) {
        if (shift) {
          this.apply((await this.run(() => api.togglePitchInChord(note, this.entryPitch(hit.pitch)))) ?? null, true);
          this.selectedTone = null;
        } else if (hit.chord_pitch) {
          this.targetTone(note, hit.chord_pitch, y, hit.cursor);
        }
        return;
      }
      if (!cursor || !this.samePosition(cursor, hit.cursor)) {
        this.cursor = hit.cursor;
        this.selected = note;
        if (note !== null && hit.chord_pitch) this.targetTone(note, hit.chord_pitch, y, hit.cursor);
        else this.selectedTone = null;
        return;
      }
      if (note !== null) {
        if (shift) {
          this.apply((await this.run(() => api.togglePitchInChord(note, this.entryPitch(hit.pitch)))) ?? null);
          this.selectedTone = null;
        } else {
          this.selected = note;
          if (hit.chord_pitch) this.targetTone(note, hit.chord_pitch, y, hit.cursor);
          else this.selectedTone = null;
        }
        return;
      }
      if (shift) await this.enterAtCursor(() => api.enterNote(cursor, this.entryPitch(hit.pitch), this.duration, true));
    } else {
      this.cursor = hit.cursor;
      this.selected = hit.note;
      if (hit.note !== null && hit.chord_pitch) this.targetTone(hit.note, hit.chord_pitch, y, hit.cursor);
      else this.selectedTone = null;
    }
  }

  async clickEvent(id: number, x: number, y: number, shift = false) {
    const eventCursor = await this.run(() => api.selectEvent(id));
    if (!eventCursor) return;
    if (!this.inputMode) {
      this.cursor = eventCursor;
      this.selected = id;
      return;
    }
    const hit = await this.run(() => api.hitTest(this.layoutWidth, x, y, eventCursor.voice));
    const isSelectedAtCursor = this.selected === id && this.samePosition(this.cursor, eventCursor);
    if (!isSelectedAtCursor) {
      this.cursor = eventCursor;
      this.selected = id;
      if (hit?.chord_pitch) this.targetTone(id, hit.chord_pitch, y, eventCursor);
      else this.selectedTone = null;
      return;
    }
    if (shift && hit) {
      this.apply((await this.run(() => api.togglePitchInChord(id, this.entryPitch(hit.pitch)))) ?? null, true);
      this.selectedTone = null;
    } else if (hit?.chord_pitch) {
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

  async transpose(semitones: number) {
    const event = this.selected;
    if (event === null) return;
    this.selectedTone = null;
    this.apply((await this.run(() => api.transposeSemitones(event, semitones))) ?? null);
  }

  async transposeChordTone(semitones: number) {
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
      this.cursor = { ...cursor, voice: cursor.voice + 1 };
      this.selected = null;
      this.selectedTone = null;
      return;
    }
    await this.moveCursor(true, "staff");
  }

  async moveUp() {
    const cursor = this.cursor;
    if (!cursor) return;
    if (cursor.voice > 0) {
      this.cursor = { ...cursor, voice: cursor.voice - 1 };
      this.selected = null;
      this.selectedTone = null;
      return;
    }
    await this.moveCursor(false, "staff");
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
    this.inputMode = !this.inputMode;
    this.message = null;
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
    const edited = await this.run(() => api.openScore());
    if (edited) this.insertHistory = [];
    this.apply(edited ?? null, true);
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
        if (this.inputMode) this.inputMode = false;
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
