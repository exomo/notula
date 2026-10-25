//! Creating new scores from the setup dialog.

use std::collections::HashSet;

use notula_model::{
    Clef, Command, Document, KeySignature, ModelError, PartId, Score, Tempo, TimeSignature,
};
use serde::{Deserialize, Serialize};

const MAX_MEASURES: usize = 999;
const MAX_INSTRUMENTS: usize = 64;

struct Preset {
    id: &'static str,
    name: &'static str,
    short_name: &'static str,
    clefs: &'static [Clef],
}

// Instruments sound as written; transposition is not supported yet.
const PRESETS: &[Preset] = &[
    Preset {
        id: "piano",
        name: "Piano",
        short_name: "Pno.",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "organ",
        name: "Organ",
        short_name: "Org.",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "harp",
        name: "Harp",
        short_name: "Hp.",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "accordion",
        name: "Accordion",
        short_name: "Acc.",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "bass-accordion",
        name: "Bass accordion",
        short_name: "Bass acc.",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "flute",
        name: "Flute",
        short_name: "Fl.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "oboe",
        name: "Oboe",
        short_name: "Ob.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "clarinet",
        name: "Clarinet",
        short_name: "Cl.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "bassoon",
        name: "Bassoon",
        short_name: "Bsn.",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "horn",
        name: "Horn",
        short_name: "Hn.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "trumpet",
        name: "Trumpet",
        short_name: "Tpt.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "trombone",
        name: "Trombone",
        short_name: "Tbn.",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "violin",
        name: "Violin",
        short_name: "Vln.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "viola",
        name: "Viola",
        short_name: "Vla.",
        clefs: &[Clef::Alto],
    },
    Preset {
        id: "cello",
        name: "Cello",
        short_name: "Vc.",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "double-bass",
        name: "Double Bass",
        short_name: "Db.",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "guitar",
        name: "Guitar",
        short_name: "Gtr.",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "voice",
        name: "Voice",
        short_name: "Vo.",
        clefs: &[Clef::Treble],
    },
];

#[derive(Debug, Serialize)]
pub struct InstrumentInfo {
    id: &'static str,
    name: &'static str,
    short_name: &'static str,
    staves: usize,
}

