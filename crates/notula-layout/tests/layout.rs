use notula_layout::{Item, LayoutOptions, RenderList, Shape, hit_test, layout};
use notula_model::{
    Articulation, Clef, Command, Document, Duration, Event, EventId, EventKind, KeySignature,
    NoteValue, Pitch, StaffId, Step, TimeSignature, TimeSignatureDisplay, VoiceLocation,
    edit::{self, Cursor},
};

const STAFF_LINE: f32 = 0.13;
const STAFF_HEIGHT: f32 = 4.0;
const SHARP: char = '\u{E262}';
const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const BLACK_HEAD: char = '\u{E0A4}';
const HALF_HEAD: char = '\u{E0A3}';

struct Builder {
    doc: Document,
    staves: Vec<StaffId>,
}

impl Builder {
    fn new(parts: &[(&str, &[Clef])]) -> Self {
        let mut doc = Document::new();
        let mut staves = Vec::new();
        for (index, (name, clefs)) in parts.iter().enumerate() {
            let part = doc.new_part(name, clefs);
            staves.extend(part.staves.iter().map(|s| s.id));
            doc.execute(Command::AddPart { index, part }).unwrap();
        }
        Self { doc, staves }
    }

    fn measures(&mut self, count: usize) {
        while self.doc.score().measures.len() < count {
            let index = self.doc.score().measures.len();
            let data = self.doc.score().blank_measure(index);
            self.doc
                .execute(Command::InsertMeasure { index, data })
                .unwrap();
        }
    }

    fn append(
        &mut self,
        staff: usize,
        measure: usize,
        kind: EventKind,
        value: NoteValue,
    ) -> EventId {
        let id = self.doc.new_event_id();
        let at = VoiceLocation {
            staff: self.staves[staff],
            measure,
            voice: 0,
        };
        let index = self.doc.score().voice(at).unwrap().events.len();
        let event = Event::new(id, Duration::new(value, 0), kind);
        self.doc
            .execute(Command::InsertEvent { at, index, event })
            .unwrap();
        id
    }

    fn note(&mut self, staff: usize, measure: usize, pitch: Pitch, value: NoteValue) -> EventId {
        let kind = EventKind::Chord {
            pitches: vec![pitch],
            tied: false,
        };
        self.append(staff, measure, kind, value)
    }

    fn layout(&self, width: f32) -> RenderList {
        layout(self.doc.score(), &LayoutOptions { width })
    }
}

fn c4() -> Pitch {
    Pitch::new(Step::C, 0, 4)
}

fn tie_polygons(list: &RenderList, event: EventId) -> usize {
    list.items
        .iter()
        .filter(|item| item.event == Some(event) && matches!(item.shape, Shape::Tie { .. }))
        .count()
}

fn glyph_x(list: &RenderList, event: EventId, glyph: char) -> f32 {
    list.items
        .iter()
        .find_map(|item| match (&item.shape, item.event) {
            (Shape::Glyph { x, glyph: g, .. }, Some(id)) if id == event && *g == glyph => Some(*x),
            _ => None,
        })
        .expect("glyph not found")
}

#[test]
fn fermata_is_above_simultaneous_voices_and_drawn_in_foreground() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let event = b.note(0, 0, c4(), NoteValue::Quarter);
    edit::toggle_articulation(&mut b.doc, event, Articulation::Fermata).unwrap();
    let staff = b.staves[0];
    let second_voice = edit::add_voice(&mut b.doc, staff, 0).unwrap();
    edit::enter_note(
        &mut b.doc,
        Cursor {
            staff,
            measure: 0,
            voice: second_voice,
            tick: 0,
        },
        Pitch::new(Step::C, 0, 6),
        Duration::new(NoteValue::Quarter, 0),
    )
    .unwrap();

    let list = b.layout(100.0);
    let fermata_index = list
        .items
        .iter()
        .position(|item| {
            matches!(
                item.shape,
                Shape::Glyph {
                    glyph: '\u{E4C0}',
                    ..
                }
            )
        })
        .unwrap();
    assert!(matches!(
        list.items[fermata_index - 1].shape,
        Shape::Knockout { .. }
    ));
    let (fermata_y, upper_note_y) = list.items.iter().enumerate().fold(
        (None, None),
        |(fermata_y, upper_note_y), (index, item)| match item.shape {
            Shape::Glyph {
                y,
                glyph: '\u{E4C0}',
                ..
            } if index == fermata_index => (Some(y), upper_note_y),
            Shape::Glyph {
                y,
                glyph: BLACK_HEAD,
                ..
            } => (
                fermata_y,
                Some(upper_note_y.map_or(y, |top: f32| top.min(y))),
            ),
            _ => (fermata_y, upper_note_y),
        },
    );

    assert!(fermata_index > 0);
    assert!(fermata_y.unwrap() < upper_note_y.unwrap());
    assert!(list.items[fermata_index + 1..].iter().all(|item| !matches!(
        item.shape,
        Shape::Glyph {
            glyph: BLACK_HEAD,
            ..
        }
    )));
}

