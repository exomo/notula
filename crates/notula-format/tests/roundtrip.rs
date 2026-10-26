use std::io::{Cursor, Write};

use notula_format::{FormatError, VERSION, load, read, save, write};
use notula_model::edit;
use notula_model::{
    Articulation, Clef, Command, Document, Duration, Dynamic, Event, EventKind, Hairpin,
    ModelError, NoteValue, Pitch, Score, Step, VoiceLocation,
};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn sample() -> Document {
    let mut doc = Document::new();
    let part = doc.new_part("Piano", &[Clef::Treble, Clef::Bass]);
    let staff = part.staves[0].id;
    doc.execute(Command::AddPart { index: 0, part }).unwrap();
    let event = Event::new(
        doc.new_event_id(),
        Duration::new(NoteValue::Quarter, 1),
        EventKind::Chord {
            pitches: vec![Pitch::new(Step::F, 1, 4)],
            tied: true,
        },
    );
    let at = VoiceLocation {
        staff,
        measure: 0,
        voice: 0,
    };
    doc.execute(Command::InsertEvent {
        at,
        index: 0,
        event,
    })
    .unwrap();
    doc
}

fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn manifest(format: &str, version: u32) -> String {
    format!(r#"{{"format":"{format}","version":{version}}}"#)
}

fn score_json(score: &Score) -> String {
    serde_json::to_string(score).unwrap()
}

#[test]
fn roundtrip_keeps_the_score() {
    let mut doc = sample();
    let mut bytes = Cursor::new(Vec::new());
    write(doc.score(), &mut bytes).unwrap();

    let restored = read(Cursor::new(bytes.into_inner())).unwrap();
    assert_eq!(&restored, doc.score());

    let mut reloaded = Document::from_score(restored);
    assert_eq!(reloaded.new_event_id(), doc.new_event_id());
}

#[test]
fn save_and_load_through_a_file() {
    let doc = sample();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("piece.notula");

    save(doc.score(), &path).unwrap();
    assert_eq!(&load(&path).unwrap(), doc.score());
    assert!(!dir.path().join("piece.notula.tmp").exists());

    save(doc.score(), &path).unwrap();
    assert_eq!(&load(&path).unwrap(), doc.score());
}

#[test]
fn failed_save_leaves_the_existing_file_alone() {
    let doc = sample();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("piece.notula");
    save(doc.score(), &path).unwrap();
    let before = std::fs::read(&path).unwrap();

    let missing = dir.path().join("no-such-folder").join("piece.notula");
    assert!(matches!(
        save(doc.score(), &missing),
        Err(FormatError::Io(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn rejects_files_that_are_not_notula_archives() {
    assert!(matches!(
        read(Cursor::new(b"plain text".to_vec())),
        Err(FormatError::Zip(_))
    ));

    let no_manifest = archive(&[("score.json", "{}")]);
    assert!(matches!(
        read(Cursor::new(no_manifest)),
        Err(FormatError::NotNotula)
    ));

    let doc = sample();
    let other = archive(&[
        ("manifest.json", &manifest("something-else", 1)),
        ("score.json", &score_json(doc.score())),
    ]);
    assert!(matches!(
        read(Cursor::new(other)),
        Err(FormatError::NotNotula)
    ));
}

#[test]
fn rejects_newer_versions() {
    let doc = sample();
    let future = archive(&[
        ("manifest.json", &manifest("notula", VERSION + 1)),
        ("score.json", &score_json(doc.score())),
    ]);
    assert!(matches!(
        read(Cursor::new(future)),
        Err(FormatError::UnsupportedVersion { .. })
    ));
}

#[test]
fn rejects_malformed_and_inconsistent_scores() {
    let broken = archive(&[
        ("manifest.json", &manifest("notula", VERSION)),
        ("score.json", "{ not json"),
    ]);
    assert!(matches!(
        read(Cursor::new(broken)),
        Err(FormatError::Json(_))
    ));

    // Two events sharing an id would break selection and undo.
    let mut doc = sample();
    let staff = doc.score().parts[0].staves[0].id;
    let existing = doc
        .score()
        .voice(VoiceLocation {
            staff,
            measure: 0,
            voice: 0,
        })
        .unwrap()
        .events[0]
        .clone();
    let duplicate = Event {
        duration: Duration::new(NoteValue::Eighth, 0),
        ..existing
    };
    doc.execute(Command::InsertEvent {
        at: VoiceLocation {
            staff,
            measure: 0,
            voice: 0,
        },
        index: 1,
        event: duplicate,
    })
    .unwrap();
    let corrupted = archive(&[
        ("manifest.json", &manifest("notula", VERSION)),
        ("score.json", &score_json(doc.score())),
    ]);
    assert!(matches!(
        read(Cursor::new(corrupted)),
        Err(FormatError::Invalid(ModelError::DuplicateId(_)))
    ));
}

#[test]
fn markings_roundtrip_through_a_file() {
    let mut doc = sample();
    let id = doc.score().parts[0].staves[0].measures[0].voices[0].events[0].id;
    edit::set_dynamic(&mut doc, id, Some(Dynamic::Ff)).unwrap();
    edit::toggle_hairpin(&mut doc, id, id, Hairpin::Crescendo).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Staccato).unwrap();
    edit::toggle_articulation(&mut doc, id, Articulation::Accent).unwrap();

    let mut bytes = Cursor::new(Vec::new());
    write(doc.score(), &mut bytes).unwrap();
    let restored = read(Cursor::new(bytes.into_inner())).unwrap();
    assert_eq!(&restored, doc.score());
    let event = &restored.parts[0].staves[0].measures[0].voices[0].events[0];
    assert_eq!(event.markings.dynamic, Some(Dynamic::Ff));
    assert_eq!(
        event.markings.articulations,
        vec![Articulation::Staccato, Articulation::Accent]
    );
}

#[test]
fn rejects_articulations_on_rests_in_a_file() {
    let doc = sample();
    let mut json = serde_json::to_value(doc.score()).unwrap();
    let event = &mut json["parts"][0]["staves"][0]["measures"][0]["voices"][0]["events"][0];
    event["kind"] = serde_json::json!({ "type": "rest" });
    event["markings"] = serde_json::json!({ "articulations": ["Staccato"] });

    let corrupted = archive(&[
        ("manifest.json", &manifest("notula", VERSION)),
        ("score.json", &json.to_string()),
    ]);
    assert!(matches!(
        read(Cursor::new(corrupted)),
        Err(FormatError::Invalid(ModelError::Invalid(_)))
    ));
}
