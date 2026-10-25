use noten_model::edit::{
    self, Cursor, adjacent_event, enter_note, enter_rest, enter_step, locate, move_cursor,
    snap_cursor,
};
use noten_model::{
    Articulation, Clef, Command, Document, Duration, Dynamic, EventKind, Hairpin, KeySignature,
    MeasureInfo, ModelError, NoteValue, Pitch, Score, StaffId, Step, TimeSignature,
    TimeSignatureDisplay, VoiceLocation,
};

fn dur(value: NoteValue) -> Duration {
    Duration::new(value, 0)
}

fn setup(measures: usize) -> (Document, Cursor) {
    let mut doc = Document::new();
    let part = doc.new_part("Piano", &[Clef::Treble]);
    let staff = part.staves[0].id;
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    for _ in 1..measures {
        edit::append_measure(&mut doc).unwrap();
    }
    let cursor = Cursor {
        staff,
        measure: 0,
        voice: 0,
        tick: 0,
    };
    (doc, cursor)
}

fn events(doc: &Document, staff: StaffId, measure: usize) -> Vec<(NoteValue, Option<Pitch>)> {
    let at = VoiceLocation {
        staff,
        measure,
        voice: 0,
    };
    doc.score()
        .voice(at)
        .unwrap()
        .events
        .iter()
        .map(|e| {
            let pitch = match &e.kind {
                EventKind::Chord { pitches, .. } => Some(pitches[0]),
                EventKind::Rest => None,
            };
            (e.duration.value, pitch)
        })
        .collect()
}

fn c4() -> Pitch {
    Pitch::new(Step::C, 0, 4)
}

fn event_at(doc: &Document, cursor: Cursor, index: usize) -> noten_model::Event {
    let at = VoiceLocation {
        staff: cursor.staff,
        measure: cursor.measure,
        voice: cursor.voice,
    };
    doc.score().voice(at).unwrap().events[index].clone()
}

#[test]
fn cursor_moves_to_the_end_or_start_of_a_selected_note() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    assert_eq!(
        move_cursor(doc.score(), cursor, Some(event), true)
            .unwrap()
            .tick,
        960
    );
    assert_eq!(
        move_cursor(doc.score(), cursor, Some(event), false)
            .unwrap()
            .tick,
        0
    );
    let second = Cursor {
        tick: 960,
        ..cursor
    };
    let second_event = enter_note(&mut doc, second, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    assert_eq!(
        move_cursor(doc.score(), second, Some(second_event), false)
            .unwrap()
            .tick,
        0
    );
}

#[test]
fn note_at_cursor_selects_only_a_note_starting_at_that_position() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    assert_eq!(edit::note_at_cursor(doc.score(), cursor), Some(event));
    assert_eq!(
        edit::note_at_cursor(doc.score(), Cursor { tick: 1, ..cursor }),
        None
    );
}

#[test]
fn alternate_horizontal_movement_starts_at_the_adjacent_measure() {
    let (mut doc, cursor) = setup(2);
    edit::add_voice(&mut doc, cursor.staff, 1).unwrap();
    let second_voice = Cursor {
        measure: 1,
        voice: 1,
        ..cursor
    };
    let moved = edit::move_cursor_to_measure(doc.score(), cursor, true).unwrap();
    assert_eq!(moved.measure, 1);
    assert_eq!(moved.voice, 0);
    assert_eq!(moved.tick, 0);
    let moved_back = edit::move_cursor_to_measure(doc.score(), second_voice, false).unwrap();
    assert_eq!(moved_back.measure, 0);
    assert_eq!(moved_back.voice, 0);
    let stepped_back = edit::move_cursor(doc.score(), second_voice, None, false).unwrap();
    assert_eq!(stepped_back.measure, 0);
    assert_eq!(stepped_back.voice, 0);
}

#[test]
fn alternate_vertical_movement_keeps_nearest_time_position() {
    let mut doc = Document::new();
    let part = doc.new_part("Piano", &[Clef::Treble, Clef::Bass]);
    let treble = part.staves[0].id;
    let bass = part.staves[1].id;
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    edit::add_voice(&mut doc, treble, 0).unwrap();
    let bass_cursor = Cursor {
        staff: bass,
        measure: 0,
        voice: 0,
        tick: 800,
    };
    enter_note(
        &mut doc,
        Cursor {
            tick: 0,
            ..bass_cursor
        },
        c4(),
        dur(NoteValue::Quarter),
    )
    .unwrap();
    enter_note(
        &mut doc,
        Cursor {
            tick: 960,
            ..bass_cursor
        },
        c4(),
        dur(NoteValue::Quarter),
    )
    .unwrap();

    let moved = edit::move_cursor_to_staff(
        doc.score(),
        Cursor {
            staff: treble,
            voice: 1,
            ..bass_cursor
        },
        true,
    )
    .unwrap();
    assert_eq!(moved.staff, bass);
    assert_eq!(moved.voice, 0);
    assert_eq!(moved.tick, 960);
    let empty_staff = edit::move_cursor_to_staff(doc.score(), moved, false).unwrap();
    assert_eq!(empty_staff.staff, treble);
    assert_eq!(empty_staff.tick, 0);
}

