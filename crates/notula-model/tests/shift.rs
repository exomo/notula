use notula_model::edit::{self, Cursor, enter_note, enter_rest};
use notula_model::selection::{Selection, Step};
use notula_model::shift::shift_selection;
use notula_model::{
    Clef, Command, Document, Duration, Dynamic, EventKind, NoteValue, Pitch, StaffId,
    Step as PitchStep,
};

const QUARTER: u32 = 960;

fn quarter() -> Duration {
    Duration::new(NoteValue::Quarter, 0)
}

/// Staves of `count` measures; each staff's first measure is filled from `notes`, where
/// `None` is a quarter rest and `Some((step, value))` a note.
fn score(
    staves: &[&[Option<(PitchStep, NoteValue)>]],
    measures: usize,
) -> (Document, Vec<StaffId>) {
    let mut doc = Document::new();
    let clefs = vec![Clef::Treble; staves.len()];
    let part = doc.new_part("Strings", &clefs);
    let ids: Vec<_> = part.staves.iter().map(|staff| staff.id).collect();
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    for _ in 1..measures {
        edit::append_measure(&mut doc).unwrap();
    }
    for (staff, notes) in ids.iter().zip(staves) {
        let mut cursor = Cursor {
            staff: *staff,
            measure: 0,
            voice: 0,
            tick: 0,
        };
        for note in *notes {
            cursor = match note {
                Some((step, value)) => {
                    enter_note(
                        &mut doc,
                        cursor,
                        Pitch::new(*step, 0, 4),
                        Duration::new(*value, 0),
                    )
                    .unwrap()
                    .next
                }
                None => enter_rest(&mut doc, cursor, quarter()).unwrap().next,
            };
        }
    }
    (doc, ids)
}

/// Each event of a voice as (measure, tick, pitch step or None for a rest).
fn contents(doc: &Document, staff: StaffId) -> Vec<(usize, u32, Option<PitchStep>)> {
    let mut out = Vec::new();
    for (measure, content) in doc
        .score()
        .staff(staff)
        .unwrap()
        .measures
        .iter()
        .enumerate()
    {
        let mut tick = 0;
        for event in &content.voices[0].events {
            let step = match &event.kind {
                EventKind::Chord { pitches, .. } => Some(pitches[0].step),
                EventKind::Rest => None,
            };
            out.push((measure, tick, step));
            tick += event.duration.ticks();
        }
    }
    out
}

fn notes(doc: &Document, staff: StaffId) -> Vec<(usize, u32, PitchStep)> {
    contents(doc, staff)
        .into_iter()
        .filter_map(|(measure, tick, step)| Some((measure, tick, step?)))
        .collect()
}

/// Selects the note starting at `tick` of the first measure in one staff.
fn select(doc: &Document, staff: StaffId, tick: u32) -> Selection {
    Selection::caret(doc.score(), staff, 0, 0, tick)
        .unwrap()
        .extend(doc.score(), Step::Next)
}

#[test]
fn a_note_moves_through_rests_and_the_selection_follows() {
    let (mut doc, staves) = score(
        &[&[
            Some((PitchStep::C, NoteValue::Quarter)),
            None,
            Some((PitchStep::E, NoteValue::Quarter)),
            None,
        ]],
        2,
    );
    let staff = staves[0];
    let selection = select(&doc, staff, 0);

    let moved = shift_selection(&mut doc, &selection, true, QUARTER).unwrap();
    assert_eq!(
        notes(&doc, staff),
        vec![(0, 960, PitchStep::C), (0, 1920, PitchStep::E)]
    );
    assert_eq!(
        moved.span(),
        notula_model::selection::Span {
            start: 960,
            end: 1920
        }
    );
    assert_eq!(moved.resolve(doc.score()).unwrap().events.len(), 1);

    // E is directly after C now: nothing moves.
    let before = doc.score().clone();
    let error = shift_selection(&mut doc, &moved, true, QUARTER).unwrap_err();
    assert!(
        error.to_string().contains("another note is in the way"),
        "{error}"
    );
    assert_eq!(doc.score(), &before);

    // Back to the start is fine, then further back is not.
    let back = shift_selection(&mut doc, &moved, false, QUARTER).unwrap();
    assert_eq!(notes(&doc, staff)[0], (0, 0, PitchStep::C));
    assert!(shift_selection(&mut doc, &back, false, QUARTER).is_err());

    doc.score().validate().unwrap();
    // One undo step per move.
    doc.undo().unwrap();
    assert_eq!(notes(&doc, staff)[0], (0, 960, PitchStep::C));
}

