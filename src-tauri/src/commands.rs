use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use notula_layout::{LayoutOptions, RenderList};
use notula_model::edit::{self, Cursor};
use notula_model::{
    Articulation, Document, Duration, EventId, EventKind, KeySignature, Pitch, Step, TimeSignature,
    TimeSignatureDisplay,
};
use serde::Serialize;
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
pub struct EventArticulations {
    articulations: Vec<Articulation>,
    is_rest: bool,
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

#[tauri::command]
pub fn enter_note(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    pitch: Pitch,
    duration: Duration,
    hold_cursor: bool,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_note(&mut session.doc, cursor, pitch, duration).map_err(text)?;
    Ok(finish_entry(&app, &session, cursor, entry, hold_cursor))
}

#[tauri::command]
pub fn enter_step(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    step: Step,
    duration: Duration,
    hold_cursor: bool,
    alter: Option<i8>,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_step_with_alter(&mut session.doc, cursor, step, duration, alter)
        .map_err(text)?;
    Ok(finish_entry(&app, &session, cursor, entry, hold_cursor))
}

#[tauri::command]
pub fn enter_rest(
    app: AppHandle,
    state: State<AppState>,
    cursor: Cursor,
    duration: Duration,
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_rest(&mut session.doc, cursor, duration).map_err(text)?;
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
pub fn event_articulations(state: State<AppState>, event: EventId) -> Option<EventArticulations> {
    let session = state.lock();
    let located = edit::locate(session.doc.score(), event)?;
    let staff = session.doc.score().staff(located.cursor.staff).ok()?;
    let event = staff.measures[located.cursor.measure].voices[located.cursor.voice]
        .events
        .get(located.index)?;
    Some(EventArticulations {
        articulations: event.markings.articulations.clone(),
        is_rest: matches!(event.kind, EventKind::Rest),
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
pub async fn open_score(app: AppHandle, state: State<'_, AppState>) -> Reply<Option<Edited>> {
    if !confirm_discard(&app, &state) {
        return Ok(None);
    }
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Notula score", &[notula_format::EXTENSION])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(text)?;
    let score = notula_format::load(&path).map_err(text)?;
    let doc = Document::from_score(score);
    Ok(Some(replace_document(&app, &state, doc, Some(path))))
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
) -> Reply<Option<()>> {
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
    let render_list = notula_layout::layout(session.doc.score(), &LayoutOptions { width: 80.0 });
    export_render_list(&render_list, format, &path)?;
    Ok(Some(()))
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
