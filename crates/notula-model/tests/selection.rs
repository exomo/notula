use notula_model::edit::{self, Cursor, enter_note};
use notula_model::selection::{MeasureSpan, Selection, Span, Step};
use notula_model::{
    Clef, Command, Document, Duration, EventId, NoteValue, Pitch, StaffId, Step as PitchStep,
};

/// A piano with two measures: four quarters in the upper staff and a whole note in the lower
/// staff of measure 1; measure 2 is empty in both.
fn piano() -> (Document, [StaffId; 2], Vec<EventId>, EventId) {
    let mut doc = Document::new();
    let part = doc.new_part("Piano", &[Clef::Treble, Clef::Bass]);
    let staves = [part.staves[0].id, part.staves[1].id];
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    edit::append_measure(&mut doc).unwrap();
    let mut cursor = Cursor {
        staff: staves[0],
        measure: 0,
        voice: 0,
        tick: 0,
    };
    let quarters = (0..4)
        .map(|_| {
            let entry = enter_note(
                &mut doc,
                cursor,
                Pitch::new(PitchStep::C, 0, 5),
                Duration::new(NoteValue::Quarter, 0),
            )
            .unwrap();
            cursor = entry.next;
            entry.event
        })
        .collect();
    let whole = enter_note(
        &mut doc,
        Cursor {
            staff: staves[1],
            measure: 0,
            voice: 0,
            tick: 0,
        },
        Pitch::new(PitchStep::C, 0, 3),
        Duration::new(NoteValue::Whole, 0),
    )
    .unwrap()
    .event;
    (doc, staves, quarters, whole)
}

fn caret(doc: &Document, staff: StaffId, measure: usize, tick: u32) -> Selection {
    Selection::caret(doc.score(), staff, 0, measure, tick).unwrap()
}

#[test]
fn a_cursor_selects_nothing_until_it_is_extended_like_text() {
    let (doc, staves, quarters, _) = piano();
    let score = doc.score();
    let start = caret(&doc, staves[0], 0, 960);
    let resolved = start.resolve(score).unwrap();
    assert!(start.is_caret());
    assert!(resolved.events.is_empty() && resolved.measures.is_empty());
    assert_eq!(resolved.caret, 960);

    // Shift+right selects the next note and puts the cursor after it.
    let one = start.extend(score, Step::Next);
    let resolved = one.resolve(score).unwrap();
    assert_eq!(
        (resolved.span, resolved.caret),
        (
            Span {
                start: 960,
                end: 1920
            },
            1920
        )
    );
    assert_eq!(resolved.events, vec![quarters[1]]);
    let two = one.extend(score, Step::Next);
    assert_eq!(
        two.span(),
        Span {
            start: 960,
            end: 2880
        }
    );

    // Stepping back past the start selects to its left, with the cursor at the left edge.
    let back = two
        .extend(score, Step::Previous)
        .extend(score, Step::Previous)
        .extend(score, Step::Previous);
    let resolved = back.resolve(score).unwrap();
    assert_eq!(
        (resolved.span, resolved.caret),
        (Span { start: 0, end: 960 }, 0)
    );
    assert_eq!(
        back.extend(score, Step::Previous),
        back,
        "nothing before the start"
    );

    // An empty measure is one step.
    let empty = caret(&doc, staves[0], 1, 0).extend(score, Step::Next);
    let resolved = empty.resolve(score).unwrap();
    assert_eq!(
        resolved.span,
        Span {
            start: 3840,
            end: 7680
        }
    );
    assert!(resolved.events.is_empty());
    assert_eq!(
        resolved.measures,
        vec![MeasureSpan {
            measure: 1,
            start: 0,
            end: 3840,
            to_end: true,
        }]
    );
}

#[test]
fn measure_steps_stop_at_barlines() {
    let (doc, staves, _, _) = piano();
    let score = doc.score();
    let start = caret(&doc, staves[0], 0, 960);
    let first = start.extend(score, Step::NextMeasure);
    assert_eq!(
        first.span(),
        Span {
            start: 960,
            end: 3840
        }
    );
    let second = first.extend(score, Step::NextMeasure);
    assert_eq!(
        second.span(),
        Span {
            start: 960,
            end: 7680
        }
    );
    assert_eq!(
        second.extend(score, Step::NextMeasure),
        second,
        "no barline after the end"
    );
    let back = second
        .extend(score, Step::PreviousMeasure)
        .extend(score, Step::PreviousMeasure);
    assert_eq!(back.span(), Span { start: 0, end: 960 });
}

