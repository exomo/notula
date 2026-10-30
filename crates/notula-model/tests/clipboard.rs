use notula_model::clipboard::{copy, cut, paste};
use notula_model::edit::{self, Cursor, enter_note, enter_rest};
use notula_model::selection::{Selection, Span, Step};
use notula_model::{
    Clef, Command, Document, Duration, EventKind, NoteValue, Pitch, SlurPlacement, StaffId,
    Step as PitchStep,
};

const QUARTER: u64 = 960;
const MEASURE: u64 = 4 * QUARTER;

fn value(digit: &str) -> NoteValue {
    match digit {
        "1" => NoteValue::Whole,
        "2" => NoteValue::Half,
        "4" => NoteValue::Quarter,
        "8" => NoteValue::Eighth,
        other => panic!("unknown note value {other}"),
    }
}

fn step(letter: char) -> PitchStep {
    match letter {
        'C' => PitchStep::C,
        'D' => PitchStep::D,
        'E' => PitchStep::E,
        'F' => PitchStep::F,
        'G' => PitchStep::G,
        'A' => PitchStep::A,
        'B' => PitchStep::B,
        other => panic!("unknown step {other}"),
    }
}

/// Enters `notes` from the start of `measure`, such as "C4 r4 E2" (r for a rest).
fn enter(doc: &mut Document, staff: StaffId, voice: usize, measure: usize, notes: &str) {
    let mut cursor = Cursor {
        staff,
        measure,
        voice,
        tick: 0,
    };
    for token in notes.split_whitespace() {
        let (letter, digit) = token.split_at(1);
        let duration = Duration::new(value(digit), 0);
        cursor = if letter == "r" {
            enter_rest(doc, cursor, duration).unwrap().next
        } else {
            let pitch = Pitch::new(step(letter.chars().next().unwrap()), 0, 4);
            enter_note(doc, cursor, pitch, duration).unwrap().next
        };
    }
}

/// One staff per entry of `staves`, each filled from its first measure, in `measures` measures.
fn score(staves: &[&str], measures: usize) -> (Document, Vec<StaffId>) {
    let mut doc = Document::new();
    let clefs = vec![Clef::Treble; staves.len()];
    let part = doc.new_part("Strings", &clefs);
    let ids: Vec<_> = part.staves.iter().map(|staff| staff.id).collect();
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    for _ in 1..measures {
        edit::append_measure(&mut doc).unwrap();
    }
    for (staff, notes) in ids.iter().zip(staves) {
        enter(&mut doc, *staff, 0, 0, notes);
    }
    (doc, ids)
}

/// A voice as text: events like "C4" or "r2", "~" after a tied note, measures separated by
/// " | ", and "-" for an empty measure.
fn render(doc: &Document, staff: StaffId, voice: usize) -> String {
    let staff = doc.score().staff(staff).unwrap();
    let measures: Vec<String> = staff
        .measures
        .iter()
        .map(|content| {
            let Some(voice) = content.voices.get(voice) else {
                return "x".to_string();
            };
            if voice.events.is_empty() {
                return "-".to_string();
            }
            let events: Vec<String> = voice
                .events
                .iter()
                .map(|event| {
                    let digit = match event.duration.value {
                        NoteValue::Whole => "1",
                        NoteValue::Half => "2",
                        NoteValue::Quarter => "4",
                        NoteValue::Eighth => "8",
                        _ => "?",
                    };
                    let dots = ".".repeat(usize::from(event.duration.dots));
                    match &event.kind {
                        EventKind::Rest => format!("r{digit}{dots}"),
                        EventKind::Chord { pitches, tied } => format!(
                            "{:?}{digit}{dots}{}",
                            pitches[0].step,
                            if *tied { "~" } else { "" }
                        ),
                    }
                })
                .collect();
            events.join(" ")
        })
        .collect();
    measures.join(" | ")
}

fn caret(doc: &Document, staff: StaffId, measure: usize, tick: u32) -> Selection {
    Selection::caret(doc.score(), staff, 0, measure, tick).unwrap()
}

fn paste_at(
    doc: &mut Document,
    staff: StaffId,
    measure: usize,
    tick: u32,
    clip: &notula_model::clipboard::Clip,
) -> Result<Selection, notula_model::ModelError> {
    let at = caret(doc, staff, measure, tick);
    paste(doc, &at, clip)
}

fn measure(doc: &Document, staff: StaffId, measure: usize) -> Selection {
    caret(doc, staff, measure, 0).extend(doc.score(), Step::NextMeasure)
}

fn event_ids(doc: &Document, staff: StaffId, measure: usize) -> Vec<notula_model::EventId> {
    doc.score().staff(staff).unwrap().measures[measure].voices[0]
        .events
        .iter()
        .map(|event| event.id)
        .collect()
}