pub fn instruments() -> Vec<InstrumentInfo> {
    PRESETS
        .iter()
        .map(|p| InstrumentInfo {
            id: p.id,
            name: p.name,
            short_name: p.short_name,
            staves: p.clefs.len(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct ScoreSetup {
    pub title: String,
    pub composer: String,
    /// Preset ids, top to bottom.
    pub instruments: Vec<InstrumentSelection>,
    pub time_numerator: u8,
    pub time_denominator: u8,
    pub key_fifths: i8,
    pub measures: usize,
    /// Length in ticks of a shortened first measure (anacrusis).
    #[serde(default)]
    pub pickup: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct InstrumentSelection {
    pub preset_id: String,
    pub staves: usize,
}

#[derive(Debug, Serialize)]
pub struct ScoreProperties {
    pub title: String,
    pub composer: String,
    pub tempo: Tempo,
    pub parts: Vec<PartProperties>,
}

#[derive(Debug, Serialize)]
pub struct PartProperties {
    pub id: PartId,
    pub name: String,
    pub short_name: String,
    pub staves: usize,
}

#[derive(Debug, Deserialize)]
pub struct PartInstrumentUpdate {
    pub part_id: PartId,
    pub name: String,
    pub short_name: String,
    pub preset_id: Option<String>,
    pub staves: usize,
}

#[derive(Debug, Deserialize)]
pub struct AddedInstrument {
    pub preset_id: String,
    pub name: String,
    pub short_name: String,
    pub staves: usize,
}

#[derive(Debug, Deserialize)]
pub struct ScorePropertiesUpdate {
    pub title: String,
    pub composer: String,
    pub tempo: Tempo,
    pub remove_part_ids: Vec<PartId>,
    pub instruments: Vec<PartInstrumentUpdate>,
    pub add_instruments: Vec<AddedInstrument>,
}

pub fn properties(score: &Score) -> ScoreProperties {
    ScoreProperties {
        title: score.title.clone(),
        composer: score.composer.clone(),
        tempo: score.tempo,
        parts: score
            .parts
            .iter()
            .map(|part| PartProperties {
                id: part.id,
                name: part.name.clone(),
                short_name: if part.short_name.is_empty() {
                    PRESETS
                        .iter()
                        .find(|preset| preset.name == part.name)
                        .map_or_else(
                            || part.short_name.clone(),
                            |preset| preset.short_name.to_owned(),
                        )
                } else {
                    part.short_name.clone()
                },
                staves: part.staves.len(),
            })
            .collect(),
    }
}

pub fn update_properties(
    doc: &mut Document,
    update: ScorePropertiesUpdate,
) -> Result<(), ModelError> {
    let parts = doc.score().parts.clone();
    let title = doc.score().title.clone();
    let composer = doc.score().composer.clone();
    update.tempo.validate()?;
    let tempo = doc.score().tempo;
    if update.instruments.len() + update.add_instruments.len() > MAX_INSTRUMENTS {
        return Err(ModelError::Invalid(
            "score parts changed while editing properties",
        ));
    }
    if update.instruments.is_empty() && update.add_instruments.is_empty() {
        return Err(ModelError::Invalid("a score needs at least one instrument"));
    }

    let mut seen = HashSet::new();
    let mut changes = Vec::new();
    for instrument in &update.instruments {
        if instrument.name.trim().is_empty() {
            return Err(ModelError::Invalid("instrument name cannot be empty"));
        }
        if !seen.insert(instrument.part_id) {
            return Err(ModelError::Invalid("duplicate score part"));
        }
        if !parts.iter().any(|part| part.id == instrument.part_id) {
            return Err(ModelError::Invalid(
                "score parts changed while editing properties",
            ));
        }
        if !(1..=2).contains(&instrument.staves) {
            return Err(ModelError::Invalid(
                "an instrument must have one or two staves",
            ));
        }
        let preset = instrument
            .preset_id
            .as_ref()
            .map(|preset_id| {
                PRESETS
                    .iter()
                    .find(|preset| preset.id == preset_id)
                    .ok_or(ModelError::Invalid("unknown instrument"))
            })
            .transpose()?;
        if preset.is_some_and(|preset| instrument.staves > preset.clefs.len()) {
            return Err(ModelError::Invalid(
                "the selected instrument does not support that many staves",
            ));
        }
        changes.push((instrument.part_id, preset, instrument.staves));
    }
    let mut removed = HashSet::new();
    for part_id in &update.remove_part_ids {
        if !removed.insert(*part_id) || seen.contains(part_id) {
            return Err(ModelError::Invalid("duplicate score part"));
        }
        if !parts.iter().any(|part| part.id == *part_id) {
            return Err(ModelError::Invalid(
                "score parts changed while editing properties",
            ));
        }
    }
    if seen.len() + removed.len() != parts.len() {
        return Err(ModelError::Invalid(
            "score parts changed while editing properties",
        ));
    }
    let additions: Vec<_> = update
        .add_instruments
        .iter()
        .map(|addition| {
            if addition.name.trim().is_empty() {
                return Err(ModelError::Invalid("instrument name cannot be empty"));
            }
            let preset = PRESETS
                .iter()
                .find(|preset| preset.id == addition.preset_id)
                .ok_or(ModelError::Invalid("unknown instrument"))?;
            if !(1..=2).contains(&addition.staves) || addition.staves > preset.clefs.len() {
                return Err(ModelError::Invalid(
                    "the selected instrument does not support that many staves",
                ));
            }
            Ok((
                preset,
                addition.name.trim(),
                addition.short_name.trim(),
                addition.staves,
            ))
        })
        .collect::<Result<_, _>>()?;

    let mut commands = Vec::new();
    if title != update.title.trim() || composer != update.composer.trim() {
        commands.push(Command::SetMetadata {
            title: update.title.trim().to_owned(),
            composer: update.composer.trim().to_owned(),
        });
    }
    if tempo != update.tempo {
        commands.push(Command::SetTempo {
            tempo: update.tempo,
        });
    }
    for instrument in &update.instruments {
        let part = parts
            .iter()
            .find(|part| part.id == instrument.part_id)
            .unwrap();
        let name = instrument.name.trim();
        if part.name != name {
            commands.push(Command::SetPartName {
                part: instrument.part_id,
                name: name.to_owned(),
            });
        }
        let short_name = instrument.short_name.trim();
        if part.short_name != short_name {
            commands.push(Command::SetPartShortName {
                part: instrument.part_id,
                short_name: short_name.to_owned(),
            });
        }
    }

    for (part_id, preset, staff_count) in changes {
        let part = parts.iter().find(|part| part.id == part_id).unwrap();
        let mut staves: Vec<_> = part.staves.iter().take(staff_count).cloned().collect();
        if let Some(preset) = preset {
            for (staff, clef) in staves.iter_mut().zip(preset.clefs) {
                staff.clef = *clef;
            }
        }
        if staff_count > staves.len() {
            let extra_clefs = preset.map_or_else(
                || vec![Clef::Bass; staff_count - staves.len()],
                |preset| preset.clefs[staves.len()..staff_count].to_vec(),
            );
            let extra = doc.new_part(&part.name, &extra_clefs);
            staves.extend(extra.staves);
        }
        if staves != part.staves {
            commands.push(Command::SetPartStaves {
                part: part_id,
                staves,
            });
        }
    }

    for part_id in &update.remove_part_ids {
        commands.push(Command::RemovePart { part: *part_id });
    }

    let mut ordered_ids: Vec<_> = parts
        .iter()
        .filter(|part| !removed.contains(&part.id))
        .map(|part| part.id)
        .collect();
    for (index, instrument) in update.instruments.iter().enumerate() {
        let current_index = ordered_ids
            .iter()
            .position(|part_id| *part_id == instrument.part_id)
            .unwrap();
        if current_index != index {
            commands.push(Command::MovePart {
                part: instrument.part_id,
                index,
            });
            let part_id = ordered_ids.remove(current_index);
            ordered_ids.insert(index, part_id);
        }
    }

    for (index, (preset, name, short_name, staff_count)) in (ordered_ids.len()..).zip(additions) {
        let part = doc.new_part(name, &preset.clefs[..staff_count]);
        let part_id = part.id;
        commands.push(Command::AddPart { index, part });
        commands.push(Command::SetPartShortName {
            part: part_id,
            short_name: if short_name.is_empty() {
                preset.short_name.to_owned()
            } else {
                short_name.to_owned()
            },
        });
    }
    if !commands.is_empty() {
        doc.execute(Command::Batch { commands })?;
    }
    Ok(())
}

pub fn build(setup: &ScoreSetup) -> Result<Document, ModelError> {
    if setup.instruments.is_empty() || setup.instruments.len() > MAX_INSTRUMENTS {
        return Err(ModelError::Invalid("choose between 1 and 64 instruments"));
    }
    if !(1..=MAX_MEASURES).contains(&setup.measures) {
        return Err(ModelError::Invalid("a score can have 1 to 999 measures"));
    }
    let time_signature = TimeSignature {
        numerator: setup.time_numerator,
        denominator: setup.time_denominator,
    };
    let key_signature = KeySignature {
        fifths: setup.key_fifths,
    };
    time_signature.validate()?;
    key_signature.validate()?;

    let mut doc = Document::new();
    for (index, selection) in setup.instruments.iter().enumerate() {
        let preset = PRESETS
            .iter()
            .find(|p| p.id == selection.preset_id)
            .ok_or(ModelError::Invalid("unknown instrument"))?;
        if !(1..=2).contains(&selection.staves) || selection.staves > preset.clefs.len() {
            return Err(ModelError::Invalid(
                "the selected instrument does not support that many staves",
            ));
        }
        let part = doc.new_part(preset.name, &preset.clefs[..selection.staves]);
        let part_id = part.id;
        doc.execute(Command::AddPart { index, part })?;
        doc.execute(Command::SetPartShortName {
            part: part_id,
            short_name: preset.short_name.to_owned(),
        })?;
    }
    doc.execute(Command::SetMetadata {
        title: setup.title.trim().to_owned(),
        composer: setup.composer.trim().to_owned(),
    })?;
    doc.execute(Command::SetTimeSignature {
        measure: 0,
        time_signature,
    })?;
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature,
    })?;
    if let Some(length) = setup.pickup {
        doc.execute(Command::SetMeasureLength {
            measure: 0,
            length: Some(length),
        })?;
    }
    for index in 1..setup.measures {
        let data = doc.score().blank_measure(index);
        doc.execute(Command::InsertMeasure { index, data })?;
    }
    // A fresh score has no history to undo.
    Ok(Document::from_score(doc.into_score()))
}

pub fn default_score() -> Score {
    let setup = ScoreSetup {
        title: String::new(),
        composer: String::new(),
        instruments: vec![InstrumentSelection {
            preset_id: "piano".to_owned(),
            staves: 2,
        }],
        time_numerator: 4,
        time_denominator: 4,
        key_fifths: 0,
        measures: 8,
        pickup: None,
    };
    build(&setup)
        .expect("the default setup is valid")
        .into_score()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notula_model::{Duration, Event, EventKind, NoteValue, Pitch, Step, VoiceLocation};

    fn setup() -> ScoreSetup {
        ScoreSetup {
            title: " Suite ".into(),
            composer: "Anon".into(),
            instruments: vec![
                InstrumentSelection {
                    preset_id: "flute".into(),
                    staves: 1,
                },
                InstrumentSelection {
                    preset_id: "piano".into(),
                    staves: 2,
                },
            ],
            time_numerator: 3,
            time_denominator: 4,
            key_fifths: -2,
            measures: 5,
            pickup: None,
        }
    }

    #[test]
    fn builds_a_valid_score_from_the_setup() {
        let doc = build(&setup()).unwrap();
        let score = doc.score();
        score.validate().unwrap();
        assert_eq!(score.title, "Suite");
        assert_eq!(score.parts.len(), 2);
        assert_eq!(score.staves().count(), 3);
        assert_eq!(score.measures.len(), 5);
        assert!(score.measures.iter().all(|m| m.key_signature.fifths == -2));
        assert!(
            score
                .measures
                .iter()
                .all(|m| m.time_signature.numerator == 3)
        );
        assert!(!doc.can_undo());
    }

    #[test]
    fn rejects_invalid_setups() {
        let mut empty = setup();
        empty.instruments.clear();
        assert!(build(&empty).is_err());

        let mut unknown = setup();
        unknown.instruments = vec![InstrumentSelection {
            preset_id: "kazoo".into(),
            staves: 1,
        }];
        assert!(build(&unknown).is_err());

        let mut time = setup();
        time.time_denominator = 3;
        assert!(build(&time).is_err());

        let mut key = setup();
        key.key_fifths = 8;
        assert!(build(&key).is_err());

        for measures in [0, 1000] {
            let mut count = setup();
            count.measures = measures;
            assert!(build(&count).is_err());
        }
    }

    #[test]
    fn a_pickup_only_shortens_the_first_measure() {
        let mut s = setup();
        s.pickup = Some(960);
        let doc = build(&s).unwrap();
        let measures = &doc.score().measures;
        assert_eq!(measures[0].length, Some(960));
        assert!(measures[1..].iter().all(|m| m.length.is_none()));
        doc.score().validate().unwrap();
        // 3/4 holds at most 2880 ticks, and lengths must be whole 64th notes.
        for invalid in [2880 + 960, 100, 0] {
            let mut s = setup();
            s.pickup = Some(invalid);
            assert!(build(&s).is_err(), "{invalid}");
        }
    }

    #[test]
    fn property_edits_preserve_existing_notes_and_add_blank_instruments() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![InstrumentSelection {
                preset_id: "flute".into(),
                staves: 1,
            }],
            ..setup()
        })
        .unwrap();
        let part_id = doc.score().parts[0].id;
        let staff_id = doc.score().parts[0].staves[0].id;
        let event = Event::new(
            doc.new_event_id(),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::C, 0, 4)],
                tied: false,
            },
        );
        doc.execute(Command::InsertEvent {
            at: VoiceLocation {
                staff: staff_id,
                measure: 0,
                voice: 0,
            },
            index: 0,
            event,
        })
        .unwrap();
        let existing_measure = doc.score().parts[0].staves[0].measures[0].clone();

        update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: "Revised".into(),
                composer: "Composer".into(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![PartInstrumentUpdate {
                    part_id,
                    name: "Flute solo".into(),
                    short_name: "Fl. solo".into(),
                    preset_id: Some("oboe".into()),
                    staves: 1,
                }],
                add_instruments: vec![AddedInstrument {
                    preset_id: "piano".into(),
                    name: "Piano".into(),
                    short_name: "Pno.".into(),
                    staves: 2,
                }],
            },
        )
        .unwrap();

        let score = doc.score();
        assert_eq!(score.title, "Revised");
        assert_eq!(score.parts[0].id, part_id);
        assert_eq!(score.parts[0].name, "Flute solo");
        assert_eq!(score.parts[0].short_name, "Fl. solo");
        assert_eq!(score.parts[0].staves[0].id, staff_id);
        assert_eq!(score.parts[0].staves[0].measures[0], existing_measure);
        assert_eq!(score.parts.len(), 2);
        assert_eq!(score.parts[1].short_name, "Pno.");
        assert!(
            score.parts[1]
                .staves
                .iter()
                .all(|staff| staff.measures[0].voices[0].events.is_empty())
        );

        doc.undo().unwrap();
        assert_eq!(doc.score().parts[0].staves[0].measures[0], existing_measure);
        assert_eq!(doc.score().parts.len(), 1);
    }

    #[test]
    fn reordering_instruments_keeps_notes_with_their_part_and_undoes() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![
                InstrumentSelection {
                    preset_id: "flute".into(),
                    staves: 1,
                },
                InstrumentSelection {
                    preset_id: "violin".into(),
                    staves: 1,
                },
            ],
            ..setup()
        })
        .unwrap();
        let original_parts = doc.score().parts.clone();
        let moved_part = &original_parts[1];
        let part_id = moved_part.id;
        let staff_id = moved_part.staves[0].id;
        let event = Event::new(
            doc.new_event_id(),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::D, 0, 5)],
                tied: false,
            },
        );
        doc.execute(Command::InsertEvent {
            at: VoiceLocation {
                staff: staff_id,
                measure: 0,
                voice: 0,
            },
            index: 0,
            event,
        })
        .unwrap();
        let entered_measure = doc.score().parts[1].staves[0].measures[0].clone();

        update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![
                    PartInstrumentUpdate {
                        part_id,
                        name: "Violin".into(),
                        short_name: "Vln.".into(),
                        preset_id: None,
                        staves: 1,
                    },
                    PartInstrumentUpdate {
                        part_id: original_parts[0].id,
                        name: "Flute".into(),
                        short_name: "Fl.".into(),
                        preset_id: None,
                        staves: 1,
                    },
                ],
                add_instruments: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(doc.score().parts[0].id, part_id);
        assert_eq!(doc.score().parts[0].staves[0].id, staff_id);
        assert_eq!(doc.score().parts[0].staves[0].measures[0], entered_measure);

        doc.undo().unwrap();
        assert_eq!(doc.score().parts[0].id, original_parts[0].id);
        assert_eq!(doc.score().parts[1].id, original_parts[1].id);
        assert_eq!(doc.score().parts[1].staves[0].measures[0], entered_measure);
    }

    #[test]
    fn removing_an_instrument_removes_its_notes_and_undo_restores_them() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![
                InstrumentSelection {
                    preset_id: "flute".into(),
                    staves: 1,
                },
                InstrumentSelection {
                    preset_id: "violin".into(),
                    staves: 1,
                },
            ],
            ..setup()
        })
        .unwrap();
        let removed_part = doc.score().parts[1].clone();
        let event = Event::new(
            doc.new_event_id(),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::A, 0, 4)],
                tied: false,
            },
        );
        doc.execute(Command::InsertEvent {
            at: VoiceLocation {
                staff: removed_part.staves[0].id,
                measure: 0,
                voice: 0,
            },
            index: 0,
            event,
        })
        .unwrap();
        let removed_measure = doc.score().parts[1].staves[0].measures[0].clone();
        let kept_part_id = doc.score().parts[0].id;

        update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: vec![removed_part.id],
                instruments: vec![PartInstrumentUpdate {
                    part_id: kept_part_id,
                    name: "Flute".into(),
                    short_name: "Fl.".into(),
                    preset_id: None,
                    staves: 1,
                }],
                add_instruments: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(doc.score().parts.len(), 1);
        assert_ne!(doc.score().parts[0].id, removed_part.id);
        let final_part_id = doc.score().parts[0].id;
        let score_after_removal = doc.score().clone();

        let result = update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: vec![final_part_id],
                instruments: Vec::new(),
                add_instruments: Vec::new(),
            },
        );
        assert!(result.is_err());
        assert_eq!(doc.score(), &score_after_removal);

        doc.undo().unwrap();
        assert_eq!(doc.score().parts[1].id, removed_part.id);
        assert_eq!(doc.score().parts[1].staves[0].measures[0], removed_measure);
    }

    #[test]
    fn property_edits_allow_duplicate_presets_with_distinct_names() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![InstrumentSelection {
                preset_id: "violin".into(),
                staves: 1,
            }],
            ..setup()
        })
        .unwrap();
        let part_id = doc.score().parts[0].id;

        update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![PartInstrumentUpdate {
                    part_id,
                    name: "Violin 1".into(),
                    short_name: "Vln. 1".into(),
                    preset_id: None,
                    staves: 1,
                }],
                add_instruments: vec![AddedInstrument {
                    preset_id: "violin".into(),
                    name: "Violin 2".into(),
                    short_name: "Vln. 2".into(),
                    staves: 1,
                }],
            },
        )
        .unwrap();

        let parts = &doc.score().parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "Violin 1");
        assert_eq!(parts[0].short_name, "Vln. 1");
        assert_eq!(parts[1].name, "Violin 2");
        assert_eq!(parts[1].short_name, "Vln. 2");
        assert_ne!(parts[0].id, parts[1].id);
        assert_ne!(parts[0].staves[0].id, parts[1].staves[0].id);
    }

    #[test]
    fn property_edits_reject_staff_count_reduction_without_mutation() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![InstrumentSelection {
                preset_id: "piano".into(),
                staves: 2,
            }],
            ..setup()
        })
        .unwrap();
        let part_id = doc.score().parts[0].id;
        let bass_staff = doc.score().parts[0].staves[1].id;
        let note = Event::new(
            doc.new_event_id(),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::C, 0, 3)],
                tied: false,
            },
        );
        doc.execute(Command::InsertEvent {
            at: VoiceLocation {
                staff: bass_staff,
                measure: 0,
                voice: 0,
            },
            index: 0,
            event: note,
        })
        .unwrap();
        let before = doc.score().clone();
        let can_undo_before = doc.can_undo();

        let result = update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: "Would be destructive".into(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![PartInstrumentUpdate {
                    part_id,
                    name: "Piano".into(),
                    short_name: "Pno.".into(),
                    preset_id: Some("flute".into()),
                    staves: 1,
                }],
                add_instruments: Vec::new(),
            },
        );

        assert!(result.is_err());
        assert_eq!(doc.score(), &before);
        assert_eq!(doc.can_undo(), can_undo_before);
    }

    #[test]
    fn accordion_supports_one_or_two_staves() {
        for (instrument, staff_count) in [("accordion", 1), ("accordion", 2), ("piano", 1)] {
            let doc = build(&ScoreSetup {
                instruments: vec![InstrumentSelection {
                    preset_id: instrument.into(),
                    staves: staff_count,
                }],
                ..setup()
            })
            .unwrap();
            assert_eq!(doc.score().parts[0].staves.len(), staff_count);
        }
    }

    #[test]
    fn bass_accordion_has_one_bass_staff() {
        let doc = build(&ScoreSetup {
            instruments: vec![InstrumentSelection {
                preset_id: "bass-accordion".into(),
                staves: 1,
            }],
            ..setup()
        })
        .unwrap();

        let part = &doc.score().parts[0];
        assert_eq!(part.name, "Bass accordion");
        assert_eq!(part.short_name, "Bass acc.");
        assert_eq!(part.staves.len(), 1);
        assert_eq!(part.staves[0].clef, Clef::Bass);
    }

    #[test]
    fn a_blank_second_staff_can_be_removed_but_a_written_staff_cannot() {
        let mut doc = build(&ScoreSetup {
            instruments: vec![InstrumentSelection {
                preset_id: "piano".into(),
                staves: 2,
            }],
            ..setup()
        })
        .unwrap();
        let part_id = doc.score().parts[0].id;
        let first_staff = doc.score().parts[0].staves[0].id;
        let second_staff = doc.score().parts[0].staves[1].id;
        update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![PartInstrumentUpdate {
                    part_id,
                    name: "Piano".into(),
                    short_name: "Pno.".into(),
                    preset_id: None,
                    staves: 1,
                }],
                add_instruments: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(doc.score().parts[0].staves.len(), 1);
        assert_eq!(doc.score().parts[0].staves[0].id, first_staff);
        doc.undo().unwrap();
        assert_eq!(doc.score().parts[0].staves[1].id, second_staff);

        let note = Event::new(
            doc.new_event_id(),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::C, 0, 4)],
                tied: false,
            },
        );
        doc.execute(Command::InsertEvent {
            at: VoiceLocation {
                staff: second_staff,
                measure: 0,
                voice: 0,
            },
            index: 0,
            event: note,
        })
        .unwrap();
        let before = doc.score().clone();
        let result = update_properties(
            &mut doc,
            ScorePropertiesUpdate {
                title: String::new(),
                composer: String::new(),
                tempo: Tempo::default(),
                remove_part_ids: Vec::new(),
                instruments: vec![PartInstrumentUpdate {
                    part_id,
                    name: "Piano".into(),
                    short_name: "Pno.".into(),
                    preset_id: None,
                    staves: 1,
                }],
                add_instruments: Vec::new(),
            },
        );
        assert!(result.is_err());
        assert_eq!(doc.score(), &before);
    }

    #[test]
    fn every_preset_can_be_built() {
        for preset in PRESETS {
            let mut s = setup();
            s.instruments = vec![InstrumentSelection {
                preset_id: preset.id.to_owned(),
                staves: preset.clefs.len(),
            }];
            let doc = build(&s).unwrap_or_else(|_| panic!("{}", preset.id));
            assert_eq!(doc.score().parts[0].short_name, preset.short_name);
        }
    }
}