#[test]
fn tied_chord_engravings_join_matching_pitches() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let source = b.append(
        0,
        0,
        EventKind::Chord {
            pitches: vec![c4(), Pitch::new(Step::E, 0, 4)],
            tied: true,
        },
        NoteValue::Quarter,
    );
    b.append(
        0,
        0,
        EventKind::Chord {
            pitches: vec![c4(), Pitch::new(Step::G, 0, 4)],
            tied: false,
        },
        NoteValue::Quarter,
    );

    let list = b.layout(100.0);
    assert_eq!(tie_polygons(&list, source), 1);
    let tie = list
        .items
        .iter()
        .find(|item| item.event == Some(source) && matches!(item.shape, Shape::Tie { .. }))
        .unwrap();
    assert_eq!(tie.staff, Some(b.staves[0]));
    assert_eq!(tie.voice, Some(0));
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item.shape, Shape::WhitePolygon { .. }))
    );
    assert!(
        list.items
            .iter()
            .all(|item| !matches!(item.shape, Shape::Knockout { .. }))
    );
}

#[test]
fn slurs_connect_range_endpoints_with_different_pitches() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let start = b.note(0, 0, c4(), NoteValue::Quarter);
    let end = b.note(0, 0, Pitch::new(Step::D, 0, 4), NoteValue::Quarter);
    edit::toggle_slur(&mut b.doc, start, end, notula_model::SlurPlacement::Auto).unwrap();

    let list = b.layout(100.0);
    let slur = list
        .items
        .iter()
        .find(|item| item.event == Some(start) && matches!(item.shape, Shape::Slur { .. }))
        .unwrap();
    assert_eq!(slur.staff, Some(b.staves[0]));
    assert_eq!(slur.voice, Some(0));
    let Shape::Slur { points } = &slur.shape else {
        unreachable!();
    };
    assert!((points[6][1] - points[19][1]).abs() > 0.17);
    let source_y = list
        .items
        .iter()
        .find_map(|item| match (&item.shape, item.event) {
            (
                Shape::Glyph {
                    y,
                    glyph: BLACK_HEAD,
                    ..
                },
                Some(id),
            ) if id == start => Some(*y),
            _ => None,
        })
        .unwrap();
    assert!(points[0][1] < source_y);
}

#[test]
fn slur_below_placement_anchors_under_the_noteheads() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let start = b.note(0, 0, c4(), NoteValue::Quarter);
    let end = b.note(0, 0, Pitch::new(Step::D, 0, 4), NoteValue::Quarter);
    edit::toggle_slur(&mut b.doc, start, end, notula_model::SlurPlacement::Below).unwrap();

    let list = b.layout(100.0);
    let slur = list
        .items
        .iter()
        .find(|item| item.event == Some(start) && matches!(item.shape, Shape::Slur { .. }))
        .unwrap();
    let Shape::Slur { points } = &slur.shape else {
        unreachable!();
    };
    let source_y = list
        .items
        .iter()
        .find_map(|item| match (&item.shape, item.event) {
            (
                Shape::Glyph {
                    y,
                    glyph: BLACK_HEAD,
                    ..
                },
                Some(id),
            ) if id == start => Some(*y),
            _ => None,
        })
        .unwrap();
    assert!(points[0][1] > source_y);
}

#[test]
fn phrase_slur_can_overlap_an_ordinary_slur() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let start = b.note(0, 0, c4(), NoteValue::Quarter);
    let middle = b.note(0, 0, Pitch::new(Step::D, 0, 4), NoteValue::Quarter);
    let end = b.note(0, 0, Pitch::new(Step::E, 0, 4), NoteValue::Quarter);
    edit::toggle_slur(
        &mut b.doc,
        start,
        middle,
        notula_model::SlurPlacement::Above,
    )
    .unwrap();
    edit::toggle_phrase_slur(&mut b.doc, start, end, notula_model::SlurPlacement::Below).unwrap();

    let list = b.layout(100.0);
    assert!(
        list.items
            .iter()
            .any(|item| { item.event == Some(start) && matches!(item.shape, Shape::Slur { .. }) })
    );
    assert!(list.items.iter().any(|item| {
        item.event == Some(start) && matches!(item.shape, Shape::PhraseSlur { .. })
    }));
    let phrase = list
        .items
        .iter()
        .find(|item| item.event == Some(start) && matches!(item.shape, Shape::PhraseSlur { .. }))
        .unwrap();
    let Shape::PhraseSlur { points } = &phrase.shape else {
        unreachable!();
    };
    let staff_top = list
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0])
        .unwrap()
        .y_top;
    assert!(
        points
            .iter()
            .all(|point| point[1] > staff_top + STAFF_HEIGHT)
    );
}

