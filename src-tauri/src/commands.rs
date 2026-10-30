use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use notula_layout::{LayoutOptions, RenderList};
use notula_model::clipboard::{self, Clip};
use notula_model::edit::{self, Cursor};
use notula_model::selection::{ResolvedSelection, Selection, Step as SelectionStep};
use notula_model::{
    Articulation, Document, Duration, Dynamic, EventId, EventKind, Hairpin, KeySignature, Pitch,
    Register, StaffId, Step, Tempo, TempoChangeMark, TempoDirection, TimeSignature,
    TimeSignatureDisplay,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::export::{ExportFormat, export_render_list};
use crate::playback::PlaybackController;
use crate::session::{Session, Status};
use crate::setup::{self, InstrumentInfo, ScoreProperties, ScorePropertiesUpdate, ScoreSetup};

pub struct AppState {
    session: Mutex<Session>,
    playback: Mutex<Option<PlaybackController>>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session: Mutex::new(session),
            playback: Mutex::new(None),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Serialize)]
pub struct PlaybackStatus {
    playing: bool,
    instrument: Option<String>,
    cursor: Option<Cursor>,
}

#[derive(Serialize)]
pub struct OpenScoreResult {
    edited: Edited,
    unsupported: Vec<String>,
}

#[derive(Serialize)]
pub struct EventArticulations {
    articulations: Vec<Articulation>,
    is_rest: bool,
    tied: bool,
    tie_possible: bool,
    dynamic: Option<Dynamic>,
    register: Option<Register>,
    /// Where the event lies in a hairpin, slur, and phrase slur, if it belongs to one.
    hairpin: Option<edit::SpanPosition>,
    slur: Option<edit::SpanPosition>,
    phrase_slur: Option<edit::SpanPosition>,
}

type Reply<T> = Result<T, String>;

fn text(error: impl ToString) -> String {
    error.to_string()
}

fn pitch_for_chord_position(position: Pitch, pitches: &[Pitch]) -> Pitch {
    pitches
        .iter()
        .copied()
        .find(|pitch| pitch.diatonic_index() == position.diatonic_index())
        .unwrap_or(position)
}

fn chord_pitch_at_position(position: Pitch, pitches: &[Pitch]) -> Option<Pitch> {
    pitches
        .iter()
        .copied()
        .find(|pitch| pitch.diatonic_index() == position.diatonic_index())
}

#[cfg(test)]
mod tests {
    use super::{chord_pitch_at_position, pitch_for_chord_position};
    use notula_model::{Pitch, Step};

    #[test]
    fn chord_hit_uses_existing_pitch_accidental_at_the_staff_position() {
        let key_pitch = Pitch::new(Step::F, 1, 4);
        let natural_pitch = Pitch::new(Step::F, 0, 4);

        assert_eq!(
            pitch_for_chord_position(key_pitch, &[natural_pitch]),
            natural_pitch
        );
        assert_eq!(
            chord_pitch_at_position(key_pitch, &[natural_pitch]),
            Some(natural_pitch)
        );
        assert_eq!(chord_pitch_at_position(key_pitch, &[]), None);
        assert_eq!(pitch_for_chord_position(key_pitch, &[]), key_pitch);
    }
}

/// What the UI needs after a change. A `None` cursor or selection means "unchanged".
#[derive(Serialize)]
pub struct Edited {
    status: Status,
    cursor: Option<Cursor>,
    selected: Option<EventId>,
    inserted: Option<EventId>,
    undo_cursor: Option<Cursor>,
    restored_insert: bool,
}

#[derive(Serialize)]
pub struct PitchEdited {
    edited: Edited,
    pitch: Pitch,
}

pub fn update_title(app: &AppHandle, session: &Session) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(&session.window_title());
    }
}

fn finish(
    app: &AppHandle,
    session: &Session,
    cursor: Option<Cursor>,
    selected: Option<EventId>,
) -> Edited {
    update_title(app, session);
    Edited {
        status: session.status(),
        cursor,
        selected,
        inserted: None,
        undo_cursor: None,
        restored_insert: false,
    }
}

fn finish_entry(
    app: &AppHandle,
    session: &Session,
    cursor: Cursor,
    entry: edit::Entry,
    hold_cursor: bool,
) -> Edited {
    let next_cursor = if hold_cursor { cursor } else { entry.next };
    let selected = if hold_cursor {
        Some(entry.event)
    } else {
        edit::note_at_cursor(session.doc.score(), entry.next)
    };
    let mut edited = finish(app, session, Some(next_cursor), selected);
    edited.inserted = Some(entry.event);
    edited.undo_cursor = Some(cursor);
    edited
}

fn cursor_of(session: &Session, event: EventId) -> Option<Cursor> {
    edit::locate(session.doc.score(), event).map(|located| located.cursor)
}

