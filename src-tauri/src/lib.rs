mod commands;
mod export;
mod playback;
mod session;
mod setup;

use notula_model::Document;
use tauri::{Manager, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use commands::AppState;
use session::Session;

pub fn run() {
    let session = Session::new(Document::from_score(setup::default_score()));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(session))
        .invoke_handler(tauri::generate_handler![
            commands::current_state,
            commands::render_score,
            commands::hit_test,
            commands::select_event,
            commands::move_cursor,
            commands::enter_note,
            commands::enter_step,
            commands::enter_rest,
            commands::set_duration,
            commands::toggle_articulation,
            commands::toggle_tie,
            commands::toggle_slur,
            commands::toggle_phrase_slur,
            commands::event_articulations,
            commands::delete_event,
            commands::transpose_semitones,
            commands::transpose_chord_pitch,
            commands::transpose_octaves,
            commands::set_alter,
            commands::add_note_above,
            commands::add_pitch_to_chord,
            commands::remove_pitch_from_chord,
            commands::toggle_pitch_in_chord,
            commands::insert_measure,
            commands::remove_measure,
            commands::add_voice,
            commands::remove_voice,
            commands::end_voice,
            commands::set_time_signature,
            commands::set_key_signature,
            commands::toggle_pickup,
            commands::undo,
            commands::redo,
            commands::instrument_presets,
            commands::score_properties,
            commands::update_score_properties,
            commands::new_score,
            commands::open_score,
            commands::save_score,
            commands::export_score,
            commands::select_soundfont,
            commands::restore_soundfont,
            commands::play_score,
            commands::stop_playback,
            commands::playback_status,
        ])
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            if !window.state::<AppState>().lock().is_dirty() {
                return;
            }
            // The dialog must not block the event loop, so closing continues in its callback.
            api.prevent_close();
            let window = window.clone();
            window
                .app_handle()
                .dialog()
                .message("The score has unsaved changes. Discard them?")
                .title("Notula")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Discard".to_owned(),
                    "Cancel".to_owned(),
                ))
                .show(move |discard| {
                    if discard {
                        let _ = window.destroy();
                    }
                });
        })
        .run(tauri::generate_context!())
        .expect("error while running Notula");
}