#[test]
fn mouse_hit_in_empty_voice_ignores_other_staff_columns() {
    let (doc, cursor) = setup(1);
    let hit = edit::cursor_at_staff_hit(
        doc.score(),
        Cursor {
            tick: 1920,
            ..cursor
        },
    )
    .unwrap();
    assert_eq!(hit.tick, 0);
}

#[test]
fn mouse_hit_without_a_note_uses_clicked_voices_content_end() {
    let (mut doc, cursor) = setup(1);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    let hit = edit::cursor_at_staff_hit(
        doc.score(),
        Cursor {
            tick: 2880,
            ..cursor
        },
    )
    .unwrap();
    assert_eq!(hit.tick, 960);
}

#[test]
fn mouse_pitch_can_be_added_to_a_chord() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let d4 = Pitch::new(Step::D, 0, 4);
    edit::add_pitch_to_chord(&mut doc, event, d4).unwrap();
    edit::add_pitch_to_chord(&mut doc, event, d4).unwrap();
    let EventKind::Chord { pitches, .. } = event_at(&doc, cursor, 0).kind else {
        panic!("expected a chord");
    };
    assert_eq!(pitches, vec![c4(), d4]);
}

#[test]
fn chord_tone_removal_preserves_the_last_pitch() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let d4 = Pitch::new(Step::D, 0, 4);
    edit::add_pitch_to_chord(&mut doc, event, d4).unwrap();
    assert!(edit::remove_pitch_from_chord(&mut doc, event, d4).unwrap());
    assert!(!edit::remove_pitch_from_chord(&mut doc, event, c4()).unwrap());
    let EventKind::Chord { pitches, .. } = event_at(&doc, cursor, 0).kind else {
        panic!("expected the final chord tone to remain");
    };
    assert_eq!(pitches, vec![c4()]);
}

#[test]
fn toggling_chord_pitches_adds_and_removes_without_clearing_the_chord() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let d4 = Pitch::new(Step::D, 0, 4);
    edit::toggle_pitch_in_chord(&mut doc, event, d4).unwrap();
    edit::toggle_pitch_in_chord(&mut doc, event, d4).unwrap();
    edit::toggle_pitch_in_chord(&mut doc, event, c4()).unwrap();
    let EventKind::Chord { pitches, .. } = event_at(&doc, cursor, 0).kind else {
        panic!("expected the final chord tone to remain");
    };
    assert_eq!(pitches, vec![c4()]);
}

#[test]
fn transposing_one_chord_tone_leaves_the_other_tones_unchanged() {
    let (mut doc, cursor) = setup(1);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let e4 = Pitch::new(Step::E, 0, 4);
    edit::add_pitch_to_chord(&mut doc, event, e4).unwrap();

    let changed = edit::transpose_chord_pitch(&mut doc, event, e4, 1).unwrap();

    assert_eq!(changed, Pitch::new(Step::F, 0, 4));
    let EventKind::Chord { pitches, .. } = event_at(&doc, cursor, 0).kind else {
        panic!("expected a chord");
    };
    assert!(pitches.contains(&c4()));
    assert!(pitches.contains(&changed));
    assert!(!pitches.contains(&e4));
}

#[test]
fn cursor_reaches_incomplete_measure_end_before_empty_measure_and_distant_event() {
    let (mut doc, cursor) = setup(3);
    let first = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let later = Cursor {
        measure: 2,
        ..cursor
    };
    enter_note(&mut doc, later, c4(), dur(NoteValue::Quarter)).unwrap();

    let next = move_cursor(doc.score(), cursor, Some(first), true).unwrap();
    assert_eq!(next.measure, 0);
    assert_eq!(next.tick, 960);

    let next = move_cursor(doc.score(), next, None, true).unwrap();
    assert_eq!(next.measure, 1);
    assert_eq!(next.tick, 0);
}

#[test]
fn cursor_traverses_current_measure_before_an_empty_neighbor() {
    let (mut doc, cursor) = setup(2);
    let first = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let after_first = Cursor {
        tick: 960,
        ..cursor
    };
    enter_note(&mut doc, after_first, c4(), dur(NoteValue::Quarter)).unwrap();

    let after_selected = move_cursor(doc.score(), cursor, Some(first), true).unwrap();
    assert_eq!(after_selected.measure, 0);
    assert_eq!(after_selected.tick, 960);
    let after_second = move_cursor(doc.score(), after_selected, None, true).unwrap();
    assert_eq!(after_second.measure, 0);
    assert_eq!(after_second.tick, 1920);
    let empty_measure = move_cursor(doc.score(), after_second, None, true).unwrap();
    assert_eq!(empty_measure.measure, 1);
    assert_eq!(empty_measure.tick, 0);
}