#[tauri::command]
pub fn current_state(app: AppHandle, state: State<AppState>) -> Edited {
    let session = state.lock();
    finish(&app, &session, session.first_cursor(), None)
}

/// `width` is the available width in staff spaces.
#[tauri::command]
pub fn render_score(state: State<AppState>, width: f32) -> RenderList {
    let session = state.lock();
    notula_layout::layout(session.doc.score(), &LayoutOptions { width })
}

/// How to create or change a range selection.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionRequest {
    /// A cursor covering one staff and no notes.
    Caret {
        cursor: Cursor,
    },
    /// The selection's staves, collapsed to a cursor position, as when a key moves the cursor.
    Collapse {
        selection: Selection,
        cursor: Cursor,
    },
    Event {
        event: EventId,
    },
    All,
    Extend {
        selection: Selection,
        step: SelectionStep,
    },
    /// Shift+click on an event.
    ExtendTo {
        selection: Selection,
        event: EventId,
    },
    /// A box dragged over the page, in staff spaces.
    Box {
        width: f32,
        from: (f32, f32),
        to: (f32, f32),
    },
    /// The same selection again, after the score changed.
    Resolve {
        selection: Selection,
    },
}

#[derive(Serialize)]
pub struct SelectionInfo {
    selection: Selection,
    resolved: ResolvedSelection,
    /// Selected notes and chords, not counting rests.
    notes: usize,
    /// Articulations every selected event that can carry them already has.
    common_articulations: Vec<Articulation>,
    /// Where the cursor goes, and the selection's start and end, in the anchor staff.
    cursor: Cursor,
    start: Cursor,
    end: Cursor,
}

/// A cursor at a score position in a staff, in `voice` if that measure has it.
fn cursor_at(
    score: &notula_model::Score,
    staff: StaffId,
    voice: usize,
    tick: u64,
) -> Option<Cursor> {
    let (measure, tick) = notula_model::selection::position(score, tick);
    let voices = score.staff(staff).ok()?.measures.get(measure)?.voices.len();
    Some(Cursor {
        staff,
        measure,
        voice: if voice < voices { voice } else { 0 },
        tick,
    })
}

fn selection_info(score: &notula_model::Score, selection: Selection) -> Option<SelectionInfo> {
    let resolved = selection.resolve(score)?;
    let notes = edit::note_count(score, &resolved.events);
    let common_articulations = edit::common_articulations(score, &resolved.events);
    let at = |tick| cursor_at(score, selection.anchor_staff, selection.anchor_voice, tick);
    Some(SelectionInfo {
        cursor: at(resolved.caret)?,
        start: at(resolved.span.start)?,
        end: at(resolved.span.end)?,
        selection,
        resolved,
        notes,
        common_articulations,
    })
}

/// An edit applied to every event in a range selection.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionEdit {
    Delete,
    Transpose { semitones: i8 },
    TransposeOctaves { octaves: i8 },
    ToggleArticulation { articulation: Articulation },
}

#[derive(Serialize)]
pub struct Shifted {
    edited: Edited,
    /// The selection at the notes' new place, or around what was pasted.
    selection: Option<SelectionInfo>,
}

/// Moves the selected notes earlier or later by one note value, through rests only.
#[tauri::command]
pub fn shift_selection(
    app: AppHandle,
    state: State<AppState>,
    selection: Selection,
    forward: bool,
    duration: Duration,
) -> Reply<Shifted> {
    let mut session = state.lock();
    duration.validate().map_err(text)?;
    let moved = notula_model::shift::shift_selection(
        &mut session.doc,
        &selection,
        forward,
        duration.ticks(),
    )
    .map_err(text)?;
    Ok(Shifted {
        selection: selection_info(session.doc.score(), moved),
        edited: finish(&app, &session, None, None),
    })
}

/// What the clipboard holds after copying or cutting.
#[derive(Serialize)]
pub struct ClipInfo {
    staves: usize,
}

fn clip_info(clip: &Clip) -> ClipInfo {
    ClipInfo {
        staves: clip.staves.len(),
    }
}

#[tauri::command]
pub fn copy_selection(state: State<AppState>, selection: Selection) -> Reply<ClipInfo> {
    let mut session = state.lock();
    let clip = clipboard::copy(session.doc.score(), &selection).map_err(text)?;
    let info = clip_info(&clip);
    session.clipboard = Some(clip);
    Ok(info)
}

#[derive(Serialize)]
pub struct Cut {
    edited: Edited,
    clip: ClipInfo,
}

