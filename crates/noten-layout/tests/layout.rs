use noten_layout::{Item, LayoutOptions, RenderList, Shape, hit_test, layout};
use noten_model::{
    Clef, Command, Document, Duration, Event, EventId, EventKind, KeySignature, NoteValue, Pitch,
    StaffId, Step, TimeSignature, VoiceLocation,
};

const STAFF_LINE: f32 = 0.13;
const SHARP: char = '\u{E262}';
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

fn glyph_x(list: &RenderList, event: EventId, glyph: char) -> f32 {
    list.items
        .iter()
        .find_map(|item| match (&item.shape, item.event) {
            (Shape::Glyph { x, glyph: g, .. }, Some(id)) if id == event && *g == glyph => Some(*x),
            _ => None,
        })
        .expect("glyph not found")
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
fn multi_staff_parts_get_a_brace_and_names_only_on_the_first_system() {
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
    let names = list
        .items
        .iter()
        .filter(|item| matches!(item.shape, Shape::Text { .. }))
        .count();
    let systems = staff_lines(&list).len() / 15;
    assert!(systems > 1);
    assert_eq!(braces, systems);
    assert_eq!(names, 2);
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