#[test]
fn a_cursor_spans_staves_and_grows_to_whole_notes_once_extended() {
    let (doc, staves, quarters, whole) = piano();
    let score = doc.score();
    let spanning = caret(&doc, staves[0], 0, 0).extend(score, Step::StaffDown);
    let resolved = spanning.resolve(score).unwrap();
    assert!(spanning.is_caret());
    assert_eq!(resolved.staves, staves.to_vec());
    assert!(resolved.events.is_empty());
    assert_eq!(
        spanning.extend(score, Step::StaffDown),
        spanning,
        "no staff below"
    );

    // One step selects the first quarter and the whole note, which grows it to the barline
    // and brings in the quarters starting in the grown part; the cursor follows the growth.
    let resolved = spanning.extend(score, Step::Next).resolve(score).unwrap();
    assert_eq!(
        (resolved.span, resolved.caret),
        (
            Span {
                start: 0,
                end: 3840
            },
            3840
        )
    );
    let mut events = resolved.events.clone();
    events.sort_by_key(|id| id.0);
    let mut expected = [quarters.clone(), vec![whole]].concat();
    expected.sort_by_key(|id| id.0);
    assert_eq!(events, expected);

    let up = spanning.extend(score, Step::StaffUp);
    assert_eq!(up.resolve(score).unwrap().staves, vec![staves[0]]);
}

#[test]
fn a_voice_filter_limits_the_selection() {
    let (mut doc, staves, quarters, _) = piano();
    edit::add_voice(&mut doc, staves[0], 0).unwrap();
    let second = enter_note(
        &mut doc,
        Cursor {
            staff: staves[0],
            measure: 0,
            voice: 1,
            tick: 0,
        },
        Pitch::new(PitchStep::E, 0, 4),
        Duration::new(NoteValue::Half, 0),
    )
    .unwrap()
    .event;
    let score = doc.score();

    let mut selection = Selection::for_event(score, quarters[0])
        .unwrap()
        .extend(score, Step::Next);
    assert!(selection.resolve(score).unwrap().events.contains(&second));
    selection.voice = Some(0);
    let only_first = selection.resolve(score).unwrap();
    assert!(!only_first.events.contains(&second));
    assert_eq!(only_first.events, vec![quarters[0], quarters[1]]);
}

#[test]
fn clicks_extend_in_either_direction_and_select_all_covers_the_score() {
    let (doc, staves, quarters, whole) = piano();
    let score = doc.score();
    let start = caret(&doc, staves[0], 0, 1920);

    let later = start.extend_to_event(score, quarters[3]).unwrap();
    let resolved = later.resolve(score).unwrap();
    assert_eq!(
        (resolved.span, resolved.caret),
        (
            Span {
                start: 1920,
                end: 3840
            },
            3840
        )
    );

    // The whole note starts before the cursor: the selection reaches back to its start.
    let earlier = start.extend_to_event(score, whole).unwrap();
    let resolved = earlier.resolve(score).unwrap();
    assert_eq!(resolved.staves, staves.to_vec());
    assert_eq!(
        (resolved.span, resolved.caret),
        (
            Span {
                start: 0,
                end: 3840
            },
            0
        )
    );

    let all = Selection::all(score).unwrap().resolve(score).unwrap();
    assert_eq!(all.staves, staves.to_vec());
    assert_eq!(
        all.span,
        Span {
            start: 0,
            end: 7680
        }
    );
    assert_eq!(all.events.len(), 5);

    let box_drawn = Selection::range(
        staves[1],
        staves[1],
        Span {
            start: 960,
            end: 1920,
        },
    );
    assert!(
        box_drawn.resolve(score).unwrap().events.is_empty(),
        "the whole note starts before"
    );

    // A barline position belongs to the next measure, except at the end of the score.
    assert_eq!(notula_model::selection::position(score, 3840), (1, 0));
    assert_eq!(notula_model::selection::position(score, 7680), (1, 3840));
    assert_eq!(notula_model::selection::absolute(score, 1, 960), Some(4800));
}

fn pitches(doc: &Document, id: EventId) -> Vec<Pitch> {
    let located = edit::locate(doc.score(), id).unwrap();
    let staff = doc.score().staff(located.cursor.staff).unwrap();
    match &staff.measures[located.cursor.measure].voices[located.cursor.voice].events[located.index]
        .kind
    {
        notula_model::EventKind::Chord { pitches, .. } => pitches.clone(),
        notula_model::EventKind::Rest => Vec::new(),
    }
}

