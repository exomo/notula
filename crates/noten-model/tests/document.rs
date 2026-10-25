use noten_model::{
    Clef, Command, Document, Duration, Event, EventKind, ModelError, NoteValue, Pitch, Score,
    StaffId, Step, TimeSignature, VoiceLocation,
};

fn setup() -> (Document, StaffId) {
    let mut doc = Document::new();
    let part = doc.new_part("Piano", &[Clef::Treble]);
    let staff = part.staves[0].id;
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    (doc, staff)
}

fn note(doc: &mut Document, value: NoteValue) -> Event {
    Event::new(
        doc.new_event_id(),
        Duration::new(value, 0),
        EventKind::Chord {
            pitches: vec![Pitch::new(Step::C, 0, 4)],
            tied: false,
        },
    )
}

fn at(staff: StaffId, measure: usize) -> VoiceLocation {
    VoiceLocation {
        staff,
        measure,
        voice: 0,
    }
}

fn insert(
    doc: &mut Document,
    staff: StaffId,
    index: usize,
    value: NoteValue,
) -> Result<(), ModelError> {
    let event = note(doc, value);
    doc.execute(Command::InsertEvent {
        at: at(staff, 0),
        index,
        event,
    })
}

#[test]
fn insert_then_undo_and_redo() {
    let (mut doc, staff) = setup();
    let before = doc.score().clone();

    insert(&mut doc, staff, 0, NoteValue::Quarter).unwrap();
    let after = doc.score().clone();
    assert_ne!(before, after);

    assert!(doc.undo().unwrap());
    assert_eq!(doc.score(), &before);
    assert!(doc.redo().unwrap());
    assert_eq!(doc.score(), &after);
}

#[test]
fn new_edit_clears_redo() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Quarter).unwrap();
    doc.undo().unwrap();
    assert!(doc.can_redo());
    insert(&mut doc, staff, 0, NoteValue::Half).unwrap();
    assert!(!doc.can_redo());
}

#[test]
fn undo_with_empty_history_is_a_noop() {
    let mut doc = Document::new();
    assert!(!doc.undo().unwrap());
    assert!(!doc.redo().unwrap());
}

#[test]
fn measure_cannot_overflow() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Whole).unwrap();
    let snapshot = doc.score().clone();
    assert_eq!(
        insert(&mut doc, staff, 1, NoteValue::Quarter),
        Err(ModelError::MeasureOverfull)
    );
    assert_eq!(doc.score(), &snapshot);
}

#[test]
fn replace_event_checks_capacity_and_undoes() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Half).unwrap();
    insert(&mut doc, staff, 1, NoteValue::Half).unwrap();
    let before = doc.score().clone();

    let too_long = note(&mut doc, NoteValue::Whole);
    let result = doc.execute(Command::ReplaceEvent {
        at: at(staff, 0),
        index: 0,
        event: too_long,
    });
    assert_eq!(result, Err(ModelError::MeasureOverfull));

    let shorter = note(&mut doc, NoteValue::Quarter);
    doc.execute(Command::ReplaceEvent {
        at: at(staff, 0),
        index: 0,
        event: shorter,
    })
    .unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.score(), &before);
}

#[test]
fn invalid_events_are_rejected() {
    let (mut doc, staff) = setup();
    let empty_chord = Event::new(
        doc.new_event_id(),
        Duration::new(NoteValue::Quarter, 0),
        EventKind::Chord {
            pitches: vec![],
            tied: false,
        },
    );
    let result = doc.execute(Command::InsertEvent {
        at: at(staff, 0),
        index: 0,
        event: empty_chord,
    });
    assert!(matches!(result, Err(ModelError::Invalid(_))));
}

#[test]
fn batch_is_atomic_and_undone_as_one_step() {
    let (mut doc, staff) = setup();
    let before = doc.score().clone();

    let first = note(&mut doc, NoteValue::Half);
    let second = note(&mut doc, NoteValue::Whole);
    let failing = Command::Batch {
        commands: vec![
            Command::InsertEvent {
                at: at(staff, 0),
                index: 0,
                event: first.clone(),
            },
            Command::InsertEvent {
                at: at(staff, 0),
                index: 1,
                event: second,
            },
        ],
    };
    assert_eq!(doc.execute(failing), Err(ModelError::MeasureOverfull));
    assert_eq!(doc.score(), &before);

    let third = note(&mut doc, NoteValue::Half);
    doc.execute(Command::Batch {
        commands: vec![
            Command::InsertEvent {
                at: at(staff, 0),
                index: 0,
                event: first,
            },
            Command::InsertEvent {
                at: at(staff, 0),
                index: 1,
                event: third,
            },
        ],
    })
    .unwrap();
    assert!(doc.undo().unwrap());
    assert_eq!(doc.score(), &before);
}

#[test]
fn measures_can_be_inserted_removed_and_restored() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Quarter).unwrap();

    let blank = doc.score().blank_measure(1);
    doc.execute(Command::InsertMeasure {
        index: 1,
        data: blank,
    })
    .unwrap();
    assert_eq!(doc.score().measures.len(), 2);

    let with_content = doc.score().clone();
    doc.execute(Command::RemoveMeasure { index: 0 }).unwrap();
    assert_eq!(doc.score().measures.len(), 1);
    doc.undo().unwrap();
    assert_eq!(doc.score(), &with_content);

    doc.execute(Command::RemoveMeasure { index: 1 }).unwrap();
    assert!(matches!(
        doc.execute(Command::RemoveMeasure { index: 0 }),
        Err(ModelError::Invalid(_))
    ));
}

