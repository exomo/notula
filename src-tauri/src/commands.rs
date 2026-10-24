use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use noten_layout::{LayoutOptions, RenderList};
use noten_model::edit::{self, Cursor};
use noten_model::{Document, Duration, EventId, Pitch, Step, TimeSignature};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::session::{Session, Status};
use crate::setup::{self, InstrumentInfo, ScoreSetup};

pub struct AppState {
    session: Mutex<Session>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session: Mutex::new(session),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

type Reply<T> = Result<T, String>;

fn text(error: impl ToString) -> String {
    error.to_string()
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
    noten_layout::layout(session.doc.score(), &LayoutOptions { width })
}

#[derive(Serialize)]
pub struct HitInfo {
    cursor: Cursor,
    pitch: Pitch,
    note: Option<EventId>,
}

#[tauri::command]
pub fn hit_test(state: State<AppState>, width: f32, x: f32, y: f32) -> Option<HitInfo> {
    let session = state.lock();
    let score = session.doc.score();
    let list = noten_layout::layout(score, &LayoutOptions { width });
    let hit = noten_layout::hit_test(score, &list, x, y)?;
    let cursor = Cursor {
        staff: hit.staff,
        measure: hit.measure,
        voice: 0,
        tick: hit.tick.unwrap_or(0),
    };
    let voice_end = edit::voice_end(score, hit.staff, hit.measure, 0).ok()?;
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
    Some(HitInfo {
        cursor,
        pitch: hit.pitch,
        note: edit::note_at_cursor(score, cursor),
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
) -> Reply<Edited> {
    let mut session = state.lock();
    let entry = edit::enter_step(&mut session.doc, cursor, step, duration).map_err(text)?;
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
) -> Reply<Edited> {
    let mut session = state.lock();
    edit::add_note_above(&mut session.doc, event, step).map_err(text)?;
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

/// Inserts an empty measure after `after`, or at the end if no position is given.
#[tauri::command]
pub fn insert_measure(
    app: AppHandle,
    state: State<AppState>,
    after: Option<usize>,
    cursor: Option<Cursor>,
) -> Reply<Edited> {
    let mut session = state.lock();
    let after = after.unwrap_or(session.doc.score().measures.len() - 1);
    edit::insert_measure_after(&mut session.doc, after).map_err(text)?;
    // Entry continues in the new measure.
    let cursor = cursor.map(|c| Cursor {
        measure: after + 1,
        tick: 0,
        ..c
    });
    Ok(finish(&app, &session, cursor, None))
}

/// Applies to the measure and all following ones up to the next time signature change.
#[tauri::command]
pub fn set_time_signature(
    app: AppHandle,
    state: State<AppState>,
    measure: usize,
    numerator: u8,
    denominator: u8,
) -> Reply<Edited> {
    let mut session = state.lock();
    let time_signature = TimeSignature {
        numerator,
        denominator,
    };
    edit::change_time_signature(&mut session.doc, measure, time_signature).map_err(text)?;
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

/// Asks before throwing away unsaved work; returns whether to go on.
fn confirm_discard(app: &AppHandle, state: &AppState) -> bool {
    if !state.lock().is_dirty() {
        return true;
    }
    app.dialog()
        .message("The score has unsaved changes. Discard them?")
        .title("Noten")
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
        .add_filter("Noten score", &[noten_format::EXTENSION])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(text)?;
    let score = noten_format::load(&path).map_err(text)?;
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
                .add_filter("Noten score", &[noten_format::EXTENSION])
                .set_file_name(format!("Untitled.{}", noten_format::EXTENSION))
                .blocking_save_file()
            else {
                return Ok(None);
            };
            let mut path = picked.into_path().map_err(text)?;
            if path
                .extension()
                .is_none_or(|e| e != noten_format::EXTENSION)
            {
                path.set_extension(noten_format::EXTENSION);
            }
            path
        }
    };

    let mut session = state.lock();
    noten_format::save(session.doc.score(), &path).map_err(text)?;
    session.mark_saved(path);
    update_title(&app, &session);
    Ok(Some(session.status()))
}