#[test]
fn phrase_slur_above_stays_outside_staff_lines() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let start = b.note(0, 0, c4(), NoteValue::Quarter);
    let end = b.note(0, 0, Pitch::new(Step::D, 0, 4), NoteValue::Quarter);
    edit::toggle_phrase_slur(&mut b.doc, start, end, notula_model::SlurPlacement::Above).unwrap();

    let list = b.layout(100.0);
    let phrase = list
        .items
        .iter()
        .find(|item| item.event == Some(start) && matches!(item.shape, Shape::PhraseSlur { .. }))
        .unwrap();
    let Shape::PhraseSlur { points } = &phrase.shape else {
        unreachable!();
    };
    let staff_top = list
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0])
        .unwrap()
        .y_top;
    assert!(points.iter().all(|point| point[1] < staff_top));
}

#[test]
fn tied_chords_continue_across_a_barline_on_the_same_system() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    let source = b.note(0, 0, c4(), NoteValue::Whole);
    edit::toggle_tie(&mut b.doc, source).unwrap();
    b.note(0, 1, c4(), NoteValue::Whole);

    let list = b.layout(100.0);
    assert_eq!(
        list.areas
            .iter()
            .filter(|area| area.staff == b.staves[0])
            .count(),
        2
    );
    assert_eq!(tie_polygons(&list, source), 1);
}

#[test]
fn tied_chords_continue_across_system_breaks() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    let source = b.note(0, 0, c4(), NoteValue::Whole);
    edit::toggle_tie(&mut b.doc, source).unwrap();
    b.note(0, 1, c4(), NoteValue::Whole);

    let list = b.layout(12.0);
    let systems: Vec<_> = list
        .areas
        .iter()
        .filter(|area| area.staff == b.staves[0])
        .collect();
    assert_eq!(systems.len(), 2);
    assert_ne!(systems[0].y_top, systems[1].y_top);
    let ties: Vec<_> = list
        .items
        .iter()
        .filter(|item| item.event == Some(source) && matches!(item.shape, Shape::Tie { .. }))
        .map(|item| item.shape.clone())
        .collect();
    assert_eq!(ties.len(), 2, "{ties:?}");
}

fn staff_lines(list: &RenderList) -> Vec<&Item> {
    list.items
        .iter()
        .filter(
            |item| matches!(item.shape, Shape::Line { thickness, .. } if thickness == STAFF_LINE),
        )
        .collect()
}

#[test]
fn hit_testing_maps_points_to_staff_positions() {
    let mut b = Builder::new(&[("", &[Clef::Treble, Clef::Bass])]);
    b.note(0, 0, c4(), NoteValue::Quarter);
    b.note(0, 0, c4(), NoteValue::Quarter);
    let list = b.layout(100.0);
    let score = b.doc.score();

    let treble = list
        .areas
        .iter()
        .find(|a| a.staff == b.staves[0] && a.measure == 0)
        .unwrap();
    let bass = list
        .areas
        .iter()
        .find(|a| a.staff == b.staves[1] && a.measure == 0)
        .unwrap();
    assert_eq!(treble.columns.len(), 2);
    let (second_tick, second_x) = treble.columns[1];

    // The bottom line of the treble staff is E4.
    let hit = hit_test(score, &list, second_x + 0.5, treble.y_top + 4.0).unwrap();
    assert_eq!(hit.staff, b.staves[0]);
    assert_eq!(hit.tick, Some(second_tick));
    assert_eq!(hit.pitch, Pitch::new(Step::E, 0, 4));

    // Past the last note means the end of the measure.
    let hit = hit_test(score, &list, second_x + 4.0, treble.y_top + 2.0).unwrap();
    assert_eq!(hit.tick, None);

    // Between the staves the nearer one wins.
    let hit = hit_test(score, &list, second_x, bass.y_top - 1.0).unwrap();
    assert_eq!(hit.staff, b.staves[1]);

    assert!(hit_test(score, &list, second_x, treble.y_top - 20.0).is_none());
    assert!(hit_test(score, &list, -5.0, treble.y_top + 2.0).is_none());
}

#[test]
fn hit_testing_at_a_measure_start_uses_that_measure() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    let list = b.layout(100.0);
    let next = list
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0] && area.measure == 1)
        .unwrap();

    let hit = hit_test(b.doc.score(), &list, next.x_start, next.y_top + 2.0).unwrap();
    assert_eq!(hit.measure, 1);
    assert_eq!(hit.tick, None);
}