#[test]
fn cursor_can_stop_after_the_last_note_in_an_incomplete_measure() {
    let (mut doc, cursor) = setup(2);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    let next = move_cursor(doc.score(), cursor, None, true).unwrap();
    assert_eq!(next.measure, 0);
    assert_eq!(next.tick, 960);
}

#[test]
fn selected_note_moves_to_incomplete_measure_end_before_empty_next_measure() {
    let (mut doc, cursor) = setup(2);
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    let after_note = move_cursor(doc.score(), cursor, Some(event), true).unwrap();
    assert_eq!(after_note.measure, 0);
    assert_eq!(after_note.tick, 960);
    assert_eq!(edit::note_at_cursor(doc.score(), after_note), None);

    let empty_measure = move_cursor(doc.score(), after_note, None, true).unwrap();
    assert_eq!(empty_measure.measure, 1);
    assert_eq!(empty_measure.tick, 0);
    let back_to_insert = move_cursor(doc.score(), empty_measure, None, false).unwrap();
    assert_eq!(back_to_insert.measure, 0);
    assert_eq!(back_to_insert.tick, 960);
}

#[test]
fn left_from_next_measure_stops_before_last_note_when_previous_measure_is_full() {
    let (mut doc, cursor) = setup(2);
    let mut last_event = None;
    for index in 0..4 {
        let at = Cursor {
            tick: index * 960,
            ..cursor
        };
        last_event = Some(
            enter_note(&mut doc, at, c4(), dur(NoteValue::Quarter))
                .unwrap()
                .event,
        );
    }
    let next_measure = Cursor {
        measure: 1,
        ..cursor
    };
    let event_in_next = enter_note(&mut doc, next_measure, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    let from_empty_position = move_cursor(doc.score(), next_measure, None, false).unwrap();
    assert_eq!(from_empty_position.measure, 0);
    assert_eq!(from_empty_position.tick, 2880);
    assert_eq!(
        edit::note_at_cursor(doc.score(), from_empty_position),
        last_event
    );

    let from_selected_note =
        move_cursor(doc.score(), next_measure, Some(event_in_next), false).unwrap();
    assert_eq!(from_selected_note.measure, 0);
    assert_eq!(from_selected_note.tick, 2880);
    assert_eq!(
        edit::note_at_cursor(doc.score(), from_selected_note),
        last_event
    );
}

#[test]
fn content_end_cursor_stays_insertable_only_when_measure_is_incomplete() {
    let (mut doc, cursor) = setup(2);
    let mut last = cursor;
    for index in 0..4 {
        last.tick = index * 960;
        enter_note(&mut doc, last, c4(), dur(NoteValue::Quarter)).unwrap();
    }
    assert_eq!(
        edit::cursor_at_content_end(doc.score(), cursor)
            .unwrap()
            .tick,
        2880
    );

    let incomplete = Cursor {
        measure: 1,
        ..cursor
    };
    enter_note(&mut doc, incomplete, c4(), dur(NoteValue::Quarter)).unwrap();
    assert_eq!(
        edit::cursor_at_content_end(doc.score(), incomplete)
            .unwrap()
            .tick,
        960
    );
}

#[test]
fn dynamics_and_hairpins_can_be_set_and_removed() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    edit::set_dynamic(&mut doc, id, Some(Dynamic::Mf)).unwrap();
    edit::set_hairpin(&mut doc, id, Some(Hairpin::Crescendo)).unwrap();
    let markings = event_at(&doc, cursor, 0).markings;
    assert_eq!(markings.dynamic, Some(Dynamic::Mf));
    assert_eq!(markings.hairpin, Some(Hairpin::Crescendo));

    edit::set_dynamic(&mut doc, id, None).unwrap();
    assert_eq!(event_at(&doc, cursor, 0).markings.dynamic, None);
    assert!(doc.undo().unwrap());
    assert_eq!(
        event_at(&doc, cursor, 0).markings.dynamic,
        Some(Dynamic::Mf)
    );
}