#[test]
fn time_signature_change_must_fit_existing_content() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Half).unwrap();
    let three_four = TimeSignature {
        numerator: 3,
        denominator: 4,
    };
    assert_eq!(
        doc.execute(Command::SetTimeSignature {
            measure: 0,
            time_signature: TimeSignature {
                numerator: 1,
                denominator: 4
            }
        }),
        Err(ModelError::MeasureOverfull)
    );
    doc.execute(Command::SetTimeSignature {
        measure: 0,
        time_signature: three_four,
    })
    .unwrap();
    assert_eq!(doc.score().measures[0].time_signature, three_four);
    doc.undo().unwrap();
    assert_eq!(
        doc.score().measures[0].time_signature,
        TimeSignature::COMMON
    );
}

#[test]
fn parts_can_be_added_and_removed_with_undo() {
    let (mut doc, _) = setup();
    let strings = doc.new_part("Violin", &[Clef::Treble]);
    let id = strings.id;
    doc.execute(Command::AddPart {
        index: 0,
        part: strings.clone(),
    })
    .unwrap();
    assert_eq!(doc.score().parts[0].id, id);

    assert!(matches!(
        doc.execute(Command::AddPart {
            index: 0,
            part: strings
        }),
        Err(ModelError::DuplicateId(_))
    ));

    let both = doc.score().clone();
    doc.execute(Command::RemovePart { part: id }).unwrap();
    assert_eq!(doc.score().parts.len(), 1);
    doc.undo().unwrap();
    assert_eq!(doc.score(), &both);
}

#[test]
fn part_properties_can_change_without_losing_notes_and_undo_together() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Quarter).unwrap();
    let before = doc.score().clone();
    let part = doc.score().parts[0].id;

    doc.execute(Command::Batch {
        commands: vec![
            Command::SetMetadata {
                title: "Revised title".into(),
                composer: "Composer".into(),
            },
            Command::SetPartName {
                part,
                name: "Viola".into(),
            },
            Command::SetPartShortName {
                part,
                short_name: "Vla.".into(),
            },
            Command::SetClef {
                staff,
                clef: Clef::Alto,
            },
        ],
    })
    .unwrap();

    let updated = doc.score();
    assert_eq!(updated.title, "Revised title");
    assert_eq!(updated.parts[0].name, "Viola");
    assert_eq!(updated.parts[0].short_name, "Vla.");
    assert_eq!(updated.parts[0].staves[0].id, staff);
    assert_eq!(
        updated.parts[0].staves[0].measures,
        before.parts[0].staves[0].measures
    );
    doc.undo().unwrap();
    assert_eq!(doc.score(), &before);
}

#[test]
fn clef_key_and_metadata_roundtrip() {
    let (mut doc, staff) = setup();
    let before = doc.score().clone();
    doc.execute(Command::SetClef {
        staff,
        clef: Clef::Bass,
    })
    .unwrap();
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature: noten_model::KeySignature { fifths: 2 },
    })
    .unwrap();
    doc.execute(Command::SetMetadata {
        title: "Etude".into(),
        composer: "Anon".into(),
    })
    .unwrap();
    assert!(
        doc.execute(Command::SetKeySignature {
            measure: 0,
            key_signature: noten_model::KeySignature { fifths: 9 },
        })
        .is_err()
    );

    for _ in 0..3 {
        assert!(doc.undo().unwrap());
    }
    assert_eq!(doc.score(), &before);
}

#[test]
fn score_and_commands_survive_json() {
    let (mut doc, staff) = setup();
    insert(&mut doc, staff, 0, NoteValue::Quarter).unwrap();

    let json = serde_json::to_string(doc.score()).unwrap();
    let restored: Score = serde_json::from_str(&json).unwrap();
    assert_eq!(&restored, doc.score());

    let command = Command::Batch {
        commands: vec![
            Command::SetMetadata {
                title: "T".into(),
                composer: "C".into(),
            },
            Command::SetPartShortName {
                part: doc.score().parts[0].id,
                short_name: "Pno.".into(),
            },
        ],
    };
    let json = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
}

#[test]
fn old_scores_without_part_short_names_still_load() {
    let (doc, _) = setup();
    let mut json = serde_json::to_value(doc.score()).unwrap();
    json["parts"][0]
        .as_object_mut()
        .unwrap()
        .remove("short_name");

    let restored: Score = serde_json::from_value(json).unwrap();
    assert_eq!(restored.parts[0].short_name, "");
}

#[test]
fn restored_score_keeps_allocating_unique_ids() {
    let (mut doc, _) = setup();
    let last = doc.new_event_id();
    let json = serde_json::to_string(doc.score()).unwrap();
    let mut loaded = Document::from_score(serde_json::from_str(&json).unwrap());
    assert!(loaded.new_event_id() > last);
}