#[test]
fn rendered_events_identify_their_voice_and_measure_voice_count() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    let staff = b.staves[0];
    let voice = notula_model::edit::add_voice(&mut b.doc, staff, 0).unwrap();
    let event = Event::new(
        b.doc.new_event_id(),
        Duration::new(NoteValue::Quarter, 0),
        EventKind::Chord {
            pitches: vec![c4()],
            tied: false,
        },
    );
    let event_id = event.id;
    b.doc
        .execute(Command::InsertEvent {
            at: VoiceLocation {
                staff,
                measure: 0,
                voice,
            },
            index: 0,
            event,
        })
        .unwrap();

    let list = b.layout(100.0);
    let rendered = list
        .items
        .iter()
        .find(|item| item.event == Some(event_id))
        .unwrap();
    assert_eq!(rendered.staff, Some(staff));
    assert_eq!(rendered.voice, Some(1));
    assert_eq!(list.areas[0].voice_count, 2);
}

#[test]
fn hit_testing_near_the_barline_targets_the_content_end() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.note(0, 0, c4(), NoteValue::Quarter);
    let list = b.layout(100.0);
    let area = list
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0] && area.measure == 0)
        .unwrap();

    let hit = hit_test(b.doc.score(), &list, area.x_end - 1.0, area.y_top + 2.0).unwrap();
    assert_eq!(hit.tick, None);
}

#[test]
fn measure_areas_identify_the_next_bar_only_within_the_same_system() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    let wide = b.layout(100.0);
    let first = wide
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0] && area.measure == 0)
        .unwrap();
    let last = wide
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0] && area.measure == 1)
        .unwrap();
    assert_eq!(first.next_measure, Some(1));
    assert_eq!(last.next_measure, None);

    let narrow = b.layout(20.0);
    let first_system_end = narrow
        .areas
        .iter()
        .find(|area| area.staff == b.staves[0] && area.measure == 0)
        .unwrap();
    assert_eq!(first_system_end.next_measure, None);
}

#[test]
fn pickup_measures_are_narrower_and_rest_for_their_own_length() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    for _ in 0..4 {
        b.note(0, 1, c4(), NoteValue::Quarter);
    }
    b.doc
        .execute(Command::SetMeasureLength {
            measure: 0,
            length: Some(960),
        })
        .unwrap();
    let list = b.layout(120.0);

    let width = |measure: usize| {
        let area = list
            .areas
            .iter()
            .find(|a| a.staff == b.staves[0] && a.measure == measure)
            .unwrap();
        area.x_end - area.x_start
    };
    assert!(width(0) < width(1));

    let rests = |glyph: char| {
        list.items
            .iter()
            .filter(|i| {
                i.event.is_none() && matches!(i.shape, Shape::Glyph { glyph: g, .. } if g == glyph)
            })
            .count()
    };
    assert_eq!(rests('\u{E4E5}'), 1, "quarter rest for the empty pickup");
    assert_eq!(rests('\u{E4E3}'), 0, "no whole-measure rest");
}

#[test]
fn a_time_signature_change_is_drawn_and_shortens_the_measures() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(4);
    for measure in 2..4 {
        b.doc
            .execute(Command::SetTimeSignature {
                measure,
                time_signature: TimeSignature {
                    numerator: 3,
                    denominator: 4,
                },
            })
            .unwrap();
    }
    let list = b.layout(200.0);

    let width = |measure: usize| {
        let area = list
            .areas
            .iter()
            .find(|a| a.staff == b.staves[0] && a.measure == measure)
            .unwrap();
        area.x_end - area.x_start
    };
    assert!(width(2) < width(1));
    assert_eq!(width(2), width(3));

    let digit = |digit: char| {
        list.items
            .iter()
            .filter(|i| {
                i.event.is_none() && matches!(i.shape, Shape::Glyph { glyph, .. } if glyph == digit)
            })
            .count()
    };
    // 4/4 once at the start and 3/4 once at the change, not repeated for every measure.
    assert_eq!(digit('\u{E084}'), 3);
    assert_eq!(digit('\u{E083}'), 1);
}

#[test]
fn score_without_staves_renders_nothing() {
    let list = Builder::new(&[]).layout(100.0);
    assert!(list.items.is_empty());
}

#[test]
fn simultaneous_events_align_across_staves() {
    let mut b = Builder::new(&[("A", &[Clef::Treble, Clef::Bass])]);
    let quarters: Vec<_> = (0..4)
        .map(|_| b.note(0, 0, c4(), NoteValue::Quarter))
        .collect();
    let halves: Vec<_> = (0..2)
        .map(|_| b.note(1, 0, c4(), NoteValue::Half))
        .collect();
    let list = b.layout(120.0);

    assert_eq!(
        glyph_x(&list, quarters[0], BLACK_HEAD),
        glyph_x(&list, halves[0], HALF_HEAD)
    );
    assert_eq!(
        glyph_x(&list, quarters[2], BLACK_HEAD),
        glyph_x(&list, halves[1], HALF_HEAD)
    );
    assert!(glyph_x(&list, quarters[1], BLACK_HEAD) > glyph_x(&list, quarters[0], BLACK_HEAD));
}