#[test]
fn articulations_toggle_and_stay_unique() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    edit::toggle_articulation(&mut doc, id, Articulation::Staccato).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Accent).unwrap();
    assert_eq!(
        event_at(&doc, cursor, 0).markings.articulations,
        vec![Articulation::Staccato, Articulation::Accent]
    );
    edit::toggle_articulation(&mut doc, id, Articulation::Staccato).unwrap();
    assert_eq!(
        event_at(&doc, cursor, 0).markings.articulations,
        vec![Articulation::Accent]
    );

    // Raw commands cannot sneak in a duplicate either.
    let mut doubled = event_at(&doc, cursor, 0);
    doubled.markings.articulations = vec![Articulation::Tenuto, Articulation::Tenuto];
    let at = VoiceLocation {
        staff: cursor.staff,
        measure: 0,
        voice: 0,
    };
    assert!(matches!(
        doc.execute(Command::ReplaceEvent {
            at,
            index: 0,
            event: doubled
        }),
        Err(ModelError::Invalid(_))
    ));
}

#[test]
fn rests_accept_dynamics_and_fermatas_only() {
    let (mut doc, cursor) = setup(1);
    let id = enter_rest(&mut doc, cursor, dur(NoteValue::Quarter))
        .unwrap()
        .event;

    edit::set_dynamic(&mut doc, id, Some(Dynamic::P)).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Fermata).unwrap();
    assert!(matches!(
        edit::toggle_articulation(&mut doc, id, Articulation::Staccato),
        Err(ModelError::Invalid(_))
    ));
}

#[test]
fn replacing_a_note_keeps_volume_marks_but_not_articulations() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    edit::set_dynamic(&mut doc, id, Some(Dynamic::Ff)).unwrap();
    edit::set_hairpin(&mut doc, id, Some(Hairpin::Diminuendo)).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Marcato).unwrap();

    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Eighth)).unwrap();
    let replaced = event_at(&doc, cursor, 0).markings;
    assert_eq!(replaced.dynamic, Some(Dynamic::Ff));
    assert_eq!(replaced.hairpin, Some(Hairpin::Diminuendo));
    assert!(replaced.articulations.is_empty());
}

#[test]
fn changing_the_duration_keeps_all_markings() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    edit::set_dynamic(&mut doc, id, Some(Dynamic::Sfz)).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Tenuto).unwrap();

    edit::set_duration(&mut doc, id, dur(NoteValue::Half)).unwrap();
    let markings = event_at(&doc, cursor, 0).markings;
    assert_eq!(markings.dynamic, Some(Dynamic::Sfz));
    assert_eq!(markings.articulations, vec![Articulation::Tenuto]);
}

#[test]
fn deleting_a_note_keeps_what_still_applies_to_a_rest() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    edit::set_dynamic(&mut doc, id, Some(Dynamic::P)).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Staccato).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Fermata).unwrap();

    edit::delete_event(&mut doc, id).unwrap();
    let rest = event_at(&doc, cursor, 0);
    assert_eq!(rest.kind, EventKind::Rest);
    assert_eq!(rest.markings.dynamic, Some(Dynamic::P));
    assert_eq!(rest.markings.articulations, vec![Articulation::Fermata]);
}

#[test]
fn markings_survive_json_and_are_omitted_when_empty() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let plain = serde_json::to_value(event_at(&doc, cursor, 0)).unwrap();
    assert!(plain.get("markings").is_none());

    edit::set_dynamic(&mut doc, id, Some(Dynamic::Mp)).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Accent).unwrap();
    let marked = event_at(&doc, cursor, 0);
    let json = serde_json::to_string(&marked).unwrap();
    assert_eq!(
        serde_json::from_str::<noten_model::Event>(&json).unwrap(),
        marked
    );

    // Events written before markings existed have no such key.
    let old = serde_json::from_value::<noten_model::Event>(plain).unwrap();
    assert!(old.markings.is_empty());
}

fn set_length(doc: &mut Document, length: Option<u32>) -> Result<(), ModelError> {
    doc.execute(Command::SetMeasureLength { measure: 0, length })
}

#[test]
fn pickup_measures_hold_only_their_length() {
    let (mut doc, cursor) = setup(1);
    set_length(&mut doc, Some(960)).unwrap();

    let entry = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    // The pickup is full, so entry continues in a new, regular measure.
    assert_eq!(entry.next.measure, 1);
    assert_eq!(doc.score().measures.len(), 2);
    assert_eq!(doc.score().measures[1].length, None);

    let again = Cursor {
        tick: 960,
        ..cursor
    };
    assert_eq!(
        enter_note(&mut doc, again, c4(), dur(NoteValue::Quarter)),
        Err(ModelError::MeasureOverfull)
    );
}

#[test]
fn new_measures_do_not_inherit_a_pickup_length() {
    let (mut doc, _) = setup(1);
    set_length(&mut doc, Some(960)).unwrap();
    edit::append_measure(&mut doc).unwrap();
    assert_eq!(doc.score().measures[1].length, None);
}

