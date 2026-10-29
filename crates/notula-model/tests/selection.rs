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

#[test]
fn keyboard_steps_follow_the_anchor_voice() {
    let (doc, staves, quarters, _) = piano();
    let score = doc.score();
    let start = Selection::for_event(score, quarters[1]).unwrap();
    assert_eq!(
        start.span(),
        Span {
            start: 960,
            end: 1920
        }
    );

    let right = start.extend(score, Step::Next);
    assert_eq!(
        right.span(),
        Span {
            start: 960,
            end: 2880
        }
    );
    // Stepping back past the anchor selects to its left instead.
    let left = right
        .extend(score, Step::Previous)
        .extend(score, Step::Previous);
    assert_eq!(
        left.span(),
        Span {
            start: 0,
            end: 1920
        }
    );
    assert_eq!(
        left.extend(score, Step::Previous),
        left,
        "no step before the first note"
    );

    // The empty second measure counts as one step.
    let into_empty = Selection::for_event(score, quarters[3])
        .unwrap()
        .extend(score, Step::Next);
    assert_eq!(
        into_empty.span(),
        Span {
            start: 2880,
            end: 7680
        }
    );

    let resolved = start.resolve(score).unwrap();
    assert_eq!(resolved.staves, vec![staves[0]]);
    assert_eq!(resolved.events, vec![quarters[1]]);
}

#[test]
fn measure_steps_move_the_moving_edge_to_barlines() {
    let (doc, _, quarters, _) = piano();
    let score = doc.score();
    let start = Selection::for_event(score, quarters[1]).unwrap();

    let to_measure_end = start.extend(score, Step::NextMeasure);
    assert_eq!(
        to_measure_end.span(),
        Span {
            start: 960,
            end: 3840
        }
    );
    let next_measure = to_measure_end.extend(score, Step::NextMeasure);
    assert_eq!(
        next_measure.span(),
        Span {
            start: 960,
            end: 7680
        }
    );
    assert_eq!(
        next_measure.extend(score, Step::NextMeasure),
        next_measure,
        "no measure after the last"
    );

    // Stepping back shrinks to the previous barline, then crosses the anchor.
    let back = next_measure.extend(score, Step::PreviousMeasure);
    assert_eq!(
        back.span(),
        Span {
            start: 960,
            end: 3840
        }
    );
    let past_anchor = back.extend(score, Step::PreviousMeasure);
    assert_eq!(
        past_anchor.span(),
        Span {
            start: 0,
            end: 1920
        }
    );
    assert_eq!(
        past_anchor.extend(score, Step::PreviousMeasure),
        past_anchor
    );
    let forward_again = past_anchor.extend(score, Step::NextMeasure);
    assert_eq!(
        forward_again.span(),
        Span {
            start: 960,
            end: 3840
        }
    );
}

#[test]
fn notes_starting_inside_grow_the_selection_to_whole_notes() {
    let (doc, staves, quarters, whole) = piano();
    let score = doc.score();
    // One quarter in the upper staff, extended down to the lower staff with its whole note.
    let both = Selection::for_event(score, quarters[0])
        .unwrap()
        .extend(score, Step::StaffDown);
    let resolved = both.resolve(score).unwrap();
    assert_eq!(resolved.staves, staves.to_vec());
    assert_eq!(
        resolved.span,
        Span {
            start: 0,
            end: 3840
        }
    );
    // Growing to the whole note brings in the quarters that start in the grown part.
    let mut events = resolved.events.clone();
    events.sort_by_key(|id| id.0);
    let mut expected = [quarters.clone(), vec![whole]].concat();
    expected.sort_by_key(|id| id.0);
    assert_eq!(events, expected);
    assert_eq!(
        resolved.measures,
        vec![MeasureSpan {
            measure: 0,
            start: 0,
            end: 3840,
            to_end: true,
        }]
    );

    assert_eq!(both.extend(score, Step::StaffDown), both, "no staff below");
    let up = both.extend(score, Step::StaffUp);
    assert_eq!(up.resolve(score).unwrap().staves, vec![staves[0]]);
}

#[test]
fn a_voice_filter_limits_the_selection() {
    let (mut doc, staves, quarters, _) = piano();
    let measure_voices = |doc: &Document| {
        doc.score().staff(staves[0]).unwrap().measures[0]
            .voices
            .len()
    };
    let cursor = Cursor {
        staff: staves[0],
        measure: 0,
        voice: 0,
        tick: 0,
    };
    edit::add_voice(&mut doc, staves[0], 0).unwrap();
    assert_eq!(measure_voices(&doc), 2);
    let second = enter_note(
        &mut doc,
        Cursor { voice: 1, ..cursor },
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
fn clicks_extend_to_events_and_select_all_covers_the_score() {
    let (doc, staves, quarters, whole) = piano();
    let score = doc.score();
    let clicked = Selection::for_event(score, quarters[2])
        .unwrap()
        .extend_to_event(score, whole)
        .unwrap();
    let resolved = clicked.resolve(score).unwrap();
    assert_eq!(resolved.staves, staves.to_vec());
    assert_eq!(
        resolved.span,
        Span {
            start: 0,
            end: 3840
        }
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
    assert_eq!(all.measures.len(), 2);

    let box_drawn = Selection::range(
        staves[1],
        staves[1],
        Span {
            start: 960,
            end: 1920,
        },
    );
    let resolved = box_drawn.resolve(score).unwrap();
    assert!(
        resolved.events.is_empty(),
        "the whole note starts before the box"
    );
}