#[test]
fn longer_notes_get_more_room() {
    let mut b = Builder::new(&[("A", &[Clef::Treble])]);
    let half = b.note(0, 0, c4(), NoteValue::Half);
    let quarter = b.note(0, 0, c4(), NoteValue::Quarter);
    let next = b.note(0, 0, c4(), NoteValue::Quarter);
    let list = b.layout(120.0);

    let after_half = glyph_x(&list, quarter, BLACK_HEAD) - glyph_x(&list, half, HALF_HEAD);
    let after_quarter = glyph_x(&list, next, BLACK_HEAD) - glyph_x(&list, quarter, BLACK_HEAD);
    assert!(after_half > after_quarter);
}

#[test]
fn measures_wrap_into_justified_systems() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(12);
    for m in 0..12 {
        for _ in 0..4 {
            b.note(0, m, c4(), NoteValue::Quarter);
        }
    }
    let list = b.layout(80.0);

    let lines = staff_lines(&list);
    assert_eq!(lines.len() % 5, 0);
    let systems = lines.len() / 5;
    assert!(systems > 1, "expected several systems, got {systems}");

    let ends: Vec<f32> = lines
        .iter()
        .step_by(5)
        .map(|item| match item.shape {
            Shape::Line { x2, .. } => x2,
            _ => unreachable!(),
        })
        .collect();
    for end in &ends[..systems - 1] {
        assert!((end - 78.0).abs() < 0.01, "system not justified: {end}");
    }
    assert!(ends[systems - 1] <= 78.0 + 0.01);

    for item in &list.items {
        if let Shape::Glyph { x, .. } = item.shape {
            assert!((0.0..=80.0).contains(&x), "glyph outside the page: {x}");
        }
    }
}

#[test]
fn only_the_end_of_the_score_has_a_final_barline() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(6);
    let list = b.layout(50.0);
    let thick = list
        .items
        .iter()
        .filter(|item| matches!(item.shape, Shape::Line { thickness, .. } if thickness == 0.5))
        .count();
    assert_eq!(thick, 1);
}

#[test]
fn key_signature_is_repeated_at_each_system_start() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(8);
    b.doc
        .execute(Command::SetKeySignature {
            measure: 0,
            key_signature: KeySignature { fifths: 2 },
        })
        .unwrap();
    for m in 1..8 {
        b.doc
            .execute(Command::SetKeySignature {
                measure: m,
                key_signature: KeySignature { fifths: 2 },
            })
            .unwrap();
    }
    let list = b.layout(45.0);

    let systems = staff_lines(&list).len() / 5;
    assert!(systems > 1);
    let sharps = list
        .items
        .iter()
        .filter(|item| {
            item.event.is_none()
                && matches!(item.shape, Shape::Glyph { glyph, .. } if glyph == SHARP)
        })
        .count();
    assert_eq!(sharps, 2 * systems);
}

#[test]
fn key_changes_cancel_removed_accidentals_before_drawing_the_new_signature() {
    for (new_fifths, expected_naturals, expected_flats) in [(-3, 1, 3), (0, 1, 0)] {
        let mut b = Builder::new(&[("", &[Clef::Treble])]);
        b.measures(2);
        for (measure, fifths) in [(0, 1), (1, new_fifths)] {
            b.doc
                .execute(Command::SetKeySignature {
                    measure,
                    key_signature: KeySignature { fifths },
                })
                .unwrap();
        }
        let list = b.layout(120.0);
        let first = list.areas.iter().find(|area| area.measure == 0).unwrap();
        let changed = list.areas.iter().find(|area| area.measure == 1).unwrap();
        let glyphs: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item.shape {
                Shape::Glyph { x, glyph, .. } if x >= first.x_end && x < changed.x_start => {
                    Some(glyph)
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            glyphs.iter().filter(|&&glyph| glyph == NATURAL).count(),
            expected_naturals
        );
        assert_eq!(
            glyphs.iter().filter(|&&glyph| glyph == FLAT).count(),
            expected_flats
        );
        assert_eq!(glyphs.iter().filter(|&&glyph| glyph == SHARP).count(), 0);
    }
}

#[test]
fn common_and_cut_time_render_their_symbols_instead_of_numeric_digits() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(2);
    b.doc
        .execute(Command::Batch {
            commands: vec![
                Command::SetTimeSignatureDisplay {
                    measure: 0,
                    display: TimeSignatureDisplay::Common,
                },
                Command::SetTimeSignature {
                    measure: 1,
                    time_signature: TimeSignature {
                        numerator: 2,
                        denominator: 2,
                    },
                },
                Command::SetTimeSignatureDisplay {
                    measure: 1,
                    display: TimeSignatureDisplay::AllaBreve,
                },
            ],
        })
        .unwrap();

    let list = b.layout(100.0);
    let common = list
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.shape,
                Shape::Glyph {
                    glyph: '\u{E08A}',
                    ..
                }
            )
        })
        .count();
    let cut = list
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.shape,
                Shape::Glyph {
                    glyph: '\u{E08B}',
                    ..
                }
            )
        })
        .count();
    let numeric_digits = list
        .items
        .iter()
        .filter(|item| {
            matches!(item.shape, Shape::Glyph { glyph, .. } if ('\u{E080}'..='\u{E089}').contains(&glyph))
        })
        .count();

    assert_eq!(common, 1);
    assert_eq!(cut, 1);
    assert_eq!(numeric_digits, 0);
}