#[test]
fn toggling_the_pickup_fits_the_measure_to_its_content() {
    let (mut doc, cursor) = setup(1);
    assert!(matches!(
        edit::toggle_pickup(&mut doc, 0),
        Err(ModelError::Invalid(_))
    ));

    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    edit::toggle_pickup(&mut doc, 0).unwrap();
    assert_eq!(doc.score().measures[0].length, Some(960));
    assert_eq!(doc.score().measure_capacity(0), Ok(960));

    edit::toggle_pickup(&mut doc, 0).unwrap();
    assert_eq!(doc.score().measures[0].length, None);

    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures[0].length, Some(960));
}

#[test]
fn a_full_measure_cannot_become_a_pickup() {
    let (mut doc, cursor) = setup(1);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Whole)).unwrap();
    assert!(matches!(
        edit::toggle_pickup(&mut doc, 0),
        Err(ModelError::Invalid(_))
    ));
}

#[test]
fn measure_length_is_checked_against_content_and_the_time_signature() {
    let (mut doc, cursor) = setup(1);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Half)).unwrap();
    assert_eq!(
        set_length(&mut doc, Some(960)),
        Err(ModelError::MeasureOverfull)
    );
    assert!(matches!(
        set_length(&mut doc, Some(100)),
        Err(ModelError::Invalid(_))
    ));
    assert!(matches!(
        set_length(&mut doc, Some(5000)),
        Err(ModelError::Invalid(_))
    ));

    // Spelling out the full length is the same as having no override.
    set_length(&mut doc, Some(3840)).unwrap();
    assert_eq!(doc.score().measures[0].length, None);
}

#[test]
fn time_signature_changes_respect_the_measure_length() {
    let (mut doc, _) = setup(1);
    set_length(&mut doc, Some(960)).unwrap();
    let change = |numerator, denominator| Command::SetTimeSignature {
        measure: 0,
        time_signature: TimeSignature {
            numerator,
            denominator,
        },
    };
    assert!(matches!(
        doc.execute(change(1, 8)),
        Err(ModelError::Invalid(_))
    ));
    doc.execute(change(3, 4)).unwrap();
    assert_eq!(doc.score().measures[0].length, Some(960));
}

#[test]
fn invalid_lengths_in_loaded_scores_are_caught() {
    let mut score = Score::new();
    score.validate().unwrap();
    score.measures[0].length = Some(7);
    assert!(score.validate().is_err());
    score.measures[0].length = Some(999 * 60);
    assert!(score.validate().is_err());
}

#[test]
fn old_files_without_a_length_still_load() {
    let json = serde_json::to_string(&MeasureInfo::default()).unwrap();
    assert!(!json.contains("length"));
    let info: MeasureInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(info.length, None);

    let with_length = MeasureInfo {
        length: Some(960),
        ..MeasureInfo::default()
    };
    let json = serde_json::to_string(&with_length).unwrap();
    assert_eq!(
        serde_json::from_str::<MeasureInfo>(&json).unwrap(),
        with_length
    );
}

fn time(numerator: u8, denominator: u8) -> TimeSignature {
    TimeSignature {
        numerator,
        denominator,
    }
}

fn signatures(doc: &Document) -> Vec<TimeSignature> {
    doc.score()
        .measures
        .iter()
        .map(|m| m.time_signature)
        .collect()
}

#[test]
fn a_time_signature_change_lasts_until_the_next_change() {
    let (mut doc, _) = setup(6);
    edit::change_time_signature(&mut doc, 2, time(3, 4)).unwrap();
    assert_eq!(
        signatures(&doc),
        [4, 4, 3, 3, 3, 3].map(|n| time(n, 4)).to_vec()
    );

    // Changing the first section stops where the 3/4 section begins.
    edit::change_time_signature(&mut doc, 0, time(2, 4)).unwrap();
    assert_eq!(
        signatures(&doc),
        [2, 2, 3, 3, 3, 3].map(|n| time(n, 4)).to_vec()
    );

    // And a later change only affects its own section.
    edit::change_time_signature(&mut doc, 4, time(6, 8)).unwrap();
    assert_eq!(doc.score().measures[3].time_signature, time(3, 4));
    assert_eq!(doc.score().measures[5].time_signature, time(6, 8));

    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures[5].time_signature, time(3, 4));
}