#[test]
fn notes_move_into_the_next_measure_but_not_across_a_barline() {
    let (mut doc, staves) = score(
        &[&[
            Some((PitchStep::C, NoteValue::Quarter)),
            None,
            None,
            Some((PitchStep::G, NoteValue::Quarter)),
        ]],
        2,
    );
    let staff = staves[0];
    let last = select(&doc, staff, 2880);
    shift_selection(&mut doc, &last, true, QUARTER).unwrap();
    assert_eq!(notes(&doc, staff)[1], (1, 0, PitchStep::G));
    // The first measure is filled up with rests where G was.
    assert_eq!(
        contents(&doc, staff)
            .iter()
            .filter(|item| item.0 == 0)
            .count(),
        4
    );

    // A half note on beat 3 cannot move by a quarter: it would straddle the barline.
    let (mut doc, staves) = score(
        &[&[
            Some((PitchStep::C, NoteValue::Quarter)),
            None,
            Some((PitchStep::E, NoteValue::Half)),
        ]],
        2,
    );
    let half = select(&doc, staves[0], 1920);
    let error = shift_selection(&mut doc, &half, true, QUARTER).unwrap_err();
    assert!(error.to_string().contains("barline"), "{error}");
    // By a half it fits into the next measure.
    shift_selection(&mut doc, &half, true, 2 * QUARTER).unwrap();
    assert_eq!(notes(&doc, staves[0])[1], (1, 0, PitchStep::E));
    doc.score().validate().unwrap();
}

#[test]
fn marked_rests_block_and_ties_to_moved_notes_are_removed() {
    let (mut doc, staves) = score(
        &[&[
            Some((PitchStep::C, NoteValue::Quarter)),
            Some((PitchStep::C, NoteValue::Quarter)),
            None,
            None,
        ]],
        1,
    );
    let staff = staves[0];
    let ids: Vec<_> = doc.score().staff(staff).unwrap().measures[0].voices[0]
        .events
        .iter()
        .map(|event| event.id)
        .collect();
    edit::toggle_tie(&mut doc, ids[0]).unwrap();
    edit::set_dynamic(&mut doc, ids[3], Some(Dynamic::P)).unwrap();

    let second = select(&doc, staff, 960);
    shift_selection(&mut doc, &second, true, QUARTER).unwrap();
    let first = &doc.score().staff(staff).unwrap().measures[0].voices[0].events[0];
    assert!(
        matches!(first.kind, EventKind::Chord { tied: false, .. }),
        "the tie into the moved note is gone"
    );

    let moved = select(&doc, staff, 1920);
    let error = shift_selection(&mut doc, &moved, true, QUARTER).unwrap_err();
    assert!(error.to_string().contains("rest with marks"), "{error}");
}

#[test]
fn several_staves_move_together_or_not_at_all() {
    let (mut doc, staves) = score(
        &[
            &[Some((PitchStep::C, NoteValue::Quarter)), None, None, None],
            &[
                Some((PitchStep::E, NoteValue::Quarter)),
                None,
                Some((PitchStep::G, NoteValue::Quarter)),
                None,
            ],
        ],
        1,
    );
    let both = select(&doc, staves[0], 0).extend(doc.score(), Step::StaffDown);
    let moved = shift_selection(&mut doc, &both, true, QUARTER).unwrap();
    assert_eq!(notes(&doc, staves[0]), vec![(0, 960, PitchStep::C)]);
    assert_eq!(notes(&doc, staves[1])[0], (0, 960, PitchStep::E));

    // The lower staff's E would run into its G: neither staff moves.
    let before = doc.score().clone();
    assert!(shift_selection(&mut doc, &moved, true, QUARTER).is_err());
    assert_eq!(doc.score(), &before);
    doc.score().validate().unwrap();
}