#[test]
fn multi_staff_parts_get_full_and_short_names_across_systems() {
    let mut b = Builder::new(&[
        ("Piano", &[Clef::Treble, Clef::Bass]),
        ("Violin", &[Clef::Treble]),
    ]);
    b.measures(10);
    let list = b.layout(60.0);

    let braces = list
        .items
        .iter()
        .filter(|item| matches!(item.shape, Shape::Glyph { glyph, .. } if glyph == '\u{E000}'))
        .count();
    let names: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match &item.shape {
            Shape::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let systems = staff_lines(&list).len() / 15;
    assert!(systems > 1);
    assert_eq!(braces, systems);
    assert_eq!(names.len(), systems * 2);
    assert_eq!(&names[..2], &["Piano", "Violin"]);
    assert!(
        names[2..]
            .chunks_exact(2)
            .all(|pair| pair == ["Pno.", "Vln."])
    );
}

#[test]
fn tiny_widths_do_not_panic_and_keep_one_measure_per_system() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.measures(3);
    let list = b.layout(1.0);
    assert_eq!(staff_lines(&list).len(), 15);
}

#[test]
fn layout_is_deterministic_and_serializes() {
    let mut b = Builder::new(&[("", &[Clef::Treble])]);
    b.note(0, 0, c4(), NoteValue::Quarter);
    let first = b.layout(100.0);
    let second = b.layout(100.0);
    assert_eq!(first, second);

    let json = serde_json::to_value(&first).unwrap();
    let item = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "glyph" && item.get("event").is_some())
        .expect("notehead item");
    assert!(item["glyph"].is_string());
}

#[test]
fn tempo_directions_render_at_the_same_position_for_each_part() {
    let mut builder = Builder::new(&[("Flute", &[Clef::Treble]), ("Cello", &[Clef::Bass])]);
    builder
        .doc
        .execute(Command::SetTempoDirection {
            measure: 0,
            tick: 0,
            direction: Some(notula_model::TempoDirection::Ritardando),
        })
        .unwrap();

    let directions: Vec<_> = builder
        .layout(100.0)
        .items
        .into_iter()
        .filter_map(|item| match item.shape {
            Shape::Text { x, y, text, .. } if text == "rit." => Some((x, y)),
            _ => None,
        })
        .collect();
    assert_eq!(directions.len(), 2);
    assert_eq!(directions[0].0, directions[1].0);
    assert_ne!(directions[0].1, directions[1].1);
}

#[test]
fn tempo_changes_render_a_metronome_mark_above_each_part() {
    let mut builder = Builder::new(&[("Flute", &[Clef::Treble]), ("Cello", &[Clef::Bass])]);
    let tempo = notula_model::Tempo {
        beat_unit: NoteValue::Quarter,
        beat_unit_dots: 1,
        units_per_minute: 72,
    };
    builder
        .doc
        .execute(Command::SetTempoChange {
            measure: 0,
            tick: 960,
            tempo: Some(tempo),
            hidden: false,
        })
        .unwrap();
    let items = builder.layout(100.0).items;

    let notes: Vec<_> = items
        .iter()
        .filter_map(|item| match item.shape {
            Shape::Glyph {
                x,
                y,
                glyph: '\u{ECA5}',
                ..
            } => Some((x, y)),
            _ => None,
        })
        .collect();
    let dots = items
        .iter()
        .filter(|item| {
            matches!(
                item.shape,
                Shape::Glyph {
                    glyph: '\u{ECB7}',
                    ..
                }
            )
        })
        .count();
    let texts: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.shape {
            Shape::Text { x, text, .. } if text == "= 72" => Some(*x),
            _ => None,
        })
        .collect();
    assert_eq!(notes.len(), 2);
    assert_eq!(dots, 2);
    assert_eq!(texts.len(), 2);
    assert_eq!(notes[0].0, notes[1].0);
    assert!(notes[0].1 < notes[1].1);
    assert!(texts[0] > notes[0].0);
}