#[test]
fn common_and_alla_breve_styles_change_the_meter_and_undo_as_one_edit() {
    let (mut doc, _) = setup(5);
    edit::change_time_signature_with_display(
        &mut doc,
        3,
        TimeSignature {
            numerator: 2,
            denominator: 2,
        },
        TimeSignatureDisplay::AllaBreve,
    )
    .unwrap();
    edit::change_time_signature_with_display(
        &mut doc,
        1,
        TimeSignature::COMMON,
        TimeSignatureDisplay::Common,
    )
    .unwrap();

    assert_eq!(
        doc.score().measures[0].time_signature_display,
        TimeSignatureDisplay::Numeric
    );
    assert_eq!(
        doc.score().measures[1].time_signature,
        TimeSignature::COMMON
    );
    assert_eq!(
        doc.score().measures[1].time_signature_display,
        TimeSignatureDisplay::Common
    );
    assert_eq!(
        doc.score().measures[2].time_signature_display,
        TimeSignatureDisplay::Common
    );
    assert_eq!(
        doc.score().measures[3].time_signature_display,
        TimeSignatureDisplay::AllaBreve
    );
    assert_eq!(
        doc.score().measures[3].time_signature.ticks_per_measure(),
        3840
    );

    assert!(doc.undo().unwrap());
    assert_eq!(
        doc.score().measures[1].time_signature_display,
        TimeSignatureDisplay::Numeric
    );
    assert!(doc.undo().unwrap());
    assert_eq!(
        doc.score().measures[3].time_signature_display,
        TimeSignatureDisplay::Numeric
    );
    assert_eq!(
        doc.score().measures[3].time_signature,
        TimeSignature::COMMON
    );
}

#[test]
fn a_key_signature_change_lasts_until_the_next_change_and_undoes() {
    let (mut doc, _) = setup(6);
    doc.execute(Command::SetKeySignature {
        measure: 3,
        key_signature: KeySignature { fifths: 4 },
    })
    .unwrap();

    edit::change_key_signature(&mut doc, 1, KeySignature { fifths: -2 }).unwrap();
    assert_eq!(
        doc.score()
            .measures
            .iter()
            .map(|info| info.key_signature.fifths)
            .collect::<Vec<_>>(),
        [0, -2, -2, 4, 0, 0]
    );

    edit::change_key_signature(&mut doc, 4, KeySignature { fifths: 2 }).unwrap();
    assert_eq!(doc.score().measures[4].key_signature.fifths, 2);
    assert_eq!(doc.score().measures[5].key_signature.fifths, 2);
    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures[4].key_signature.fifths, 0);
    assert_eq!(doc.score().measures[5].key_signature.fifths, 0);
    assert_eq!(
        edit::change_key_signature(&mut doc, 0, KeySignature { fifths: 8 }),
        Err(ModelError::Invalid("key signature must be within -7..=7"))
    );
}

#[test]
fn a_time_signature_change_must_fit_all_affected_measures() {
    let (mut doc, mut cursor) = setup(4);
    cursor.measure = 3;
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Whole)).unwrap();
    let before = doc.score().clone();

    assert_eq!(
        edit::change_time_signature(&mut doc, 1, time(3, 4)),
        Err(ModelError::MeasureOverfull)
    );
    assert_eq!(doc.score(), &before);
}

#[test]
fn inserting_a_measure_follows_the_signature_of_its_section() {
    let (mut doc, cursor) = setup(3);
    edit::change_time_signature(&mut doc, 1, time(3, 4)).unwrap();
    let second = Cursor {
        measure: 2,
        ..cursor
    };
    let entry = enter_note(&mut doc, second, c4(), dur(NoteValue::Quarter)).unwrap();

    // After a 4/4 measure that is followed by 3/4 measures, the new one is 4/4.
    edit::insert_measure_after(&mut doc, 0).unwrap();
    assert_eq!(doc.score().measures.len(), 4);
    assert_eq!(doc.score().measures[1].time_signature, time(4, 4));
    // Inside the 3/4 section it is 3/4, and later measures move back.
    edit::insert_measure_after(&mut doc, 2).unwrap();
    assert_eq!(doc.score().measures[3].time_signature, time(3, 4));
    assert_eq!(locate(doc.score(), entry.event).unwrap().cursor.measure, 4);

    assert_eq!(
        edit::insert_measure_after(&mut doc, 99),
        Err(ModelError::MeasureOutOfRange(99))
    );
}

#[test]
fn inserting_before_the_first_measure_inherits_its_signature() {
    let (mut doc, _) = setup(1);
    edit::change_time_signature(&mut doc, 0, time(3, 4)).unwrap();

    edit::insert_measure_at(&mut doc, 0).unwrap();

    assert_eq!(doc.score().measures.len(), 2);
    assert_eq!(doc.score().measures[0].time_signature, time(3, 4));
    assert_eq!(doc.score().measures[1].time_signature, time(3, 4));
    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures.len(), 1);
}

#[test]
fn an_empty_trailing_measure_from_note_entry_can_be_removed_and_restored() {
    let (mut doc, mut cursor) = setup(1);
    for _ in 0..4 {
        cursor = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
            .unwrap()
            .next;
    }
    assert_eq!(doc.score().measures.len(), 2);
    assert!(events(&doc, cursor.staff, 1).is_empty());

    edit::remove_measure(&mut doc, 1).unwrap();
    assert_eq!(doc.score().measures.len(), 1);
    assert_eq!(events(&doc, cursor.staff, 0).len(), 4);

    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures.len(), 2);
    assert!(events(&doc, cursor.staff, 1).is_empty());
}