#[test]
fn a_pasted_measure_overwrites_the_target_and_is_selected() {
    let (mut doc, staves) = score(&["C4 D4 E4 F4"], 3);
    let staff = staves[0];
    let clip = copy(doc.score(), &measure(&doc, staff, 0)).unwrap();
    assert_eq!(clip.length, MEASURE);

    let pasted = paste_at(&mut doc, staff, 1, 0, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 D4 E4 F4 | C4 D4 E4 F4 | -");
    assert_eq!(
        pasted.span(),
        Span {
            start: MEASURE,
            end: 2 * MEASURE
        }
    );
    let resolved = pasted.resolve(doc.score()).unwrap();
    assert_eq!(resolved.events, event_ids(&doc, staff, 1));
    // Pasted notes are new events.
    assert!(
        event_ids(&doc, staff, 1)
            .iter()
            .all(|id| !event_ids(&doc, staff, 0).contains(id))
    );
    doc.score().validate().unwrap();

    // Pasting over a range selection starts at its beginning; the clip decides the length.
    let short = caret(&doc, staff, 1, 0).extend(doc.score(), Step::Next);
    let clip = copy(
        doc.score(),
        &caret(&doc, staff, 0, 960).extend(doc.score(), Step::Next),
    )
    .unwrap();
    paste(&mut doc, &short, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 D4 E4 F4 | D4 D4 E4 F4 | -");

    // One undo step per paste.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 D4 E4 F4 | - | -");
}

#[test]
fn notes_crossing_a_barline_are_split_into_ties() {
    let (mut doc, staves) = score(&["C2 D2"], 3);
    let staff = staves[0];
    let clip = copy(doc.score(), &measure(&doc, staff, 0)).unwrap();
    paste_at(&mut doc, staff, 1, 960, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C2 D2 | r4 C2 D4~ | D4 r2 r4");
    doc.score().validate().unwrap();
}

#[test]
fn pasting_past_the_end_adds_measures() {
    let (mut doc, staves) = score(&["C4 D4 E4 F4"], 2);
    let staff = staves[0];
    let clip = copy(doc.score(), &measure(&doc, staff, 0)).unwrap();
    paste_at(&mut doc, staff, 1, 1920, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 D4 E4 F4 | r2 C4 D4 | E4 F4 r2");
    doc.score().validate().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.score().measures.len(), 2);
}

#[test]
fn staves_must_match_unless_one_staff_was_copied() {
    let (mut doc, staves) = score(&["C1", "E1", "G1"], 2);
    let two = measure(&doc, staves[0], 0).extend(doc.score(), Step::StaffDown);
    let clip = copy(doc.score(), &two).unwrap();
    assert_eq!(clip.staves.len(), 2);

    let before = doc.score().clone();
    let error = paste_at(&mut doc, staves[0], 1, 0, &clip).unwrap_err();
    assert!(error.to_string().contains("as many staves"), "{error}");
    assert_eq!(doc.score(), &before);

    let lower = caret(&doc, staves[1], 1, 0).extend(doc.score(), Step::StaffDown);
    paste(&mut doc, &lower, &clip).unwrap();
    assert_eq!(render(&doc, staves[1], 0), "E1 | C1");
    assert_eq!(render(&doc, staves[2], 0), "G1 | E1");

    // A single staff fills every staff the cursor spans.
    let clip = copy(doc.score(), &measure(&doc, staves[2], 0)).unwrap();
    let all = caret(&doc, staves[0], 1, 0)
        .extend(doc.score(), Step::StaffDown)
        .extend(doc.score(), Step::StaffDown);
    paste(&mut doc, &all, &clip).unwrap();
    for staff in &staves {
        assert!(render(&doc, *staff, 0).ends_with("| G1"));
    }
    doc.score().validate().unwrap();
}

#[test]
fn overwriting_shortens_notes_and_removes_cut_marks() {
    let (mut doc, staves) = score(&["G4 A4 r2", "C2 D2 E2 F2"], 2);
    let target = staves[1];
    let first = event_ids(&doc, target, 0);
    let second = event_ids(&doc, target, 1);
    edit::toggle_tie(&mut doc, first[0]).unwrap();
    edit::toggle_slur(&mut doc, first[1], second[0], SlurPlacement::Auto).unwrap();

    let source = caret(&doc, staves[0], 0, 0)
        .extend(doc.score(), Step::Next)
        .extend(doc.score(), Step::Next);
    let clip = copy(doc.score(), &source).unwrap();
    paste_at(&mut doc, target, 0, 960, &clip).unwrap();

    // C keeps its first quarter and loses its tie; D's tail becomes rest.
    assert_eq!(render(&doc, target, 0), "C4 G4 A4 r4 | E2 F2 | -");
    let e = &doc.score().staff(target).unwrap().measures[1].voices[0].events[0];
    assert!(
        e.markings.is_empty(),
        "the slur from the overwritten D is gone"
    );
    doc.score().validate().unwrap();
}

#[test]
fn copies_keep_inner_marks_and_cut_leaves_rests() {
    let (mut doc, staves) = score(&["C4 D4 D4 F4"], 2);
    let staff = staves[0];
    let ids = event_ids(&doc, staff, 0);
    edit::toggle_slur(&mut doc, ids[0], ids[2], SlurPlacement::Auto).unwrap();
    edit::toggle_tie(&mut doc, ids[1]).unwrap();
    edit::toggle_tie(&mut doc, ids[2]).unwrap();

    let middle = caret(&doc, staff, 0, 960)
        .extend(doc.score(), Step::Next)
        .extend(doc.score(), Step::Next);
    let clip = copy(doc.score(), &middle).unwrap();
    let copied = &clip.staves[0].voices[0].events;
    assert_eq!(copied.len(), 2);
    // The tie inside the copy stays, the one leaving it and the slur from C do not.
    assert!(matches!(
        copied[0].1.kind,
        EventKind::Chord { tied: true, .. }
    ));
    assert!(matches!(
        copied[1].1.kind,
        EventKind::Chord { tied: false, .. }
    ));
    assert!(copied[1].1.markings.is_empty());

    let clip = cut(&mut doc, &middle).unwrap();
    assert_eq!(clip.staves[0].voices[0].events.len(), 2);
    assert_eq!(render(&doc, staff, 0), "C4 r4 r4 F4 | -");
    paste_at(&mut doc, staff, 1, 0, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 r4 r4 F4 | D4~ D4 r2");
    doc.score().validate().unwrap();
    // Cut is one undo step.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(render(&doc, staff, 0), "C4 D4~ D4~ F4 | -");
}

#[test]
fn voices_go_to_the_cursor_voice_or_their_own() {
    let (mut doc, staves) = score(&["C1", "G1"], 3);
    let staff = staves[0];
    let voice = edit::add_voice(&mut doc, staff, 0).unwrap();
    enter(&mut doc, staff, voice, 0, "E4 F4 E4 F4");

    // A clip limited to voice 2 goes into the cursor's voice.
    let only = Selection {
        voice: Some(voice),
        ..measure(&doc, staff, 0)
    };
    let clip = copy(doc.score(), &only).unwrap();
    assert_eq!(clip.staves[0].voices.len(), 1);
    paste_at(&mut doc, staff, 1, 0, &clip).unwrap();
    assert_eq!(render(&doc, staff, 0), "C1 | E4 F4 E4 F4 | -");

    // All voices keep their numbers; the other staff gets voice 2 only where pasted.
    let clip = copy(doc.score(), &measure(&doc, staff, 0)).unwrap();
    paste_at(&mut doc, staves[1], 1, 0, &clip).unwrap();
    assert_eq!(render(&doc, staves[1], 0), "G1 | C1 | -");
    assert_eq!(render(&doc, staves[1], 1), "x | E4 F4 E4 F4 | x");
    doc.score().validate().unwrap();
    doc.undo().unwrap();
    assert_eq!(render(&doc, staves[1], 1), "x | x | x");
}

#[test]
fn voices_of_single_measures_are_added_only_where_pasted() {
    let (mut doc, staves) = score(&["C1", "G1"], 5);
    // Each staff has a second voice in one measure only, as entered voices usually are.
    for (staff, measure, notes) in [(staves[0], 1, "E4 F4 E4 F4"), (staves[1], 3, "A4 A4 A4 A4")] {
        let voice = edit::add_voice(&mut doc, staff, measure).unwrap();
        enter(&mut doc, staff, voice, measure, notes);
        edit::end_voice_at(&mut doc, staff, measure, voice, MEASURE as u32).unwrap();
    }
    assert_eq!(render(&doc, staves[0], 1), "x | E4 F4 E4 F4 | x | x | x");

    let both = measure(&doc, staves[0], 1).extend(doc.score(), Step::StaffDown);
    let clip = copy(doc.score(), &both).unwrap();
    let target = caret(&doc, staves[0], 0, 0).extend(doc.score(), Step::StaffDown);
    paste(&mut doc, &target, &clip).unwrap();
    assert_eq!(
        render(&doc, staves[0], 1),
        "E4 F4 E4 F4 | E4 F4 E4 F4 | x | x | x"
    );
    assert_eq!(render(&doc, staves[1], 1), "x | x | x | A4 A4 A4 A4 | x");
    doc.score().validate().unwrap();

    doc.undo().unwrap();
    assert_eq!(render(&doc, staves[0], 1), "x | E4 F4 E4 F4 | x | x | x");
}