#[test]
fn metronome_marks_clear_high_notes_beneath_them() {
    let mark_y = |pitch: Pitch| {
        let mut builder = Builder::new(&[("Flute", &[Clef::Treble])]);
        builder.note(0, 0, pitch, NoteValue::Quarter);
        builder
            .doc
            .execute(Command::SetTempoChange {
                measure: 0,
                tick: 0,
                tempo: Some(notula_model::Tempo::default()),
                hidden: false,
            })
            .unwrap();
        builder
            .layout(100.0)
            .items
            .into_iter()
            .find_map(|item| match item.shape {
                Shape::Text { y, text, .. } if text == "= 120" => Some(y),
                _ => None,
            })
            .unwrap()
    };
    // A ledger-line A6 reaches about four spaces higher than a G4 inside the staff.
    assert!(mark_y(Pitch::new(Step::A, 0, 6)) < mark_y(Pitch::new(Step::G, 0, 4)) - 3.0);
}

#[test]
fn hidden_tempo_changes_are_not_printed() {
    let mut builder = Builder::new(&[("Flute", &[Clef::Treble])]);
    builder
        .doc
        .execute(Command::SetTempoChange {
            measure: 0,
            tick: 0,
            tempo: Some(notula_model::Tempo::default()),
            hidden: true,
        })
        .unwrap();
    assert!(builder.layout(100.0).items.iter().all(|item| !matches!(
        &item.shape,
        Shape::Text { text, .. } if text.starts_with('=')
    )));
}

fn hairpin_lines(list: &RenderList, start: EventId) -> Vec<(f32, f32, f32, f32)> {
    list.items
        .iter()
        .filter(|item| item.event == Some(start))
        .filter_map(|item| match item.shape {
            // Stems carry the event too, but are thinner.
            Shape::Line {
                x1,
                y1,
                x2,
                y2,
                thickness,
            } if thickness > 0.15 => Some((x1, y1, x2, y2)),
            _ => None,
        })
        .collect()
}

#[test]
fn dynamics_and_hairpins_share_a_line_below_the_staff() {
    let mut builder = Builder::new(&[("Flute", &[Clef::Treble])]);
    let ids: Vec<_> = (0..4)
        .map(|_| builder.note(0, 0, Pitch::new(Step::G, 0, 4), NoteValue::Quarter))
        .collect();
    edit::set_dynamic(&mut builder.doc, ids[0], Some(notula_model::Dynamic::P)).unwrap();
    edit::toggle_hairpin(
        &mut builder.doc,
        ids[0],
        ids[1],
        notula_model::Hairpin::Crescendo,
    )
    .unwrap();
    edit::toggle_hairpin(
        &mut builder.doc,
        ids[1],
        ids[2],
        notula_model::Hairpin::Diminuendo,
    )
    .unwrap();
    edit::set_dynamic(&mut builder.doc, ids[3], Some(notula_model::Dynamic::F)).unwrap();
    builder
        .doc
        .execute(Command::SetTempoDirection {
            measure: 0,
            tick: 0,
            direction: Some(notula_model::TempoDirection::Ritardando),
        })
        .unwrap();
    let list = builder.layout(100.0);

    let dynamics: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item.shape {
            Shape::Glyph { x, y, glyph, .. } if ('\u{E520}'..='\u{E53D}').contains(&glyph) => {
                Some((item.event, x, y))
            }
            _ => None,
        })
        .collect();
    assert_eq!(dynamics.len(), 2);
    assert_eq!(dynamics[0].0, Some(ids[0]));
    assert_eq!(dynamics[0].2, dynamics[1].2);
    assert!(dynamics[0].2 > 4.0, "dynamics sit below the staff");

    let crescendo = hairpin_lines(&list, ids[0]);
    let diminuendo = hairpin_lines(&list, ids[1]);
    assert_eq!((crescendo.len(), diminuendo.len()), (2, 2));
    // A crescendo starts closed and opens; the diminuendo continues from its open end.
    let (x1, y1, x2, y2) = crescendo[0];
    let (_, other_y1, _, other_y2) = crescendo[1];
    assert_eq!(y1, other_y1);
    assert!((y2 - other_y2).abs() > 0.5);
    assert!(x1 > dynamics[0].1, "the crescendo starts after the p");
    assert_eq!(diminuendo[0].0, x2);
    assert!(
        diminuendo[0].2 < dynamics[1].1,
        "the diminuendo stops before the f"
    );
    assert!((y1 - dynamics[0].2).abs() < 1.0);

    let rit = list
        .items
        .iter()
        .find_map(|item| match &item.shape {
            Shape::Text { y, text, .. } if text == "rit." => Some(*y),
            _ => None,
        })
        .unwrap();
    assert!(
        rit > dynamics[0].2 + 1.0,
        "tempo text moves below the dynamics"
    );
}