/// Copies the selection and turns its notes into rests.
#[tauri::command]
pub fn cut_selection(app: AppHandle, state: State<AppState>, selection: Selection) -> Reply<Cut> {
    let mut session = state.lock();
    let clip = clipboard::cut(&mut session.doc, &selection).map_err(text)?;
    let info = clip_info(&clip);
    session.clipboard = Some(clip);
    Ok(Cut {
        edited: finish(&app, &session, None, None),
        clip: info,
    })
}

/// Pastes the clipboard at the cursor or the selection's start and selects what was pasted.
#[tauri::command]
pub fn paste(app: AppHandle, state: State<AppState>, selection: Selection) -> Reply<Shifted> {
    let mut session = state.lock();
    let session = &mut *session;
    let clip = session
        .clipboard
        .as_ref()
        .ok_or("nothing has been copied")?;
    let pasted = clipboard::paste(&mut session.doc, &selection, clip).map_err(text)?;
    Ok(Shifted {
        selection: selection_info(session.doc.score(), pasted),
        edited: finish(&app, session, None, None),
    })
}

#[tauri::command]
pub fn edit_selection(
    app: AppHandle,
    state: State<AppState>,
    selection: Selection,
    edit: SelectionEdit,
) -> Reply<Edited> {
    let mut session = state.lock();
    let events = selection
        .resolve(session.doc.score())
        .ok_or("the selection no longer fits the score")?
        .events;
    let doc = &mut session.doc;
    match edit {
        SelectionEdit::Delete => edit::delete_events(doc, &events),
        SelectionEdit::Transpose { semitones } => {
            edit::transpose_events(doc, &events, semitones, 0)
        }
        SelectionEdit::TransposeOctaves { octaves } => {
            edit::transpose_events(doc, &events, 0, octaves)
        }
        SelectionEdit::ToggleArticulation { articulation } => {
            edit::toggle_articulation_events(doc, &events, articulation)
        }
    }
    .map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

#[tauri::command]
pub fn select(state: State<AppState>, request: SelectionRequest) -> Option<SelectionInfo> {
    let session = state.lock();
    let score = session.doc.score();
    let selection = match request {
        SelectionRequest::Caret { cursor } => Selection::caret(
            score,
            cursor.staff,
            cursor.voice,
            cursor.measure,
            cursor.tick,
        )?,
        SelectionRequest::Collapse { selection, cursor } => Selection {
            anchor_staff: cursor.staff,
            anchor_voice: cursor.voice,
            ..selection
        }
        .collapsed_to(notula_model::selection::absolute(
            score,
            cursor.measure,
            cursor.tick,
        )?),
        SelectionRequest::Event { event } => Selection::for_event(score, event)?,
        SelectionRequest::All => Selection::all(score)?,
        SelectionRequest::Extend { selection, step } => selection.extend(score, step),
        SelectionRequest::ExtendTo { selection, event } => {
            selection.extend_to_event(score, event)?
        }
        SelectionRequest::Box { width, from, to } => {
            let list = notula_layout::layout(score, &LayoutOptions { width });
            notula_layout::box_selection(score, &list, from, to)?
        }
        SelectionRequest::Resolve { selection } => selection,
    };
    selection_info(score, selection)
}

#[derive(Serialize)]
pub struct HitInfo {
    cursor: Cursor,
    pitch: Pitch,
    chord_pitch: Option<Pitch>,
    note: Option<EventId>,
}

#[tauri::command]
pub fn hit_test(
    state: State<AppState>,
    width: f32,
    x: f32,
    y: f32,
    voice: usize,
) -> Option<HitInfo> {
    let session = state.lock();
    let score = session.doc.score();
    let list = notula_layout::layout(score, &LayoutOptions { width });
    let hit = notula_layout::hit_test(score, &list, x, y)?;
    let voice = voice.min(
        score.staff(hit.staff).ok()?.measures[hit.measure]
            .voices
            .len()
            - 1,
    );
    let cursor = Cursor {
        staff: hit.staff,
        measure: hit.measure,
        voice,
        tick: hit.tick.unwrap_or(0),
    };
    let voice_end = edit::voice_end(score, hit.staff, hit.measure, voice).ok()?;
    let capacity = score.measure_capacity(hit.measure).ok()?;
    let cursor = if hit.tick.is_none() {
        if voice_end >= capacity && hit.next_measure.is_some() {
            Cursor {
                measure: hit.next_measure?,
                tick: 0,
                ..cursor
            }
        } else {
            edit::cursor_at_content_end(score, cursor)?
        }
    } else {
        edit::cursor_at_staff_hit(score, cursor)?
    };
    let note = edit::note_at_cursor(score, cursor);
    let pitches = note
        .and_then(|id| edit::locate(score, id))
        .and_then(|located| {
            let staff = score.staff(located.cursor.staff).ok()?;
            let event = staff.measures[located.cursor.measure].voices[located.cursor.voice]
                .events
                .get(located.index)?;
            match &event.kind {
                EventKind::Chord { pitches, .. } => Some(pitches.as_slice()),
                EventKind::Rest => None,
            }
        });
    Some(HitInfo {
        cursor,
        pitch: pitches.map_or(hit.pitch, |pitches| {
            pitch_for_chord_position(hit.pitch, pitches)
        }),
        chord_pitch: pitches.and_then(|pitches| chord_pitch_at_position(hit.pitch, pitches)),
        note,
    })
}

#[tauri::command]
pub fn select_event(state: State<AppState>, event: EventId) -> Option<Cursor> {
    cursor_of(&state.lock(), event)
}

#[tauri::command]
pub fn move_cursor(
    state: State<AppState>,
    cursor: Cursor,
    selected: Option<EventId>,
    forward: bool,
    movement: String,
) -> Option<CursorPosition> {
    let session = state.lock();
    let score = session.doc.score();
    let cursor = match movement.as_str() {
        "measure" => edit::move_cursor_to_measure(score, cursor, forward)?,
        "staff" => edit::move_cursor_to_staff(score, cursor, forward)?,
        _ => edit::move_cursor(score, cursor, selected, forward)?,
    };
    Some(CursorPosition {
        selected: edit::note_at_cursor(score, cursor),
        cursor,
    })
}

#[derive(Serialize)]
pub struct CursorPosition {
    cursor: Cursor,
    selected: Option<EventId>,
}

/// Optional settings shared by the note and rest entry commands.
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct EntryOptions {
    /// Keep the cursor on the entered event, as when building a chord.
    hold_cursor: bool,
    /// Accidental for a note entered by letter; `None` follows the key signature.
    alter: Option<i8>,
    /// Further staves to enter the same event into, as one undo step.
    also: Vec<StaffId>,
}

#[tauri::command]
pub fn enter_note(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    pitch: Pitch,
    duration: Duration,
    options: EntryOptions,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_in_staves(&mut session.doc, cursor, &options.also, |doc, at| {
        edit::enter_note(doc, at, pitch, duration)
    })
    .map_err(text)?;
    Ok(finish_entry(
        &app,
        &session,
        cursor,
        entry,
        options.hold_cursor,
    ))
}

#[tauri::command]
pub fn enter_step(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    step: Step,
    duration: Duration,
    options: EntryOptions,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_in_staves(&mut session.doc, cursor, &options.also, |doc, at| {
        edit::enter_step_with_alter(doc, at, step, duration, options.alter)
    })
    .map_err(text)?;
    Ok(finish_entry(
        &app,
        &session,
        cursor,
        entry,
        options.hold_cursor,
    ))
}

#[tauri::command]
pub fn enter_rest(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    duration: Duration,
    options: EntryOptions,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_in_staves(&mut session.doc, cursor, &options.also, |doc, at| {
        edit::enter_rest(doc, at, duration)
    })
    .map_err(text)?;
    Ok(finish_entry(&app, &session, cursor, entry, false))
}

#[tauri::command]
pub fn set_duration(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    duration: Duration,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::set_duration(&mut session.doc, event, duration).map_err(text)?;
    Ok(finish(&app, &session, Some(entry.next), Some(entry.event)))
}

#[tauri::command]
pub fn toggle_articulation(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    articulation: Articulation,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_articulation(&mut session.doc, event, articulation).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn toggle_tie(app: AppHandle, state: State<AppState>, event: EventId) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_tie(&mut session.doc, event).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn toggle_slur(
    app: AppHandle,
    state: State<AppState>,
    start: EventId,
    end: EventId,
    placement: notula_model::SlurPlacement,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_slur(&mut session.doc, start, end, placement).map_err(text)?;
    let cursor = cursor_of(&session, end);
    Ok(finish(&app, &session, cursor, Some(end)))
}

#[tauri::command]
pub fn remove_slur(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    phrase: bool,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::remove_slur(&mut session.doc, event, phrase).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn set_dynamic(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    dynamic: Option<Dynamic>,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::set_dynamic(&mut session.doc, event, dynamic).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

/// Sets or, with `None`, removes the accordion register at an event.
#[tauri::command]
pub fn set_register(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    register: Option<Register>,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::set_register(&mut session.doc, event, register).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn toggle_hairpin(
    app: AppHandle,
    state: State<AppState>,
    start: EventId,
    end: EventId,
    kind: Hairpin,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_hairpin(&mut session.doc, start, end, kind).map_err(text)?;
    let cursor = cursor_of(&session, end);
    Ok(finish(&app, &session, cursor, Some(end)))
}

#[tauri::command]
pub fn remove_hairpin(app: AppHandle, state: State<AppState>, event: EventId) -> Reply<Edited> {
    let mut session = state.lock();
    edit::remove_hairpin(&mut session.doc, event).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn toggle_phrase_slur(
    app: AppHandle,
    state: State<AppState>,
    start: EventId,
    end: EventId,
    placement: notula_model::SlurPlacement,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_phrase_slur(&mut session.doc, start, end, placement).map_err(text)?;
    let cursor = cursor_of(&session, end);
    Ok(finish(&app, &session, cursor, Some(end)))
}

#[tauri::command]
pub fn tempo_direction_at(
    state: State<AppState>,
    measure: usize,
    tick: u32,
) -> Option<TempoDirection> {
    let session = state.lock();
    session
        .doc
        .score()
        .tempo_directions
        .iter()
        .find(|mark| mark.measure == measure && mark.tick == tick)
        .map(|mark| mark.direction)
}

#[tauri::command]
pub fn set_tempo_direction(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    tick: u32,
    direction: Option<TempoDirection>,
) -> Reply<Edited> {
    let mut session = state.lock();
    session
        .doc
        .execute(notula_model::Command::SetTempoDirection {
            measure,
            tick,
            direction,
        })
        .map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

#[tauri::command]
pub fn tempo_change_at(
    state: State<AppState>,
    measure: usize,
    tick: u32,
) -> Option<TempoChangeMark> {
    let session = state.lock();
    session.doc.score().tempo_change_at(measure, tick).copied()
}

#[tauri::command]
pub fn set_tempo_change(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    tick: u32,
    tempo: Option<Tempo>,
    hidden: bool,
) -> Reply<Edited> {
    let mut session = state.lock();
    session
        .doc
        .execute(notula_model::Command::SetTempoChange {
            measure,
            tick,
            tempo,
            hidden,
        })
        .map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

#[tauri::command]
pub fn event_articulations(state: State<AppState>, event: EventId) -> Option<EventArticulations> {
    let session = state.lock();
    let located = edit::locate(session.doc.score(), event)?;
    let staff = session.doc.score().staff(located.cursor.staff).ok()?;
    let event = staff.measures[located.cursor.measure].voices[located.cursor.voice]
        .events
        .get(located.index)?;
    let tied = matches!(event.kind, EventKind::Chord { tied: true, .. });
    let tie_possible = tied
        || match &event.kind {
            EventKind::Chord { pitches, .. } => {
                edit::adjacent_event(session.doc.score(), event.id, true)
                    .and_then(|next| edit::locate(session.doc.score(), next))
                    .and_then(|next| {
                        let next_staff = session.doc.score().staff(next.cursor.staff).ok()?;
                        let next_event = next_staff.measures[next.cursor.measure].voices
                            [next.cursor.voice]
                            .events
                            .get(next.index)?;
                        match &next_event.kind {
                            EventKind::Chord {
                                pitches: next_pitches,
                                ..
                            } => Some(pitches.iter().any(|pitch| next_pitches.contains(pitch))),
                            EventKind::Rest => Some(false),
                        }
                    })
                    .unwrap_or(false)
            }
            EventKind::Rest => false,
        };
    Some(EventArticulations {
        articulations: event.markings.articulations.clone(),
        is_rest: matches!(event.kind, EventKind::Rest),
        tied,
        tie_possible,
        dynamic: event.markings.dynamic,
        register: event.markings.register,
        hairpin: edit::hairpin_at(session.doc.score(), event.id)
            .map(|span| span.position_of(event.id)),
        slur: edit::slur_at(session.doc.score(), event.id, false)
            .map(|span| span.position_of(event.id)),
        phrase_slur: edit::slur_at(session.doc.score(), event.id, true)
            .map(|span| span.position_of(event.id)),
    })
}

#[tauri::command]
pub fn delete_event(app: AppHandle, state: State<AppState>, event: EventId) -> Reply<Edited> {
    let mut session = state.lock();
    edit::delete_event(&mut session.doc, event).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn transpose_semitones(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    semitones: i8,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::transpose_semitones(&mut session.doc, event, semitones).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn transpose_chord_pitch(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    pitch: Pitch,
    semitones: i8,
) -> Reply<PitchEdited> {
    let mut session = state.lock();
    let pitch =
        edit::transpose_chord_pitch(&mut session.doc, event, pitch, semitones).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(PitchEdited {
        edited: finish(&app, &session, cursor, Some(event)),
        pitch,
    })
}

#[tauri::command]
pub fn transpose_octaves(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    octaves: i8,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::transpose_octaves(&mut session.doc, event, octaves).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn set_alter(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    alter: i8,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::set_alter(&mut session.doc, event, alter).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn add_note_above(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    step: Step,
    alter: Option<i8>,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::add_note_above_with_alter(&mut session.doc, event, step, alter).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn add_pitch_to_chord(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    pitch: Pitch,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::add_pitch_to_chord(&mut session.doc, event, pitch).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn remove_pitch_from_chord(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    pitch: Pitch,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::remove_pitch_from_chord(&mut session.doc, event, pitch).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

#[tauri::command]
pub fn toggle_pitch_in_chord(
    app: AppHandle,
    state: State<AppState>,
    event: EventId,
    pitch: Pitch,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_pitch_in_chord(&mut session.doc, event, pitch).map_err(text)?;
    let cursor = cursor_of(&session, event);
    Ok(finish(&app, &session, cursor, Some(event)))
}

/// Inserts an empty measure before or after the current measure.
#[tauri::command]
pub fn insert_measure(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    before: bool,
    cursor: Option<Cursor>,
) -> Reply<Edited> {
    let mut session = state.lock();
    let index = if before { measure } else { measure + 1 };
    edit::insert_measure_at(&mut session.doc, index).map_err(text)?;
    let cursor = cursor.map(|c| Cursor {
        measure: measure + 1,
        tick: if before { c.tick } else { 0 },
        ..c
    });
    Ok(finish(&app, &session, cursor, None))
}

#[tauri::command]
pub fn remove_measure(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    cursor: Option<Cursor>,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::remove_measure(&mut session.doc, measure).map_err(text)?;
    let cursor = cursor.map(|c| Cursor {
        measure: if measure == 0 { 0 } else { measure - 1 },
        tick: 0,
        ..c
    });
    Ok(finish(&app, &session, cursor, None))
}

#[tauri::command]
pub fn add_voice(
    app: AppHandle,
    state: State<AppState>,
    staff: notula_model::StaffId,
    measure: usize,
) -> Reply<Edited> {
    let mut session = state.lock();
    session
        .doc
        .score()
        .measure_capacity(measure)
        .map_err(text)?;
    let voice = edit::add_voice(&mut session.doc, staff, measure).map_err(text)?;
    let cursor = Cursor {
        staff,
        measure,
        voice,
        tick: 0,
    };
    Ok(finish(&app, &session, Some(cursor), None))
}

#[tauri::command]
pub fn remove_voice(
    app: AppHandle,
    state: State<AppState>,
    staff: notula_model::StaffId,
    measure: usize,
    voice: usize,
) -> Reply<Edited> {
    let mut session = state.lock();
    session
        .doc
        .score()
        .measure_capacity(measure)
        .map_err(text)?;
    edit::remove_voice(&mut session.doc, staff, voice).map_err(text)?;
    let cursor = Cursor {
        staff,
        measure,
        voice: voice.saturating_sub(1),
        tick: 0,
    };
    Ok(finish(&app, &session, Some(cursor), None))
}

#[tauri::command]
pub fn end_voice(
    app: AppHandle,
    state: State<AppState>,
    staff: notula_model::StaffId,
    measure: usize,
    voice: usize,
    tick: u32,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::end_voice_at(&mut session.doc, staff, measure, voice, tick).map_err(text)?;
    let cursor = Cursor {
        staff,
        measure,
        voice: 0,
        tick,
    };
    Ok(finish(&app, &session, Some(cursor), None))
}

/// Applies to the measure and all following ones up to the next time signature change.
#[tauri::command]
pub fn set_time_signature(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    numerator: u8,
    denominator: u8,
    display: TimeSignatureDisplay,
) -> Reply<Edited> {
    let mut session = state.lock();
    let time_signature = TimeSignature {
        numerator,
        denominator,
    };
    edit::change_time_signature_with_display(&mut session.doc, measure, time_signature, display)
        .map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

#[tauri::command]
pub fn set_key_signature(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    fifths: i8,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::change_key_signature(&mut session.doc, measure, KeySignature { fifths }).map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

/// Shortens the measure to what is written in it, or restores its full length.
#[tauri::command]
pub fn toggle_pickup(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    cursor: Option<Cursor>,
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::toggle_pickup(&mut session.doc, measure).map_err(text)?;

    let score = session.doc.score();
    // A cursor left at the new end of the measure continues in the next one.
    let moved = cursor.filter(|c| {
        c.measure == measure
            && measure + 1 < score.measures.len()
            && score
                .measure_capacity(measure)
                .is_ok_and(|capacity| c.tick >= capacity)
    });
    let moved = moved.map(|c| Cursor {
        measure: measure + 1,
        tick: 0,
        ..c
    });
    Ok(finish(&app, &session, moved, None))
}

/// Keeps the selection and cursor only if they still exist after the history moved.
fn after_history(
    app: &AppHandle,
    session: &Session,
    cursor: Option<Cursor>,
    selected: Option<EventId>,
) -> Edited {
    let score = session.doc.score();
    let selected = selected.filter(|&id| edit::locate(score, id).is_some());
    let cursor = selected
        .and_then(|id| cursor_of(session, id))
        .or_else(|| cursor.and_then(|c| edit::snap_cursor(score, c).ok()))
        .or_else(|| session.first_cursor());
    finish(app, session, cursor, selected)
}

#[tauri::command]
pub fn undo(
    app: AppHandle,
    state: State<AppState>,
    cursor: Option<Cursor>,
    selected: Option<EventId>,
    restore_cursor: Option<Cursor>,
    inserted_event: Option<EventId>,
) -> Reply<Edited> {
    let mut session = state.lock();
    let changed = session.doc.undo().map_err(text)?;
    let insertion_removed = changed
        && inserted_event.is_some_and(|event| edit::locate(session.doc.score(), event).is_none());
    let mut edited = after_history(&app, &session, cursor, selected);
    if insertion_removed {
        edited.cursor = restore_cursor;
        edited.selected =
            restore_cursor.and_then(|at| edit::note_at_cursor(session.doc.score(), at));
        edited.restored_insert = true;
    }
    Ok(edited)
}

#[tauri::command]
pub fn redo(
    app: AppHandle,
    state: State<AppState>,
    cursor: Option<Cursor>,
    selected: Option<EventId>,
) -> Reply<Edited> {
    let mut session = state.lock();
    session.doc.redo().map_err(text)?;
    Ok(after_history(&app, &session, cursor, selected))
}

#[tauri::command]
pub fn instrument_presets() -> Vec<InstrumentInfo> {
    setup::instruments()
}

#[tauri::command]
pub fn score_properties(state: State<AppState>) -> ScoreProperties {
    setup::properties(state.lock().doc.score())
}

#[tauri::command]
pub fn update_score_properties(
    app: AppHandle,
    state: State<AppState>,
    update: ScorePropertiesUpdate,
) -> Reply<Edited> {
    let mut session = state.lock();
    setup::update_properties(&mut session.doc, update).map_err(text)?;
    Ok(finish(&app, &session, None, None))
}

/// Asks before throwing away unsaved work; returns whether to go on.
fn confirm_discard(app: &AppHandle, state: &AppState) -> bool {
    if !state.lock().is_dirty() {
        return true;
    }
    app.dialog()
        .message("The score has unsaved changes. Discard them?")
        .title("Notula")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Discard".to_owned(),
            "Cancel".to_owned(),
        ))
        .blocking_show()
}

fn replace_document(
    app: &AppHandle,
    state: &AppState,
    doc: Document,
    path: Option<PathBuf>,
) -> Edited {
    let mut session = state.lock();
    session.replace(doc, path);
    finish(app, &session, session.first_cursor(), None)
}

/// Returns `None` when the user cancelled.
#[tauri::command]
pub async fn new_score(
    app: AppHandle,
    state: State<'_, AppState>,
    setup: ScoreSetup,
) -> Reply<Option<Edited>> {
    let doc = setup::build(&setup).map_err(text)?;
    if !confirm_discard(&app, &state) {
        return Ok(None);
    }
    Ok(Some(replace_document(&app, &state, doc, None)))
}

#[tauri::command]
pub async fn open_score(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Reply<Option<OpenScoreResult>> {
    let Some(picked) = app
        .dialog()
        .file()
        // The first filter is preselected, so the type dropdown is rarely needed.
        .add_filter(
            "All scores",
            &[notula_format::EXTENSION, "musicxml", "xml", "mxl"],
        )
        .add_filter("Notula score", &[notula_format::EXTENSION])
        .add_filter("MusicXML score", &["musicxml", "xml", "mxl"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(text)?;
    let is_musicxml = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("musicxml")
            || extension.eq_ignore_ascii_case("xml")
            || extension.eq_ignore_ascii_case("mxl")
    });
    let (score, unsupported) = if is_musicxml {
        let imported = notula_format::musicxml::import_file(&path).map_err(text)?;
        (imported.score, imported.unsupported)
    } else if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(notula_format::EXTENSION))
    {
        (notula_format::load(&path).map_err(text)?, Vec::new())
    } else {
        return Err("Unsupported score file extension.".to_owned());
    };
    if !confirm_discard(&app, &state) {
        return Ok(None);
    }
    let doc = Document::from_score(score);
    let native_path = (!is_musicxml).then_some(path);
    Ok(Some(OpenScoreResult {
        edited: replace_document(&app, &state, doc, native_path),
        unsupported,
    }))
}

/// Saves to the current file, or asks for one; returns `None` when the user cancelled.
#[tauri::command]
pub async fn save_score(
    app: AppHandle,
    state: State<'_, AppState>,
    save_as: bool,
) -> Reply<Option<Status>> {
    let current = if save_as {
        None
    } else {
        state.lock().path().cloned()
    };
    let path = match current {
        Some(path) => path,
        None => {
            let Some(picked) = app
                .dialog()
                .file()
                .add_filter("Notula score", &[notula_format::EXTENSION])
                .set_file_name(format!("Untitled.{}", notula_format::EXTENSION))
                .blocking_save_file()
            else {
                return Ok(None);
            };
            let mut path = picked.into_path().map_err(text)?;
            if path
                .extension()
                .is_none_or(|e| e != notula_format::EXTENSION)
            {
                path.set_extension(notula_format::EXTENSION);
            }
            path
        }
    };

    let mut session = state.lock();
    notula_format::save(session.doc.score(), &path).map_err(text)?;
    session.mark_saved(path);
    update_title(&app, &session);
    Ok(Some(session.status()))
}

#[tauri::command]
pub async fn export_score(
    app: AppHandle,
    state: State<'_, AppState>,
    format: String,
) -> Reply<Option<Vec<String>>> {
    let format = ExportFormat::parse(&format)?;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter(format.filter_name(), &[format.extension()])
        .set_file_name(format!("Score.{}", format.extension()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let mut path = picked.into_path().map_err(text)?;
    if path
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(format.extension()))
    {
        path.set_extension(format.extension());
    }
    let session = state.lock();
    if matches!(format, ExportFormat::MusicXml) {
        return notula_format::musicxml::export_file(session.doc.score(), &path)
            .map(Some)
            .map_err(text);
    }
    let render_list = notula_layout::layout(session.doc.score(), &LayoutOptions { width: 80.0 });
    export_render_list(&render_list, format, &path)?;
    Ok(Some(Vec::new()))
}

#[tauri::command]
pub async fn select_soundfont(app: AppHandle, state: State<'_, AppState>) -> Reply<Option<String>> {
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("Instruments", &["sf2", "sf3", "sfz"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(text)?;
    let controller = PlaybackController::load(&path)?;
    persist_soundfont_path(&app, &path)?;
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Instrument".to_owned());
    *state
        .playback
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(controller);
    Ok(Some(display_name))
}

#[tauri::command]
pub fn restore_soundfont(app: AppHandle, state: State<AppState>) -> Reply<Option<String>> {
    let config = soundfont_config_path(&app)?;
    let path = match std::fs::read_to_string(config) {
        Ok(path) => PathBuf::from(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !path.is_file() {
        return Ok(None);
    }
    let controller = PlaybackController::load(&path)?;
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Instrument".to_owned());
    *state
        .playback
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(controller);
    Ok(Some(display_name))
}

fn persist_soundfont_path(app: &AppHandle, path: &std::path::Path) -> Reply<()> {
    let config = soundfont_config_path(app)?;
    let parent = config
        .parent()
        .ok_or_else(|| "Could not determine the application config directory.".to_owned())?;
    std::fs::create_dir_all(parent).map_err(text)?;
    std::fs::write(config, path.to_string_lossy().as_bytes()).map_err(text)
}

fn soundfont_config_path(app: &AppHandle) -> Reply<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(text)?
        .join("playback-soundfont.txt"))
}

#[tauri::command]
pub fn play_score(state: State<AppState>) -> Reply<()> {
    let session = state.lock();
    let playback = state
        .playback
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let controller = playback
        .as_ref()
        .ok_or_else(|| "Choose an instrument before playing.".to_owned())?;
    controller.play(session.doc.score())
}

#[tauri::command]
pub fn stop_playback(state: State<AppState>) -> Reply<()> {
    let playback = state
        .playback
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(controller) = playback.as_ref() {
        controller.stop()?;
    }
    Ok(())
}

#[tauri::command]
pub fn playback_status(state: State<AppState>) -> PlaybackStatus {
    let session = state.lock();
    let playback = state
        .playback
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(controller) = playback.as_ref() else {
        return PlaybackStatus {
            playing: false,
            instrument: None,
            cursor: None,
        };
    };
    let score = session.doc.score();
    let mut tick = controller.position();
    let mut cursor = None;
    for (measure, info) in score.measures.iter().enumerate() {
        let capacity = u64::from(info.capacity());
        if tick < capacity || measure + 1 == score.measures.len() {
            cursor = score.staves().next().map(|staff| Cursor {
                staff: staff.id,
                measure,
                voice: 0,
                tick: tick.min(capacity) as u32,
            });
            break;
        }
        tick -= capacity;
    }
    PlaybackStatus {
        playing: controller.is_playing(),
        instrument: controller
            .instrument_path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        cursor,
    }
}