#[test]
fn entering_notes_advances_and_grows_the_score() {
    let (mut doc, mut cursor) = setup(1);
    for _ in 0..4 {
        cursor = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
            .unwrap()
            .next;
    }
    assert_eq!(doc.score().measures.len(), 2);
    assert_eq!(cursor.measure, 1);
    assert_eq!(cursor.tick, 0);
    assert_eq!(events(&doc, cursor.staff, 0).len(), 4);

    assert!(doc.undo().unwrap());
    assert_eq!(doc.score().measures.len(), 1);
    assert_eq!(events(&doc, cursor.staff, 0).len(), 3);
}

#[test]
fn shorter_notes_leave_rests_and_one_undo_restores() {
    let (mut doc, cursor) = setup(1);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Half)).unwrap();
    let before = doc.score().clone();

    let entry = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    assert_eq!(entry.next.tick, 960);
    assert_eq!(
        events(&doc, cursor.staff, 0),
        vec![(NoteValue::Quarter, Some(c4())), (NoteValue::Quarter, None),]
    );

    doc.undo().unwrap();
    assert_eq!(doc.score(), &before);
}

#[test]
fn longer_notes_swallow_following_events() {
    let (mut doc, mut cursor) = setup(1);
    for _ in 0..4 {
        cursor = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Eighth))
            .unwrap()
            .next;
    }
    let start = Cursor { tick: 0, ..cursor };
    enter_note(&mut doc, start, c4(), dur(NoteValue::Quarter)).unwrap();
    assert_eq!(
        events(&doc, cursor.staff, 0)
            .iter()
            .map(|e| e.0)
            .collect::<Vec<_>>(),
        vec![NoteValue::Quarter, NoteValue::Eighth, NoteValue::Eighth]
    );

    // A half note over a quarter + two eighths leaves nothing behind.
    enter_note(&mut doc, start, c4(), dur(NoteValue::Half)).unwrap();
    assert_eq!(events(&doc, cursor.staff, 0).len(), 1);
}

#[test]
fn overfull_entries_are_rejected_without_changes() {
    let (mut doc, cursor) = setup(1);
    let entry = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    let before = doc.score().clone();
    assert_eq!(
        enter_note(&mut doc, entry.next, c4(), dur(NoteValue::Whole)),
        Err(ModelError::MeasureOverfull)
    );
    assert_eq!(doc.score(), &before);
}

#[test]
fn cursor_inside_an_event_snaps_to_its_start() {
    let (mut doc, cursor) = setup(1);
    enter_note(&mut doc, cursor, c4(), dur(NoteValue::Half)).unwrap();
    let inside = Cursor {
        tick: 500,
        ..cursor
    };
    assert_eq!(snap_cursor(doc.score(), inside).unwrap().tick, 0);
    let beyond = Cursor {
        tick: 3000,
        ..cursor
    };
    assert_eq!(snap_cursor(doc.score(), beyond).unwrap().tick, 1920);
}

#[test]
fn rests_can_be_entered() {
    let (mut doc, cursor) = setup(1);
    enter_rest(&mut doc, cursor, dur(NoteValue::Quarter)).unwrap();
    assert_eq!(
        events(&doc, cursor.staff, 0),
        vec![(NoteValue::Quarter, None)]
    );
}

#[test]
fn letters_choose_the_octave_nearest_the_previous_note() {
    let (mut doc, cursor) = setup(1);
    let first = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    let second = enter_step(&mut doc, first.next, Step::G, dur(NoteValue::Quarter)).unwrap();
    enter_step(&mut doc, second.next, Step::C, dur(NoteValue::Quarter)).unwrap();

    let pitches: Vec<_> = events(&doc, cursor.staff, 0)
        .into_iter()
        .filter_map(|e| e.1)
        .collect();
    // G is a fourth below C4, then C is a fourth above G3.
    assert_eq!(pitches[1], Pitch::new(Step::G, 0, 3));
    assert_eq!(pitches[2], Pitch::new(Step::C, 0, 4));
}

#[test]
fn letters_follow_the_key_signature() {
    let (mut doc, cursor) = setup(1);
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature: KeySignature { fifths: 2 },
    })
    .unwrap();
    enter_step(&mut doc, cursor, Step::F, dur(NoteValue::Quarter)).unwrap();
    assert_eq!(events(&doc, cursor.staff, 0)[0].1.unwrap().alter, 1);
}