#[test]
fn hairpins_continue_across_system_breaks() {
    let mut builder = Builder::new(&[("Flute", &[Clef::Treble])]);
    builder.measures(8);
    let first = builder.note(0, 0, Pitch::new(Step::G, 0, 4), NoteValue::Whole);
    for measure in 1..7 {
        builder.note(0, measure, Pitch::new(Step::G, 0, 4), NoteValue::Whole);
    }
    let last = builder.note(0, 7, Pitch::new(Step::G, 0, 4), NoteValue::Whole);
    edit::toggle_hairpin(
        &mut builder.doc,
        first,
        last,
        notula_model::Hairpin::Crescendo,
    )
    .unwrap();
    let list = builder.layout(40.0);

    let segments = hairpin_lines(&list, first);
    assert!(segments.len() >= 4, "one pair of lines per system");
    let openings: Vec<f32> = segments
        .chunks(2)
        .map(|pair| (pair[0].1 - pair[1].1).abs())
        .collect();
    assert_eq!(openings[0], 0.0);
    // Each later system starts as open as the previous one ended.
    for (pair, next) in segments.chunks(2).zip(segments.chunks(2).skip(1)) {
        let end = (pair[0].3 - pair[1].3).abs();
        let start = (next[0].1 - next[1].1).abs();
        assert!((end - start).abs() < 0.05);
    }
}

#[test]
fn a_box_selects_the_staves_and_note_columns_it_covers() {
    use notula_layout::box_selection;
    use notula_model::selection::Span;
    let mut builder = Builder::new(&[("Piano", &[Clef::Treble, Clef::Bass])]);
    builder.measures(2);
    let quarters: Vec<_> = (0..4)
        .map(|_| builder.note(0, 0, Pitch::new(Step::C, 0, 5), NoteValue::Quarter))
        .collect();
    builder.note(1, 0, Pitch::new(Step::C, 0, 3), NoteValue::Whole);
    let staves = builder.staves.clone();
    let list = builder.layout(100.0);
    let area = |staff, measure| {
        list.areas
            .iter()
            .find(|area| area.staff == staff && area.measure == measure)
            .unwrap()
            .clone()
    };
    let upper = area(staves[0], 0);
    let lower = area(staves[1], 0);
    let x = |tick: u32| {
        upper
            .columns
            .iter()
            .find(|column| column.0 == tick)
            .unwrap()
            .1
    };

    // Around the second and third quarter of the upper staff only.
    let around_two = box_selection(
        builder.doc.score(),
        &list,
        (x(960) - 0.5, upper.y_top - 1.0),
        (x(1920) + 0.5, upper.y_top + 5.0),
    )
    .unwrap();
    let resolved = around_two.resolve(builder.doc.score()).unwrap();
    assert_eq!(resolved.staves, vec![staves[0]]);
    assert_eq!(
        resolved.span,
        Span {
            start: 960,
            end: 2880
        }
    );
    assert_eq!(resolved.events, vec![quarters[1], quarters[2]]);

    // Dragged from the lower staff up and from right to left: both staves; the lower staff's
    // whole note starts at 0, outside the box, so it is not selected. The cursor goes to the
    // left edge, where the drag ended.
    let both = box_selection(
        builder.doc.score(),
        &list,
        (x(1920) + 0.5, lower.y_top + 4.0),
        (x(960) - 0.5, upper.y_top),
    )
    .unwrap();
    let resolved = both.resolve(builder.doc.score()).unwrap();
    assert_eq!(resolved.staves, staves);
    assert_eq!(
        (resolved.span, resolved.caret),
        (
            Span {
                start: 960,
                end: 2880
            },
            960
        )
    );
    assert_eq!(around_two.resolve(builder.doc.score()).unwrap().caret, 2880);

    // An empty measure counts in full.
    let empty = area(staves[0], 1);
    let in_empty = box_selection(
        builder.doc.score(),
        &list,
        (empty.x_start + 1.0, empty.y_top + 1.0),
        (empty.x_start + 2.0, empty.y_top + 2.0),
    )
    .unwrap();
    assert_eq!(
        in_empty.span(),
        Span {
            start: 3840,
            end: 7680
        }
    );

    // Empty paper selects nothing.
    assert!(box_selection(builder.doc.score(), &list, (0.0, 0.0), (1.0, 1.0)).is_none());
}