#[test]
fn selected_notes_transpose_together_and_undo_in_one_step() {
    let (mut doc, _, quarters, whole) = piano();
    let selection = Selection::for_event(doc.score(), quarters[0])
        .unwrap()
        .extend(doc.score(), Step::StaffDown);
    let events = selection.resolve(doc.score()).unwrap().events;
    assert_eq!(events.len(), 5, "four quarters and the whole note");

    assert_eq!(edit::transpose_events(&mut doc, &events, 2, 0).unwrap(), 5);
    assert_eq!(
        pitches(&doc, quarters[0]),
        vec![Pitch::new(PitchStep::D, 0, 5)]
    );
    assert_eq!(pitches(&doc, whole), vec![Pitch::new(PitchStep::D, 0, 3)]);
    edit::transpose_events(&mut doc, &events, 0, 1).unwrap();
    assert_eq!(pitches(&doc, whole), vec![Pitch::new(PitchStep::D, 0, 4)]);
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(pitches(&doc, whole), vec![Pitch::new(PitchStep::C, 0, 3)]);

    // Too far for one note means no note moves.
    let before = doc.score().clone();
    assert!(edit::transpose_events(&mut doc, &events, 0, 10).is_err());
    assert_eq!(doc.score(), &before);
}

#[test]
fn selected_notes_are_deleted_to_rests() {
    let (mut doc, _, quarters, _) = piano();
    let selection = Selection::for_event(doc.score(), quarters[1])
        .unwrap()
        .extend(doc.score(), Step::Next);
    let events = selection.resolve(doc.score()).unwrap().events;
    assert_eq!(edit::delete_events(&mut doc, &events).unwrap(), 2);
    assert!(pitches(&doc, quarters[1]).is_empty());
    assert!(pitches(&doc, quarters[2]).is_empty());
    assert_eq!(pitches(&doc, quarters[0]).len(), 1);
    // Deleting rests again changes nothing and adds no undo step.
    assert_eq!(edit::delete_events(&mut doc, &events).unwrap(), 0);
    doc.undo().unwrap();
    assert_eq!(pitches(&doc, quarters[2]).len(), 1);
}

#[test]
fn articulations_toggle_for_the_whole_selection() {
    use notula_model::Articulation;
    let (mut doc, _, quarters, _) = piano();
    edit::toggle_articulation(&mut doc, quarters[0], Articulation::Staccato).unwrap();
    edit::delete_event(&mut doc, quarters[3]).unwrap();
    let events = Selection::for_event(doc.score(), quarters[0])
        .unwrap()
        .extend(doc.score(), Step::NextMeasure)
        .resolve(doc.score())
        .unwrap()
        .events;
    assert_eq!(events.len(), 4);
    assert!(edit::common_articulations(doc.score(), &events).is_empty());

    // Only one note had it, so all notes get it; the rest is skipped.
    assert_eq!(
        edit::toggle_articulation_events(&mut doc, &events, Articulation::Staccato).unwrap(),
        2
    );
    assert_eq!(
        edit::common_articulations(doc.score(), &events),
        vec![Articulation::Staccato]
    );
    // Now all have it, so it is removed from all.
    assert_eq!(
        edit::toggle_articulation_events(&mut doc, &events, Articulation::Staccato).unwrap(),
        3
    );
    assert!(edit::common_articulations(doc.score(), &events).is_empty());
    // A fermata may also go on the rest.
    assert_eq!(
        edit::toggle_articulation_events(&mut doc, &events, Articulation::Fermata).unwrap(),
        4
    );
}

#[test]
fn entering_in_several_staves_is_one_undo_step_or_nothing() {
    let (mut doc, staves, _, _) = piano();
    let cursor = Cursor {
        staff: staves[0],
        measure: 1,
        voice: 0,
        tick: 0,
    };
    let entry = edit::enter_in_staves(&mut doc, cursor, &staves, |doc, at| {
        edit::enter_step(doc, at, PitchStep::G, Duration::new(NoteValue::Half, 0))
    })
    .unwrap();
    assert_eq!(entry.next.tick, 1920);
    let first_event = |doc: &Document, staff: StaffId| {
        doc.score().staff(staff).unwrap().measures[1].voices[0]
            .events
            .first()
            .map(|event| event.id)
    };
    let upper = first_event(&doc, staves[0]).unwrap();
    let lower = first_event(&doc, staves[1]).unwrap();
    // Same letter, octave nearest each staff's previous note.
    assert_eq!(pitches(&doc, upper), vec![Pitch::new(PitchStep::G, 0, 4)]);
    assert_eq!(pitches(&doc, lower), vec![Pitch::new(PitchStep::G, 0, 2)]);
    doc.undo().unwrap();
    assert!(first_event(&doc, staves[0]).is_none() && first_event(&doc, staves[1]).is_none());

    // A failure in one staff leaves both unchanged.
    doc.redo().unwrap();
    let before = doc.score().clone();
    let mut calls = 0;
    let failed = edit::enter_in_staves(&mut doc, cursor, &staves, |doc, at| {
        calls += 1;
        if calls == 2 {
            return Err(notula_model::ModelError::Invalid("refused"));
        }
        edit::enter_step(doc, at, PitchStep::A, Duration::new(NoteValue::Quarter, 0))
    });
    assert!(failed.is_err());
    assert_eq!(doc.score(), &before);
    assert!(doc.can_redo() || doc.can_undo());
}