#[test]
fn natural_override_enters_a_note_against_the_key_signature() {
    let (mut doc, cursor) = setup(1);
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature: KeySignature { fifths: 1 },
    })
    .unwrap();

    edit::enter_step_with_alter(&mut doc, cursor, Step::F, dur(NoteValue::Quarter), Some(0))
        .unwrap();

    assert_eq!(
        events(&doc, cursor.staff, 0)[0].1.unwrap(),
        Pitch::new(Step::F, 0, 4)
    );
}

#[test]
fn natural_override_adds_a_natural_pitch_to_a_chord() {
    let (mut doc, cursor) = setup(1);
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature: KeySignature { fifths: 1 },
    })
    .unwrap();
    let event = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;

    edit::add_note_above_with_alter(&mut doc, event, Step::F, Some(0)).unwrap();

    let EventKind::Chord { pitches, .. } = event_at(&doc, cursor, 0).kind else {
        panic!("expected a chord");
    };
    assert!(pitches.contains(&Pitch::new(Step::F, 0, 4)));
}

#[test]
fn deleting_turns_a_note_into_a_rest_and_keeps_its_id() {
    let (mut doc, cursor) = setup(1);
    let entry = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    assert!(edit::delete_event(&mut doc, entry.event).unwrap());
    assert_eq!(
        events(&doc, cursor.staff, 0),
        vec![(NoteValue::Quarter, None)]
    );
    assert!(locate(doc.score(), entry.event).is_some());
    assert!(!edit::delete_event(&mut doc, entry.event).unwrap());
}

#[test]
fn changing_the_duration_keeps_pitch_and_id() {
    let (mut doc, cursor) = setup(1);
    let entry = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter)).unwrap();
    let changed = edit::set_duration(&mut doc, entry.event, dur(NoteValue::Half)).unwrap();
    assert_eq!(changed.event, entry.event);
    assert_eq!(
        events(&doc, cursor.staff, 0),
        vec![(NoteValue::Half, Some(c4()))]
    );
}

#[test]
fn transposing_spells_by_direction() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    let pitch = |doc: &Document| events(doc, cursor.staff, 0)[0].1.unwrap();

    edit::transpose_semitones(&mut doc, id, 1).unwrap();
    assert_eq!(pitch(&doc), Pitch::new(Step::C, 1, 4));
    edit::transpose_semitones(&mut doc, id, -2).unwrap();
    assert_eq!(pitch(&doc), Pitch::new(Step::B, 0, 3));
    edit::transpose_octaves(&mut doc, id, 1).unwrap();
    assert_eq!(pitch(&doc), Pitch::new(Step::B, 0, 4));
    edit::set_alter(&mut doc, id, -1).unwrap();
    assert_eq!(pitch(&doc), Pitch::new(Step::B, -1, 4));
    edit::transpose_semitones(&mut doc, id, -1).unwrap();
    assert_eq!(pitch(&doc), Pitch::new(Step::A, 0, 4));

    let before = doc.score().clone();
    assert!(edit::transpose_octaves(&mut doc, id, 10).is_err());
    assert_eq!(doc.score(), &before);
}

#[test]
fn chord_tones_stack_upwards() {
    let (mut doc, cursor) = setup(1);
    let id = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Quarter))
        .unwrap()
        .event;
    edit::add_note_above(&mut doc, id, Step::E).unwrap();
    edit::add_note_above(&mut doc, id, Step::C).unwrap();

    let at = VoiceLocation {
        staff: cursor.staff,
        measure: 0,
        voice: 0,
    };
    let EventKind::Chord { pitches, .. } = &doc.score().voice(at).unwrap().events[0].kind else {
        panic!("expected a chord");
    };
    assert_eq!(
        pitches,
        &vec![c4(), Pitch::new(Step::E, 0, 4), Pitch::new(Step::C, 0, 5)]
    );
}

#[test]
fn navigation_crosses_measures() {
    let (mut doc, cursor) = setup(2);
    let a = enter_note(&mut doc, cursor, c4(), dur(NoteValue::Whole)).unwrap();
    let b = enter_note(&mut doc, a.next, c4(), dur(NoteValue::Whole)).unwrap();
    assert_eq!(a.next.measure, 1);

    assert_eq!(adjacent_event(doc.score(), a.event, true), Some(b.event));
    assert_eq!(adjacent_event(doc.score(), b.event, false), Some(a.event));
    assert_eq!(adjacent_event(doc.score(), a.event, false), None);
    assert_eq!(adjacent_event(doc.score(), b.event, true), None);
}

#[test]
fn unknown_events_are_reported() {
    let (mut doc, _) = setup(1);
    let missing = doc.new_event_id();
    assert_eq!(
        edit::delete_event(&mut doc, missing),
        Err(ModelError::EventNotFound(missing))
    );
}
